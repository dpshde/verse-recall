package com.routebible.recall

import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.double
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.math.abs
import kotlin.math.sqrt
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlin.test.assertNotSame
import kotlin.test.assertTrue

class RecallEngineTest {
    @Test
    fun `query adapter parses current buffer range and applies normalized matrix`() {
        val encoded = queryAdapterBytes(
            inputDimensions = 2,
            outputDimensions = 3,
            matrix = floatArrayOf(
                1f, 0f, 1f,
                0f, 2f, 1f,
            ),
        )
        val framed = ByteBuffer.allocate(encoded.size + 2).apply {
            put(9)
            put(encoded)
            put(8)
            flip()
            position(1)
            limit(1 + encoded.size)
        }

        val adapter = QueryAdapter(framed)
        val output = adapter.apply(floatArrayOf(3f, 4f))

        assertEquals(1, framed.position(), "the caller's position must not change")
        assertEquals(2, adapter.inputDimensions)
        assertEquals(3, adapter.outputDimensions)
        assertEquals(3, output.size)
        assertTrue(abs(output[0] - (3.0 / sqrt(122.0)).toFloat()) < 1e-6f)
        assertTrue(abs(output[1] - (8.0 / sqrt(122.0)).toFloat()) < 1e-6f)
        assertTrue(abs(output[2] - (7.0 / sqrt(122.0)).toFloat()) < 1e-6f)
        assertTrue(abs(output.sumOf { it.toDouble() * it.toDouble() } - 1.0) < 1e-6)
    }

    @Test
    fun `query adapter fails closed on malformed metadata and embeddings`() {
        val valid = queryAdapterBytes(2, 1, floatArrayOf(1f, 0f))
        val truncated = valid.copyOf(valid.size - 1)
        val badMagic = valid.copyOf().apply { this[0] = 0 }
        val nonFinite = queryAdapterBytes(1, 1, floatArrayOf(Float.NaN))
        val overflowingDimensions = ByteBuffer.allocate(16)
            .order(ByteOrder.LITTLE_ENDIAN)
            .apply {
                put("VRQADP01".encodeToByteArray())
                putInt(-1)
                putInt(-1)
            }.array()

        assertFailsWith<RecallQueryAdapterException> { QueryAdapter(truncated) }
        assertFailsWith<RecallQueryAdapterException> { QueryAdapter(badMagic) }
        assertFailsWith<RecallQueryAdapterException> { QueryAdapter(nonFinite) }
        assertFailsWith<RecallQueryAdapterException> { QueryAdapter(overflowingDimensions) }

        val adapter = QueryAdapter(valid)
        assertFailsWith<RecallQueryAdapterException> { adapter.apply(floatArrayOf(1f)) }
        assertFailsWith<RecallInvalidArgumentException> {
            adapter.apply(floatArrayOf(Float.POSITIVE_INFINITY, 0f))
        }
        assertFailsWith<RecallInvalidArgumentException> {
            adapter.apply(floatArrayOf(0f, 0f))
        }
    }

    @Test
    fun `query adapter matches shared double accumulation golden`() {
        val fixtureFile = generateSequence(File(System.getProperty("user.dir")).canonicalFile) {
            it.parentFile
        }.map { root ->
            root.resolve("tests/fixtures/vrqadp01-golden-v1.json")
        }.firstOrNull(File::isFile) ?: error("shared query-adapter golden fixture not found")
        val fixture = Json.parseToJsonElement(fixtureFile.readText()).jsonObject
        assertEquals(
            "verse-recall-vrqadp01-arithmetic-v1",
            fixture.getValue("schema").jsonPrimitive.content,
        )
        val inputDimensions = fixture.getValue("input_dimensions").jsonPrimitive.int
        val outputDimensions = fixture.getValue("output_dimensions").jsonPrimitive.int
        val matrix = fixture.getValue("matrix").jsonArray
            .map { it.jsonPrimitive.double.toFloat() }
            .toFloatArray()
        val embedding = fixture.getValue("embedding").jsonArray
            .map { it.jsonPrimitive.double.toFloat() }
            .toFloatArray()
        val expectedBits = fixture.getValue("expected_f32_bits").jsonArray
            .map { it.jsonPrimitive.content }

        val adapter = QueryAdapter(
            queryAdapterBytes(inputDimensions, outputDimensions, matrix),
        )
        val actualBits = adapter.apply(embedding).map { value ->
            value.toRawBits().toUInt().toString(16).padStart(8, '0')
        }
        assertEquals(expectedBits, actualBits)
    }

