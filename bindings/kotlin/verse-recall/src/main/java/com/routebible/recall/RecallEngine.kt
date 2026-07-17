package com.routebible.recall

import java.io.Closeable
import java.nio.ByteBuffer
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json

/**
 * Thread-safe owner of one native Verse Recall engine.
 *
 * Pack bytes are copied by the native C ABI during [open], so no input buffer is retained.
 */
public class RecallEngine private constructor(
    private var handle: Long,
    private val nativeBridge: NativeBridge,
    private val json: Json,
) : Closeable {
    /** Search with lexical, translation, concept, and other loaded non-host lanes. */
    @Synchronized
    public fun search(
        query: String,
        options: SearchOptions = SearchOptions(),
    ): SearchResponse {
        validateSearch(query, options)
        val activeHandle = requireOpen()
        val result = callNative("Native search failed") {
            nativeBridge.search(activeHandle, query, options)
        }
        return decodeSearch(requireSearchValue(result, "search"))
    }

    /**
     * Compatibility search for legacy LVEC v1 packs.
     *
     * New integrations should use the overload that supplies [EmbeddingSpaceId].
     * LVEC v2 packs deliberately reject this unverified path.
     */
    @Synchronized
    public fun searchWithEmbedding(
        query: String,
        embedding: FloatArray,
        options: SearchOptions = SearchOptions(),
    ): SearchResponse {
        validateSearch(query, options)
        validateEmbedding(embedding)
        val activeHandle = requireOpen()
        val result = callNative("Native semantic search failed") {
            nativeBridge.searchWithEmbedding(activeHandle, query, embedding, options)
        }
        return decodeSearch(requireSearchValue(result, "legacy semantic search"))
    }

    /**
     * Search an LVEC v2 lane using a host-computed vector and its exact encoder identity.
     *
     * The native engine fails closed if [embeddingSpaceId] differs from the ID embedded
     * in the loaded LVEC pack, even when the vector dimensions happen to match.
     */
    @Synchronized
    public fun searchWithEmbedding(
        query: String,
        embedding: FloatArray,
        embeddingSpaceId: EmbeddingSpaceId,
        options: SearchOptions = SearchOptions(),
    ): SearchResponse {
        validateSearch(query, options)
        validateEmbedding(embedding)
        val activeHandle = requireOpen()
        val result = callNative("Native semantic search failed") {
            nativeBridge.searchWithEmbedding(
                handle = activeHandle,
                query = query,
                embeddingSpaceId = embeddingSpaceId.nativeBytes(),
                embedding = embedding,
                options = options,
            )
        }
        return decodeSearch(requireSearchValue(result, "semantic search"))
    }

    /**
     * Search with a whole-query vector plus independently embedded canonical clauses.
     *
     * The native planner is rerun for [query] before retrieval. Clause text must exactly
     * match its ordered output, preventing arbitrary or stale segments from being accepted.
     */
    @Synchronized
    public fun searchWithSegmentEmbeddings(
        query: String,
        wholeEmbedding: FloatArray,
        segments: List<QuerySegmentEmbedding>,
        embeddingSpaceId: EmbeddingSpaceId,
        options: SearchOptions = SearchOptions(),
    ): SearchResponse {
        validateSearch(query, options)
        validateEmbedding(wholeEmbedding)
        validateSegments(segments)
        val activeHandle = requireOpen()
        val canonicalPlan = planQuerySegments(query, nativeBridge, json)
        if (segments.map { it.text } != canonicalPlan.segments) {
            throw RecallInvalidArgumentException(
                "segment text must exactly match the canonical ordered query plan",
            )
        }
        val result = callNative("Native segmented semantic search failed") {
            nativeBridge.searchWithSegmentEmbeddings(
                handle = activeHandle,
                query = query,
                embeddingSpaceId = embeddingSpaceId.nativeBytes(),
                wholeEmbedding = wholeEmbedding,
                segments = segments,
                options = options,
            )
        }
        return decodeSearch(requireSearchValue(result, "segmented semantic search"))
    }

    /** Runtime pack and lane metadata, including LVEC v2 dimensions and space ID. */
    @Synchronized
    public fun info(): EngineInfo {
        val activeHandle = requireOpen()
        val result = callNative("Could not read native engine metadata") {
            nativeBridge.engineInfo(activeHandle)
        }
        val nativeError = RecallNativeError.fromCode(result.errorCode)
        if (result.value == null || nativeError != RecallNativeError.OK) {
            throw RecallEngineInfoException(
                nativeError = nativeError,
                nativeErrorCode = result.errorCode,
                diagnostic = result.diagnostic,
            )
        }
        return try {
            json.decodeFromString<EngineInfo>(result.value)
        } catch (error: SerializationException) {
            throw RecallDecodingException("Native engine metadata was invalid JSON", error)
        } catch (error: IllegalArgumentException) {
            throw RecallDecodingException("Native engine metadata was invalid JSON", error)
        }
    }

    /** Idempotently release native resources. */
    @Synchronized
    override fun close() {
        if (handle == CLOSED_HANDLE) return
        val closingHandle = handle
        handle = CLOSED_HANDLE
        callNative("Could not close native engine") {
            nativeBridge.close(closingHandle)
        }
    }

    private fun requireSearchValue(
        result: NativeStringResult,
        operation: String,
    ): String {
        val nativeError = RecallNativeError.fromCode(result.errorCode)
        if (result.value != null && nativeError == RecallNativeError.OK) {
            return result.value
        }
        val detail = result.diagnostic?.let { ": $it" }.orEmpty()
        throw RecallSearchException(
            message = "Native $operation failed: ${nativeError.name} " +
                "(native code ${result.errorCode})$detail",
            nativeError = nativeError,
            nativeErrorCode = result.errorCode,
            diagnostic = result.diagnostic,
        )
    }

    private fun decodeSearch(raw: String): SearchResponse =
        try {
            json.decodeFromString<SearchResponse>(raw)
        } catch (error: SerializationException) {
            throw RecallDecodingException("Native search returned invalid response JSON", error)
        } catch (error: IllegalArgumentException) {
            throw RecallDecodingException("Native search returned invalid response JSON", error)
        }

    private fun requireOpen(): Long =
        handle.takeUnless { it == CLOSED_HANDLE } ?: throw RecallClosedException()

    public companion object Factory {
        /** C ABI version required by this wrapper. */
        public const val ABI_VERSION: Int = 1

        private const val CLOSED_HANDLE: Long = 0L

        private val RESPONSE_JSON: Json = Json {
            ignoreUnknownKeys = true
            explicitNulls = false
        }

        /** Run the corpus-independent canonical clause planner without opening packs. */
        @JvmStatic
        public fun planQuerySegments(query: String): QuerySegmentPlan =
            planQuerySegments(query, JniNativeBridge.instance, RESPONSE_JSON)

        /** Open packs supplied as byte arrays. */
        @JvmStatic
        public fun open(packs: List<ByteArray>): RecallEngine =
            openPrepared(
                packs = packs.map { bytes ->
                    ByteBuffer.allocateDirect(bytes.size).apply {
                        put(bytes)
                        flip()
                    }
                },
                nativeBridge = JniNativeBridge.instance,
            )

        /**
         * Open the bytes in each buffer's current `position..limit` range.
         *
         * Non-direct buffers are copied to a temporary direct buffer for JNI. Input positions are
         * not mutated.
         */
        @JvmStatic
        public fun openBuffers(packs: List<ByteBuffer>): RecallEngine =
            openPrepared(packs, JniNativeBridge.instance)

        internal fun openForTesting(
            packs: List<ByteBuffer>,
            nativeBridge: NativeBridge,
            json: Json = RESPONSE_JSON,
        ): RecallEngine = openPrepared(packs, nativeBridge, json)

        internal fun planQuerySegmentsForTesting(
            query: String,
            nativeBridge: NativeBridge,
            json: Json = RESPONSE_JSON,
        ): QuerySegmentPlan = planQuerySegments(query, nativeBridge, json)

        private fun openPrepared(
            packs: List<ByteBuffer>,
            nativeBridge: NativeBridge,
            json: Json = RESPONSE_JSON,
        ): RecallEngine {
            if (packs.isEmpty()) {
                throw RecallInvalidArgumentException("at least one pack is required")
            }
            if (packs.any { !it.hasRemaining() }) {
                throw RecallInvalidArgumentException("packs must not contain empty buffers")
            }

            val directPacks = packs.map(::directRemainingSlice).toTypedArray()
            val actualAbi = callNative("Could not query native ABI version") {
                nativeBridge.abiVersion()
            }
            if (actualAbi != ABI_VERSION) {
                throw RecallAbiMismatchException(ABI_VERSION, actualAbi)
            }

            val result = callNative("Could not open native engine") {
                nativeBridge.open(directPacks)
            }
            if (result.handle == CLOSED_HANDLE || result.errorCode != RecallNativeError.OK.code) {
                if (result.handle != CLOSED_HANDLE) {
                    callNative("Could not close failed native engine") {
                        nativeBridge.close(result.handle)
                    }
                }
                throw RecallOpenException(
                    nativeError = RecallNativeError.fromCode(result.errorCode),
                    nativeErrorCode = result.errorCode,
                    diagnostic = result.diagnostic,
                )
            }
            return RecallEngine(result.handle, nativeBridge, json)
        }

        private fun directRemainingSlice(source: ByteBuffer): ByteBuffer {
            val remaining = source.slice()
            if (remaining.isDirect) return remaining

            return ByteBuffer.allocateDirect(remaining.remaining()).apply {
                put(remaining)
                flip()
            }
        }

        private fun validateSearch(
            query: String,
            options: SearchOptions,
        ) {
            if (query.isBlank()) {
                throw RecallInvalidArgumentException("query must not be blank")
            }
            if ('\u0000' in query) {
                throw RecallInvalidArgumentException("query must not contain a NUL character")
            }
            if (options.topK !in 1..SearchOptions.MAX_TOP_K) {
                throw RecallInvalidArgumentException(
                    "topK must be between 1 and ${SearchOptions.MAX_TOP_K}",
                )
            }
            if (options.bookHint?.contains('\u0000') == true) {
                throw RecallInvalidArgumentException("bookHint must not contain a NUL character")
            }
        }

        private fun validateQuery(query: String) {
            if (query.isBlank()) {
                throw RecallInvalidArgumentException("query must not be blank")
            }
            if ('\u0000' in query) {
                throw RecallInvalidArgumentException("query must not contain a NUL character")
            }
        }

        private fun validateEmbedding(embedding: FloatArray) {
            if (embedding.isEmpty()) {
                throw RecallInvalidArgumentException("embedding must not be empty")
            }
            if (embedding.any { !it.isFinite() }) {
                throw RecallInvalidArgumentException("embedding must contain only finite values")
            }
            var squaredNorm = 0.0
            embedding.forEach { component ->
                val value = component.toDouble()
                squaredNorm += value * value
            }
            if (!squaredNorm.isFinite() || squaredNorm <= 0.0) {
                throw RecallInvalidArgumentException(
                    "embedding must have a finite, nonzero norm",
                )
            }
        }

        private fun validateSegments(segments: List<QuerySegmentEmbedding>) {
            if (segments.size !in 1..QuerySegmentPlan.MAX_SEGMENTS) {
                throw RecallInvalidArgumentException(
                    "segments must contain between 1 and ${QuerySegmentPlan.MAX_SEGMENTS} clauses",
                )
            }
            segments.forEachIndexed { index, segment ->
                if (segment.text.isBlank() || '\u0000' in segment.text) {
                    throw RecallInvalidArgumentException("segment $index has invalid text")
                }
                try {
                    validateEmbedding(segment.nativeEmbedding())
                } catch (error: RecallInvalidArgumentException) {
                    throw RecallInvalidArgumentException("segment $index has an invalid embedding")
                }
            }
        }

        private fun planQuerySegments(
            query: String,
            nativeBridge: NativeBridge,
            json: Json,
        ): QuerySegmentPlan {
            validateQuery(query)
            val result = callNative("Native query planning failed") {
                nativeBridge.planQuerySegments(query)
            }
            val raw = requireNativeString(result, "query planning")
            val plan = try {
                json.decodeFromString<QuerySegmentPlan>(raw)
            } catch (error: SerializationException) {
                throw RecallDecodingException("Native planner returned invalid JSON", error)
            } catch (error: IllegalArgumentException) {
                throw RecallDecodingException("Native planner returned invalid JSON", error)
            }
            if (
                plan.planner != QuerySegmentPlan.PLANNER_ID ||
                (
                    plan.segments.isNotEmpty() &&
                        plan.segments.size !in 2..QuerySegmentPlan.MAX_SEGMENTS
                ) ||
                plan.segments.any { it.isBlank() || '\u0000' in it }
            ) {
                throw RecallDecodingException(
                    "Native planner returned an invalid canonical query plan",
                    IllegalArgumentException("invalid planner identity or clause list"),
                )
            }
            return plan
        }

        private fun requireNativeString(
            result: NativeStringResult,
            operation: String,
        ): String {
            val nativeError = RecallNativeError.fromCode(result.errorCode)
            if (result.value != null && nativeError == RecallNativeError.OK) {
                return result.value
            }
            val detail = result.diagnostic?.let { ": $it" }.orEmpty()
            throw RecallSearchException(
                message = "Native $operation failed: ${nativeError.name} " +
                    "(native code ${result.errorCode})$detail",
                nativeError = nativeError,
                nativeErrorCode = result.errorCode,
                diagnostic = result.diagnostic,
            )
        }

        private inline fun <T> callNative(
            message: String,
            block: () -> T,
        ): T =
            try {
                block()
            } catch (error: RecallException) {
                throw error
            } catch (error: LinkageError) {
                throw RecallNativeLibraryException(message, error)
            } catch (error: SecurityException) {
                throw RecallNativeLibraryException(message, error)
            }
    }
}
