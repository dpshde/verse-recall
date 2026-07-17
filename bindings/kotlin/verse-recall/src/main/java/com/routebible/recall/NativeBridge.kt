package com.routebible.recall

import java.nio.ByteBuffer

internal data class NativeOpenResult(
    val handle: Long,
    val errorCode: Int,
    val diagnostic: String? = null,
)

internal data class NativeStringResult(
    val value: String?,
    val errorCode: Int,
    val diagnostic: String? = null,
)

/** Small injectable seam that keeps JVM tests independent from Android/JNI. */
internal interface NativeBridge {
    fun abiVersion(): Int

    fun open(packs: Array<ByteBuffer>): NativeOpenResult

    fun engineInfo(handle: Long): NativeStringResult

    fun planQuerySegments(query: String): NativeStringResult

    fun search(
        handle: Long,
        query: String,
        options: SearchOptions,
    ): NativeStringResult

    /** Compatibility path for legacy LVEC v1 packs. */
    fun searchWithEmbedding(
        handle: Long,
        query: String,
        embedding: FloatArray,
        options: SearchOptions,
    ): NativeStringResult

    /** Fail-closed LVEC v2 search over a whole query and its canonical clauses. */
    fun searchWithSegmentEmbeddings(
        handle: Long,
        query: String,
        embeddingSpaceId: ByteArray,
        wholeEmbedding: FloatArray,
        segments: List<QuerySegmentEmbedding>,
        options: SearchOptions,
    ): NativeStringResult

    /** Fail-closed path for LVEC v2 packs. */
    fun searchWithEmbedding(
        handle: Long,
        query: String,
        embeddingSpaceId: ByteArray,
        embedding: FloatArray,
        options: SearchOptions,
    ): NativeStringResult

    fun close(handle: Long)
}