    @Test
    fun `open sends only remaining bytes in direct buffers`() {
        val bridge = FakeNativeBridge()
        val source = ByteBuffer.wrap(byteArrayOf(9, 1, 2, 3, 8)).apply {
            position(1)
            limit(4)
        }

        val engine = RecallEngine.openForTesting(listOf(source), bridge)

        assertEquals(1, source.position(), "the caller's position must not change")
        assertEquals(1, bridge.openedPacks.size)
        assertTrue(bridge.openedPacks.single().isDirect)
        assertContentEquals(byteArrayOf(1, 2, 3), bridge.openedPackBytes.single())
        engine.close()
    }

    private fun queryAdapterBytes(
        inputDimensions: Int,
        outputDimensions: Int,
        matrix: FloatArray,
    ): ByteArray =
        ByteBuffer.allocate(16 + matrix.size * Float.SIZE_BYTES)
            .order(ByteOrder.LITTLE_ENDIAN)
            .apply {
                put("VRQADP01".encodeToByteArray())
                putInt(inputDimensions)
                putInt(outputDimensions)
                matrix.forEach { putFloat(it) }
            }.array()

    @Test
    fun `search decodes complete typed response and forwards options`() {
        val bridge = FakeNativeBridge().apply {
            searchResult = NativeStringResult(RESPONSE_JSON, RecallNativeError.OK.code)
        }
        val engine = openEngine(bridge)

        val response = engine.search(
            "the fall",
            SearchOptions(topK = 3, bookHint = "Genesis"),
        )

        assertEquals("the fall", response.query)
        val result = response.results.single()
        assertEquals("Genesis 3:6", result.reference)
        assertEquals(Confidence.RELATED, result.confidence)
        assertEquals(0.25f, result.lanes.c)
        assertEquals(listOf("fall"), result.matchedTerms)
        assertEquals("Genesis 3:1", result.passages.single().startReference)
        assertEquals(SearchOptions(3, "Genesis"), bridge.lastOptions)
        assertEquals("the fall", bridge.lastQuery)
        engine.close()
    }

    @Test
    fun `missing optional result arrays decode as empty`() {
        val bridge = FakeNativeBridge().apply {
            searchResult = NativeStringResult(
                value = """
                    {"query":"lost sheep","results":[{
                      "ref":"Luke 15:4","text":"Which of you...","score":1.0,
                      "confidence":"recovered","lanes":{}
                    }]}
                """.trimIndent(),
                errorCode = RecallNativeError.OK.code,
            )
        }
        val engine = openEngine(bridge)

        val result = engine.search("lost sheep").results.single()

        assertTrue(result.matchedTerms.isEmpty())
        assertTrue(result.expandedTerms.isEmpty())
        assertTrue(result.passages.isEmpty())
        assertTrue(result.chips.isEmpty())
        engine.close()
    }

    @Test
    fun `semantic search validates and forwards embedding`() {
        val bridge = FakeNativeBridge()
        val engine = openEngine(bridge)
        val embedding = floatArrayOf(0.25f, -0.5f, 1f)

        engine.searchWithEmbedding(
            query = "strength when weak",
            embedding = embedding,
            options = SearchOptions(topK = 8),
        )

        assertContentEquals(embedding, bridge.lastEmbedding)
        assertEquals(1, bridge.legacySemanticCalls)
        assertEquals(0, bridge.v2SemanticCalls)
        assertEquals(8, bridge.lastOptions?.topK)
        assertFailsWith<RecallInvalidArgumentException> {
            engine.searchWithEmbedding("query", floatArrayOf(Float.NaN))
        }
        assertFailsWith<RecallInvalidArgumentException> {
            engine.searchWithEmbedding("query", floatArrayOf())
        }
        assertFailsWith<RecallInvalidArgumentException> {
            engine.searchWithEmbedding("query", floatArrayOf(0f, 0f))
        }
        engine.close()
    }

    @Test
    fun `v2 semantic search requires and forwards exact 32 byte space id`() {
        val bridge = FakeNativeBridge()
        val engine = openEngine(bridge)
        val id = EmbeddingSpaceId(ByteArray(32) { index -> (index + 1).toByte() })
        val embedding = floatArrayOf(0.25f, -0.5f, 1f)

        engine.searchWithEmbedding(
            query = "strength when weak",
            embedding = embedding,
            embeddingSpaceId = id,
            options = SearchOptions(topK = 4),
        )

        assertContentEquals(id.bytes, bridge.lastEmbeddingSpaceId)
        assertContentEquals(embedding, bridge.lastEmbedding)
        assertEquals(0, bridge.legacySemanticCalls)
        assertEquals(1, bridge.v2SemanticCalls)
        engine.close()
    }

    @Test
    fun `canonical planner and segmented search forward the complete contract`() {
        val bridge = FakeNativeBridge().apply {
            planResult = NativeStringResult(SEGMENT_PLAN_JSON, RecallNativeError.OK.code)
        }
        val engine = openEngine(bridge)
        val spaceId = EmbeddingSpaceId(ByteArray(32) { index -> (index + 1).toByte() })

        val plan = RecallEngine.planQuerySegmentsForTesting(
            "lost sheep and prodigal son",
            bridge,
        )
        assertEquals(QuerySegmentPlan.PLANNER_ID, plan.planner)
        assertEquals(listOf("lost sheep", "prodigal son"), plan.segments)

        val firstVector = floatArrayOf(1f, 0f)
        val segments = listOf(
            QuerySegmentEmbedding(plan.segments[0], firstVector),
            QuerySegmentEmbedding(plan.segments[1], floatArrayOf(0f, 1f)),
        )
        firstVector[0] = 99f
        engine.searchWithSegmentEmbeddings(
            query = "lost sheep and prodigal son",
            wholeEmbedding = floatArrayOf(0.5f, 0.5f),
            segments = segments,
            embeddingSpaceId = spaceId,
            options = SearchOptions(topK = 4, bookHint = "Luke"),
        )

        assertEquals(2, bridge.planCalls)
        assertEquals(1, bridge.segmentedSemanticCalls)
        assertEquals("lost sheep and prodigal son", bridge.lastQuery)
        assertContentEquals(floatArrayOf(0.5f, 0.5f), bridge.lastEmbedding)
        assertEquals(segments, bridge.lastSegments)
        assertContentEquals(floatArrayOf(1f, 0f), bridge.lastSegments?.first()?.embedding)
        assertContentEquals(spaceId.bytes, bridge.lastEmbeddingSpaceId)
        assertEquals(SearchOptions(4, "Luke"), bridge.lastOptions)
        engine.close()
    }