internal class JniNativeBridge private constructor(
    private val loadFailure: Throwable?,
) : NativeBridge {
    override fun abiVersion(): Int {
        ensureLoaded()
        return nativeAbiVersion()
    }

    override fun open(packs: Array<ByteBuffer>): NativeOpenResult {
        ensureLoaded()
        nativeClearLastError()
        val raw = nativeOpen(packs)
        if (raw.size != OPEN_RESULT_SIZE) {
            throw RecallNativeLibraryException(
                "JNI returned an invalid open result (expected $OPEN_RESULT_SIZE values, got ${raw.size})",
            )
        }
        val errorCode = raw[1].toInt()
        return NativeOpenResult(
            handle = raw[0],
            errorCode = errorCode,
            diagnostic = if (raw[0] == 0L || errorCode != RecallNativeError.OK.code) {
                nativeLastErrorMessage()
            } else {
                null
            },
        )
    }

    override fun engineInfo(handle: Long): NativeStringResult =
        stringResult { outError -> nativeEngineInfo(handle, outError) }

    override fun planQuerySegments(query: String): NativeStringResult =
        stringResult { outError ->
            nativePlanQuerySegments(query.encodeToByteArray(), outError)
        }

    private inline fun stringResult(block: (IntArray) -> String?): NativeStringResult {
        ensureLoaded()
        nativeClearLastError()
        val outError = intArrayOf(RecallNativeError.INTERNAL.code)
        val value = block(outError)
        return NativeStringResult(
            value = value,
            errorCode = outError[0],
            diagnostic = if (value == null) nativeLastErrorMessage() else null,
        )
    }

    override fun search(
        handle: Long,
        query: String,
        options: SearchOptions,
    ): NativeStringResult = stringResult { outError ->
        nativeSearch(
            handle = handle,
            queryUtf8 = query.encodeToByteArray(),
            topK = options.topK,
            bookHintUtf8 = options.bookHint?.encodeToByteArray(),
            outError = outError,
        )
    }

    override fun searchWithEmbedding(
        handle: Long,
        query: String,
        embedding: FloatArray,
        options: SearchOptions,
    ): NativeStringResult = stringResult { outError ->
        nativeSearchWithEmbedding(
            handle = handle,
            queryUtf8 = query.encodeToByteArray(),
            embedding = embedding,
            topK = options.topK,
            bookHintUtf8 = options.bookHint?.encodeToByteArray(),
            outError = outError,
        )
    }

    override fun searchWithSegmentEmbeddings(
        handle: Long,
        query: String,
        embeddingSpaceId: ByteArray,
        wholeEmbedding: FloatArray,
        segments: List<QuerySegmentEmbedding>,
        options: SearchOptions,
    ): NativeStringResult = stringResult { outError ->
        nativeSearchWithSegmentEmbeddingsV2(
            handle = handle,
            queryUtf8 = query.encodeToByteArray(),
            embeddingSpaceId = embeddingSpaceId,
            wholeEmbedding = wholeEmbedding,
            segmentTextsUtf8 = segments.map { it.text.encodeToByteArray() }.toTypedArray(),
            segmentEmbeddings = segments.map { it.nativeEmbedding() }.toTypedArray(),
            topK = options.topK,
            bookHintUtf8 = options.bookHint?.encodeToByteArray(),
            outError = outError,
        )
    }

    override fun searchWithEmbedding(
        handle: Long,
        query: String,
        embeddingSpaceId: ByteArray,
        embedding: FloatArray,
        options: SearchOptions,
    ): NativeStringResult = stringResult { outError ->
        nativeSearchWithEmbeddingV2(
            handle = handle,
            queryUtf8 = query.encodeToByteArray(),
            embeddingSpaceId = embeddingSpaceId,
            embedding = embedding,
            topK = options.topK,
            bookHintUtf8 = options.bookHint?.encodeToByteArray(),
            outError = outError,
        )
    }

    override fun close(handle: Long) {
        ensureLoaded()
        nativeClose(handle)
    }

    private fun ensureLoaded() {
        loadFailure?.let { failure ->
            throw RecallNativeLibraryException(
                "Could not load librecall_jni.so (and its librecall_ffi.so dependency)",
                failure,
            )
        }
    }

    private external fun nativeAbiVersion(): Int

    /** Returns `[handle, recall_err]`. */
    private external fun nativeOpen(packs: Array<ByteBuffer>): LongArray

    private external fun nativeEngineInfo(
        handle: Long,
        outError: IntArray,
    ): String?

    private external fun nativePlanQuerySegments(
        queryUtf8: ByteArray,
        outError: IntArray,
    ): String?

    private external fun nativeSearch(
        handle: Long,
        queryUtf8: ByteArray,
        topK: Int,
        bookHintUtf8: ByteArray?,
        outError: IntArray,
    ): String?

    private external fun nativeSearchWithEmbedding(
        handle: Long,
        queryUtf8: ByteArray,
        embedding: FloatArray,
        topK: Int,
        bookHintUtf8: ByteArray?,
        outError: IntArray,
    ): String?

    private external fun nativeSearchWithEmbeddingV2(
        handle: Long,
        queryUtf8: ByteArray,
        embeddingSpaceId: ByteArray,
        embedding: FloatArray,
        topK: Int,
        bookHintUtf8: ByteArray?,
        outError: IntArray,
    ): String?

    private external fun nativeSearchWithSegmentEmbeddingsV2(
        handle: Long,
        queryUtf8: ByteArray,
        embeddingSpaceId: ByteArray,
        wholeEmbedding: FloatArray,
        segmentTextsUtf8: Array<ByteArray>,
        segmentEmbeddings: Array<FloatArray>,
        topK: Int,
        bookHintUtf8: ByteArray?,
        outError: IntArray,
    ): String?

    private external fun nativeLastErrorMessage(): String?

    private external fun nativeClearLastError()

    private external fun nativeClose(handle: Long)

    internal companion object {
        private const val OPEN_RESULT_SIZE: Int = 2

        val instance: JniNativeBridge by lazy {
            var failure: Throwable? = null
            try {
                System.loadLibrary("recall_jni")
            } catch (error: LinkageError) {
                failure = error
            } catch (error: SecurityException) {
                failure = error
            }
            JniNativeBridge(failure)
        }
    }
}