    @Test
    fun `segmented search rejects noncanonical and invalid clauses before JNI`() {
        val bridge = FakeNativeBridge().apply {
            planResult = NativeStringResult(SEGMENT_PLAN_JSON, RecallNativeError.OK.code)
        }
        val engine = openEngine(bridge)
        val id = EmbeddingSpaceId(ByteArray(32) { 1 })

        assertFailsWith<RecallInvalidArgumentException> {
            engine.searchWithSegmentEmbeddings(
                query = "lost sheep and prodigal son",
                wholeEmbedding = floatArrayOf(1f, 1f),
                segments = listOf(
                    QuerySegmentEmbedding("prodigal son", floatArrayOf(1f, 0f)),
                    QuerySegmentEmbedding("lost sheep", floatArrayOf(0f, 1f)),
                ),
                embeddingSpaceId = id,
            )
        }
        assertFailsWith<RecallInvalidArgumentException> {
            engine.searchWithSegmentEmbeddings(
                query = "lost sheep and prodigal son",
                wholeEmbedding = floatArrayOf(1f, 1f),
                segments = emptyList(),
                embeddingSpaceId = id,
            )
        }
        assertFailsWith<RecallInvalidArgumentException> {
            engine.searchWithSegmentEmbeddings(
                query = "lost sheep and prodigal son",
                wholeEmbedding = floatArrayOf(1f, 1f),
                segments = listOf(
                    QuerySegmentEmbedding("lost sheep", floatArrayOf(Float.NaN)),
                    QuerySegmentEmbedding("prodigal son", floatArrayOf(0f, 1f)),
                ),
                embeddingSpaceId = id,
            )
        }
        assertEquals(0, bridge.segmentedSemanticCalls)
        engine.close()
    }

    @Test
    fun `planner rejects malformed identity unbounded clauses and native failures`() {
        val bridge = FakeNativeBridge().apply {
            planResult = NativeStringResult(
                "{\"planner\":\"other\",\"segments\":[\"one\"]}",
                RecallNativeError.OK.code,
            )
        }
        assertFailsWith<RecallDecodingException> {
            RecallEngine.planQuerySegmentsForTesting("one and two", bridge)
        }

        bridge.planResult = NativeStringResult(
            "{\"planner\":\"verse-recall-clause-v2\",\"segments\":[\"1\",\"2\",\"3\",\"4\",\"5\"]}",
            RecallNativeError.OK.code,
        )
        assertFailsWith<RecallDecodingException> {
            RecallEngine.planQuerySegmentsForTesting("one and two", bridge)
        }

        bridge.planResult = NativeStringResult(
            null,
            RecallNativeError.INVALID_ARGUMENT.code,
            "planner rejected query",
        )
        val failure = assertFailsWith<RecallSearchException> {
            RecallEngine.planQuerySegmentsForTesting("one and two", bridge)
        }
        assertEquals(RecallNativeError.INVALID_ARGUMENT, failure.nativeError)
        assertEquals("planner rejected query", failure.diagnostic)
    }

    @Test
    fun `embedding space id validates hex and owns its bytes`() {
        val source = ByteArray(32) { index -> (index + 1).toByte() }
        val id = EmbeddingSpaceId(source)
        val returned = id.bytes

        source[0] = 0
        returned[1] = 0

        assertEquals("01020304", id.hex.take(8))
        assertNotSame(returned, id.bytes)
        assertEquals(id, EmbeddingSpaceId(id.hex.uppercase()))
        assertFailsWith<RecallInvalidArgumentException> { EmbeddingSpaceId(ByteArray(31)) }
        assertFailsWith<RecallInvalidArgumentException> { EmbeddingSpaceId(ByteArray(32)) }
        assertFailsWith<RecallInvalidArgumentException> { EmbeddingSpaceId("not-hex") }
    }

    @Test
    fun `engine info decodes dense v2 contract`() {
        val bridge = FakeNativeBridge().apply {
            infoResult = NativeStringResult(ENGINE_INFO_JSON, RecallNativeError.OK.code)
        }
        val engine = openEngine(bridge)

        val info = engine.info()

        assertEquals(1, info.abiVersion)
        assertEquals("BSB", info.translationId)
        assertEquals(31_102, info.verseCount)
        assertEquals(listOf("a", "b", "c", "d"), info.lanes)
        assertEquals(384, info.dense?.dims)
        assertEquals(listOf("verse", "window"), info.dense?.tableRoles)
        assertEquals("01".repeat(32), info.dense?.embeddingSpaceId?.hex)
        engine.close()
    }

    @Test
    fun `engine info preserves native failure and rejects malformed space identity`() {
        val bridge = FakeNativeBridge().apply {
            infoResult = NativeStringResult(
                value = null,
                errorCode = RecallNativeError.JSON_FAILED.code,
                diagnostic = "metadata serialization failed",
            )
        }
        val engine = openEngine(bridge)

        val nativeError = assertFailsWith<RecallEngineInfoException> { engine.info() }
        assertEquals(RecallNativeError.JSON_FAILED, nativeError.nativeError)
        assertEquals("metadata serialization failed", nativeError.diagnostic)

        bridge.infoResult = NativeStringResult(
            value = ENGINE_INFO_JSON.replace("01".repeat(32), "00".repeat(32)),
            errorCode = RecallNativeError.OK.code,
        )
        assertFailsWith<RecallDecodingException> { engine.info() }
        engine.close()
    }

    @Test
    fun `structured native semantic failure preserves code and diagnostic`() {
        val bridge = FakeNativeBridge().apply {
            v2SemanticResult = NativeStringResult(
                value = null,
                errorCode = RecallNativeError.EMBEDDING_SPACE_MISMATCH.code,
                diagnostic = "query encoder does not match LVEC",
            )
        }
        val engine = openEngine(bridge)

        val error = assertFailsWith<RecallSearchException> {
            engine.searchWithEmbedding(
                query = "strength when weak",
                embedding = floatArrayOf(1f),
                embeddingSpaceId = EmbeddingSpaceId(ByteArray(32) { 1 }),
            )
        }

        assertEquals(RecallNativeError.EMBEDDING_SPACE_MISMATCH, error.nativeError)
        assertEquals(8, error.nativeErrorCode)
        assertEquals("query encoder does not match LVEC", error.diagnostic)
        engine.close()
    }

    @Test
    fun `close is idempotent and all later searches fail`() {
        val bridge = FakeNativeBridge()
        val engine = openEngine(bridge)

        engine.close()
        engine.close()

        assertEquals(listOf(bridge.openResult.handle), bridge.closedHandles)
        assertFailsWith<RecallClosedException> { engine.search("anything") }
        assertFailsWith<RecallClosedException> {
            engine.searchWithEmbedding("anything", floatArrayOf(1f))
        }
        assertFailsWith<RecallClosedException> { engine.info() }
    }

    @Test
    fun `open maps native error code`() {
        val bridge = FakeNativeBridge().apply {
            openResult = NativeOpenResult(
                handle = 0,
                errorCode = 2,
                diagnostic = "invalid pack checksum",
            )
        }

        val error = assertFailsWith<RecallOpenException> {
            RecallEngine.openForTesting(listOf(pack()), bridge)
        }

        assertEquals(RecallNativeError.OPEN_FAILED, error.nativeError)
        assertEquals(2, error.nativeErrorCode)
        assertEquals("invalid pack checksum", error.diagnostic)
    }

    @Test
    fun `open rejects incompatible ABI before opening packs`() {
        val bridge = FakeNativeBridge().apply { abi = 7 }

        val error = assertFailsWith<RecallAbiMismatchException> {
            RecallEngine.openForTesting(listOf(pack()), bridge)
        }

        assertEquals(1, error.expected)
        assertEquals(7, error.actual)
        assertTrue(bridge.openedPacks.isEmpty())
    }

    @Test
    fun `invalid public inputs produce typed argument failures`() {
        val bridge = FakeNativeBridge()

        assertFailsWith<RecallInvalidArgumentException> {
            RecallEngine.openForTesting(emptyList(), bridge)
        }
        assertFailsWith<RecallInvalidArgumentException> {
            RecallEngine.openForTesting(listOf(ByteBuffer.allocate(0)), bridge)
        }

        val engine = openEngine(bridge)
        assertFailsWith<RecallInvalidArgumentException> { engine.search("   ") }
        assertFailsWith<RecallInvalidArgumentException> { engine.search("bad\u0000query") }
        assertFailsWith<RecallInvalidArgumentException> {
            engine.search("query", SearchOptions(topK = 0))
        }
        assertFailsWith<RecallInvalidArgumentException> {
            engine.search("query", SearchOptions(topK = SearchOptions.MAX_TOP_K + 1))
        }
        assertFailsWith<RecallInvalidArgumentException> {
            engine.search("query", SearchOptions(bookHint = "bad\u0000hint"))
        }
        engine.close()
    }

    @Test
    fun `null native response and malformed JSON have distinct typed errors`() {
        val bridge = FakeNativeBridge().apply {
            searchResult = NativeStringResult(null, RecallNativeError.SEARCH_FAILED.code, "failed")
        }
        val engine = openEngine(bridge)

        val nativeError = assertFailsWith<RecallSearchException> { engine.search("query") }
        assertEquals(RecallNativeError.SEARCH_FAILED, nativeError.nativeError)

        bridge.searchResult = NativeStringResult("not-json", RecallNativeError.OK.code)
        assertFailsWith<RecallDecodingException> { engine.search("query") }
        engine.close()
    }

    @Test
    fun `all hardened c error codes map without collapsing`() {
        assertEquals(RecallNativeError.INVALID_ARGUMENT, RecallNativeError.fromCode(5))
        assertEquals(RecallNativeError.SEARCH_FAILED, RecallNativeError.fromCode(6))
        assertEquals(RecallNativeError.INVALID_EMBEDDING, RecallNativeError.fromCode(7))
        assertEquals(RecallNativeError.EMBEDDING_SPACE_MISMATCH, RecallNativeError.fromCode(8))
        assertEquals(RecallNativeError.PANIC_CONTAINED, RecallNativeError.fromCode(9))
        assertEquals(RecallNativeError.JSON_FAILED, RecallNativeError.fromCode(10))
        assertEquals(RecallNativeError.UNKNOWN, RecallNativeError.fromCode(99))
    }

    @Test
    fun `linkage failures are wrapped as native library errors`() {
        val bridge = object : NativeBridge by FakeNativeBridge() {
            override fun abiVersion(): Int = throw UnsatisfiedLinkError("missing")
        }

        val error = assertFailsWith<RecallNativeLibraryException> {
            RecallEngine.openForTesting(listOf(pack()), bridge)
        }

        assertIs<UnsatisfiedLinkError>(error.cause)
    }

    private fun openEngine(bridge: FakeNativeBridge): RecallEngine =
        RecallEngine.openForTesting(listOf(pack()), bridge)

    private fun pack(): ByteBuffer = ByteBuffer.wrap(byteArrayOf(1, 2, 3))

    private class FakeNativeBridge : NativeBridge {
        var abi: Int = 1
        var openResult: NativeOpenResult = NativeOpenResult(handle = 42, errorCode = 0)
        var searchResult: NativeStringResult =
            NativeStringResult(EMPTY_RESPONSE_JSON, RecallNativeError.OK.code)
        var legacySemanticResult: NativeStringResult =
            NativeStringResult(EMPTY_RESPONSE_JSON, RecallNativeError.OK.code)
        var v2SemanticResult: NativeStringResult =
            NativeStringResult(EMPTY_RESPONSE_JSON, RecallNativeError.OK.code)
        var infoResult: NativeStringResult =
            NativeStringResult(ENGINE_INFO_JSON, RecallNativeError.OK.code)
        var planResult: NativeStringResult =
            NativeStringResult(
                "{\"planner\":\"verse-recall-clause-v2\",\"segments\":[]}",
                RecallNativeError.OK.code,
            )
        val openedPacks = mutableListOf<ByteBuffer>()
        val openedPackBytes = mutableListOf<ByteArray>()
        val closedHandles = mutableListOf<Long>()
        var lastQuery: String? = null
        var lastOptions: SearchOptions? = null
        var lastEmbedding: FloatArray? = null
        var lastEmbeddingSpaceId: ByteArray? = null
        var lastSegments: List<QuerySegmentEmbedding>? = null
        var legacySemanticCalls: Int = 0
        var v2SemanticCalls: Int = 0
        var segmentedSemanticCalls: Int = 0
        var planCalls: Int = 0

        override fun abiVersion(): Int = abi

        override fun open(packs: Array<ByteBuffer>): NativeOpenResult {
            openedPacks += packs
            openedPackBytes += packs.map { buffer ->
                buffer.duplicate().let { copy ->
                    ByteArray(copy.remaining()).also(copy::get)
                }
            }
            return openResult
        }

        override fun search(
            handle: Long,
            query: String,
            options: SearchOptions,
        ): NativeStringResult {
            lastQuery = query
            lastOptions = options
            return searchResult
        }

        override fun engineInfo(handle: Long): NativeStringResult = infoResult

        override fun planQuerySegments(query: String): NativeStringResult {
            lastQuery = query
            planCalls += 1
            return planResult
        }

        override fun searchWithEmbedding(
            handle: Long,
            query: String,
            embedding: FloatArray,
            options: SearchOptions,
        ): NativeStringResult {
            lastQuery = query
            lastEmbedding = embedding.copyOf()
            lastOptions = options
            legacySemanticCalls += 1
            return legacySemanticResult
        }

        override fun searchWithEmbedding(
            handle: Long,
            query: String,
            embeddingSpaceId: ByteArray,
            embedding: FloatArray,
            options: SearchOptions,
        ): NativeStringResult {
            lastQuery = query
            lastEmbeddingSpaceId = embeddingSpaceId.copyOf()
            lastEmbedding = embedding.copyOf()
            lastOptions = options
            v2SemanticCalls += 1
            return v2SemanticResult
        }

        override fun searchWithSegmentEmbeddings(
            handle: Long,
            query: String,
            embeddingSpaceId: ByteArray,
            wholeEmbedding: FloatArray,
            segments: List<QuerySegmentEmbedding>,
            options: SearchOptions,
        ): NativeStringResult {
            lastQuery = query
            lastEmbeddingSpaceId = embeddingSpaceId.copyOf()
            lastEmbedding = wholeEmbedding.copyOf()
            lastSegments = segments.map { segment ->
                QuerySegmentEmbedding(segment.text, segment.embedding)
            }
            lastOptions = options
            segmentedSemanticCalls += 1
            return v2SemanticResult
        }

        override fun close(handle: Long) {
            closedHandles += handle
        }
    }

    private companion object {
        const val EMPTY_RESPONSE_JSON: String = "{\"query\":\"query\",\"results\":[]}"

        const val SEGMENT_PLAN_JSON: String =
            "{\"planner\":\"verse-recall-clause-v2\",\"segments\":[\"lost sheep\",\"prodigal son\"]}"

        val ENGINE_INFO_JSON: String = """
            {
              "abi_version": 1,
              "translation_id": "BSB",
              "verse_count": 31102,
              "lanes": ["a", "b", "c", "d"],
              "dense": {
                "version": 2,
                "dims": 384,
                "table_count": 2,
                "table_roles": ["verse", "window"],
                "embedding_space_id": "${"01".repeat(32)}"
              }
            }
        """.trimIndent()

        val RESPONSE_JSON: String = """
            {
              "query": "the fall",
              "results": [{
                "ref": "Genesis 3:6",
                "text": "The woman saw that the tree was good for food...",
                "score": 3.25,
                "confidence": "related",
                "lanes": {"a": 1.0, "b": 0.5, "c": 0.25, "d": 0.0},
                "matched_terms": ["fall"],
                "expanded_terms": ["disobedience"],
                "passages": [{"start_ref": "Genesis 3:1", "end_ref": "Genesis 3:24"}],
                "chips": ["event"],
                "future_field": true
              }]
            }
        """.trimIndent()
    }
}
