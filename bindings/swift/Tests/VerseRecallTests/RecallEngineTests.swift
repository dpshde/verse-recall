import Foundation
import XCTest
@testable import VerseRecall

final class RecallEngineTests: XCTestCase {
    func testNativeEngineSearchesCanonicalPackWhenAvailable() throws {
        guard let packPath = ProcessInfo.processInfo.environment["VERSE_RECALL_TEST_PACK"] else {
            throw XCTSkip("VERSE_RECALL_TEST_PACK is not configured")
        }
        let engine = try RecallEngine(packURLs: [URL(fileURLWithPath: packPath)])
        defer { engine.close() }

        let info = try engine.info()
        let response = try engine.search("the Lord is my shepherd")

        XCTAssertEqual(info.abiVersion, RecallEngine.supportedABIVersion)
        XCTAssertEqual(info.translationID, "BSB")
        XCTAssertEqual(info.verseCount, 31_086)
        XCTAssertFalse(info.lanes.isEmpty)
        XCTAssertNil(info.dense)
        XCTAssertEqual(response.query, "the Lord is my shepherd")
        XCTAssertEqual(response.results.first?.reference, "Psalm 23:1")
        XCTAssertEqual(response.results.first?.confidence, .recovered)

        let compoundQuery = "lost sheep and prodigal son"
        let plan = try RecallEngine.planQuerySegments(compoundQuery)
        XCTAssertEqual(plan.segments, ["lost sheep", "prodigal son"])
        XCTAssertThrowsError(
            try engine.search(
                compoundQuery,
                embedding: [1, 1],
                segments: [
                    QuerySegmentEmbedding(text: plan.segments[0], embedding: [1, 0]),
                    QuerySegmentEmbedding(text: plan.segments[1], embedding: [0, 1]),
                ],
                embeddingSpaceID: Self.spaceID
            )
        ) { error in
            guard case let .nativeFailure(operation, code, message) = error as? RecallError else {
                return XCTFail("expected nativeFailure, got \(error)")
            }
            XCTAssertEqual(operation, .semanticSearch)
            XCTAssertEqual(code, .embedding)
            XCTAssertTrue(message?.contains("no LVEC pack") == true)
        }

        XCTAssertThrowsError(
            try engine.search(
                "meaning-only query",
                embedding: [1, 0],
                embeddingSpaceID: Self.spaceID
            )
        ) { error in
            guard case let .nativeFailure(operation, code, message) = error as? RecallError else {
                return XCTFail("expected nativeFailure, got \(error)")
            }
            XCTAssertEqual(operation, .semanticSearch)
            XCTAssertEqual(code, .embedding)
            XCTAssertTrue(message?.contains("no LVEC pack") == true)
        }
    }

    func testSearchDecodesCanonicalResponseAndForwardsOptions() throws {
        let backend = MockBackend()
        backend.response = Self.responseJSON
        let engine = try RecallEngine(
            packs: [Data([1, 2, 3])],
            backend: backend
        )

        let response = try engine.search(
            "lost sheep",
            options: SearchOptions(topK: 3, bookHint: "Luke")
        )

        XCTAssertEqual(response.query, "lost sheep")
        XCTAssertEqual(response.results.first?.reference, "Luke 15:4")
        XCTAssertEqual(response.results.first?.confidence, .related)
        XCTAssertEqual(response.results.first?.passages, [])
        XCTAssertEqual(backend.lastRequest?.options, SearchOptions(topK: 3, bookHint: "Luke"))
        XCTAssertNil(backend.lastRequest?.embedding)
        XCTAssertNil(backend.lastRequest?.embeddingSpaceID)
    }

    func testLegacySemanticSearchForwardsEmbeddingWithoutSpaceID() throws {
        let backend = MockBackend()
        backend.response = Self.responseJSON
        let engine = try RecallEngine(packs: [Data([1])], backend: backend)

        _ = try engine.search("creation", embedding: [0.25, -0.75])

        XCTAssertEqual(backend.lastRequest?.embedding, [0.25, -0.75])
        XCTAssertNil(backend.lastRequest?.embeddingSpaceID)
    }

    func testV2SemanticSearchForwardsEmbeddingAndSpaceID() throws {
        let backend = MockBackend()
        backend.response = Self.responseJSON
        let engine = try RecallEngine(packs: [Data([1])], backend: backend)

        _ = try engine.search(
            "creation",
            embedding: [0.25, -0.75],
            embeddingSpaceID: Self.spaceID
        )

        XCTAssertEqual(backend.lastRequest?.embedding, [0.25, -0.75])
        XCTAssertEqual(backend.lastRequest?.embeddingSpaceID, Self.spaceID)
    }

    func testProviderSearchComputesEmbedding() async throws {
        let backend = MockBackend()
        backend.response = Self.responseJSON
        let engine = try RecallEngine(packs: [Data([1])], backend: backend)
        let provider = StubProvider(vector: [1, 2, 3])

        _ = try await engine.search("creation", using: provider)

        XCTAssertEqual(backend.lastRequest?.embedding, [1, 2, 3])
        XCTAssertEqual(backend.lastRequest?.embeddingSpaceID, provider.embeddingSpaceID)
    }

    func testCanonicalPlannerAndSegmentedSearchForwardCompleteContract() throws {
        let backend = MockBackend()
        backend.response = Self.responseJSON
        backend.planResponse = Self.segmentPlanJSON
        let engine = try RecallEngine(packs: [Data([1])], backend: backend)

        let plan = try RecallEngine.planQuerySegments(
            "lost sheep and prodigal son",
            backend: backend
        )
        XCTAssertEqual(plan.planner, QuerySegmentPlan.plannerID)
        XCTAssertEqual(plan.segments, ["lost sheep", "prodigal son"])

        let segments = [
            QuerySegmentEmbedding(text: plan.segments[0], embedding: [1, 0]),
            QuerySegmentEmbedding(text: plan.segments[1], embedding: [0, 1]),
        ]
        _ = try engine.search(
            "lost sheep and prodigal son",
            embedding: [0.5, 0.5],
            segments: segments,
            embeddingSpaceID: Self.spaceID,
            options: SearchOptions(topK: 4, bookHint: "Luke")
        )

        XCTAssertEqual(backend.planCount, 2)
        XCTAssertEqual(backend.segmentSearchCount, 1)
        XCTAssertEqual(backend.lastSegmentRequest?.query, "lost sheep and prodigal son")
        XCTAssertEqual(backend.lastSegmentRequest?.wholeEmbedding, [0.5, 0.5])
        XCTAssertEqual(backend.lastSegmentRequest?.segments, segments)
        XCTAssertEqual(backend.lastSegmentRequest?.embeddingSpaceID, Self.spaceID)
        XCTAssertEqual(
            backend.lastSegmentRequest?.options,
            SearchOptions(topK: 4, bookHint: "Luke")
        )
    }

    func testSegmentedSearchRejectsNoncanonicalOrInvalidClausesBeforeRetrieval() throws {
        let backend = MockBackend()
        backend.response = Self.responseJSON
        backend.planResponse = Self.segmentPlanJSON
        let engine = try RecallEngine(packs: [Data([1])], backend: backend)

        XCTAssertThrowsError(
            try engine.search(
                "lost sheep and prodigal son",
                embedding: [1, 1],
                segments: [
                    QuerySegmentEmbedding(text: "prodigal son", embedding: [1, 0]),
                    QuerySegmentEmbedding(text: "lost sheep", embedding: [0, 1]),
                ],
                embeddingSpaceID: Self.spaceID
            )
        ) { error in
            guard case .invalidQuerySegments = error as? RecallError else {
                return XCTFail("expected invalidQuerySegments, got \(error)")
            }
        }
        XCTAssertThrowsError(
            try engine.search(
                "lost sheep and prodigal son",
                embedding: [1, 1],
                segments: [],
                embeddingSpaceID: Self.spaceID
            )
        )
        XCTAssertThrowsError(
            try engine.search(
                "lost sheep and prodigal son",
                embedding: [1, 1],
                segments: [
                    QuerySegmentEmbedding(text: "lost sheep", embedding: [.nan]),
                    QuerySegmentEmbedding(text: "prodigal son", embedding: [0, 1]),
                ],
                embeddingSpaceID: Self.spaceID
            )
        )
        XCTAssertEqual(backend.segmentSearchCount, 0)
    }

    func testPlannerRejectsMalformedOrUnboundedNativePlans() throws {
        let backend = MockBackend()
        backend.planResponse = Data(
            "{\"planner\":\"unexpected\",\"segments\":[\"one\"]}".utf8
        )
        XCTAssertThrowsError(try RecallEngine.planQuerySegments("one and two", backend: backend))

        backend.planResponse = Data(
            "{\"planner\":\"verse-recall-clause-v2\",\"segments\":[\"1\",\"2\",\"3\",\"4\",\"5\"]}".utf8
        )
        XCTAssertThrowsError(try RecallEngine.planQuerySegments("one and two", backend: backend))
    }

    func testEngineInfoDecodesDenseCapabilities() throws {
        let backend = MockBackend()
        let engine = try RecallEngine(packs: [Data([1])], backend: backend)

        let info = try engine.info()

        XCTAssertEqual(info.abiVersion, 1)
        XCTAssertEqual(info.translationID, "BSB")
        XCTAssertEqual(info.verseCount, 31_086)
        XCTAssertEqual(info.lanes, ["a", "b", "c", "d"])
        XCTAssertEqual(info.dense?.version, 2)
        XCTAssertEqual(info.dense?.dims, 384)
        XCTAssertEqual(info.dense?.tableCount, 2)
        XCTAssertEqual(info.dense?.tableRoles, ["verse", "window"])
        XCTAssertEqual(info.dense?.embeddingSpaceID, Self.spaceID)
        XCTAssertEqual(backend.infoCount, 1)
    }

    func testEmbeddingSpaceIDValidatesAndRoundTripsHexAndJSON() throws {
        let uppercaseHex = Self.spaceID.hex.uppercased()
        let decoded = try EmbeddingSpaceID(hex: uppercaseHex)
        let encoded = try JSONEncoder().encode(decoded)

        XCTAssertEqual(decoded, Self.spaceID)
        XCTAssertEqual(decoded.hex, Self.spaceID.hex)
        XCTAssertEqual(try JSONDecoder().decode(EmbeddingSpaceID.self, from: encoded), Self.spaceID)

        XCTAssertThrowsError(try EmbeddingSpaceID(bytes: []))
        XCTAssertThrowsError(try EmbeddingSpaceID(bytes: Array(repeating: 0, count: 32)))
        XCTAssertThrowsError(try EmbeddingSpaceID(hex: "not-a-space-id"))
    }

    func testCloseIsDeterministicAndIdempotent() throws {
        let backend = MockBackend()
        backend.response = Self.responseJSON
        let engine = try RecallEngine(packs: [Data([1])], backend: backend)

        XCTAssertFalse(engine.isClosed)
        engine.close()
        engine.close()

        XCTAssertTrue(engine.isClosed)
        XCTAssertEqual(backend.closeCount, 1)
        XCTAssertThrowsError(try engine.search("shepherd")) { error in
            XCTAssertEqual(error as? RecallError, .closed)
        }
        XCTAssertThrowsError(try engine.info()) { error in
            XCTAssertEqual(error as? RecallError, .closed)
        }
    }

    func testCloseWaitsForInFlightNativeSearch() throws {
        let searchStarted = DispatchSemaphore(value: 0)
        let allowSearchToFinish = DispatchSemaphore(value: 0)
        let searchFinished = DispatchSemaphore(value: 0)
        let closeFinished = DispatchSemaphore(value: 0)
        let backend = MockBackend()
        backend.response = Self.responseJSON
        backend.onSearch = {
            searchStarted.signal()
            allowSearchToFinish.wait()
        }
        let engine = try RecallEngine(packs: [Data([1])], backend: backend)

        DispatchQueue.global().async {
            _ = try? engine.search("lost sheep")
            searchFinished.signal()
        }
        XCTAssertEqual(searchStarted.wait(timeout: .now() + 1), .success)

        DispatchQueue.global().async {
            engine.close()
            closeFinished.signal()
        }
        XCTAssertEqual(closeFinished.wait(timeout: .now() + 0.05), .timedOut)

        allowSearchToFinish.signal()
        XCTAssertEqual(searchFinished.wait(timeout: .now() + 1), .success)
        XCTAssertEqual(closeFinished.wait(timeout: .now() + 1), .success)
        XCTAssertTrue(engine.isClosed)
        XCTAssertEqual(backend.closeCount, 1)
    }

    func testDeinitClosesNativeHandle() throws {
        let backend = MockBackend()
        backend.response = Self.responseJSON
        var engine: RecallEngine? = try RecallEngine(packs: [Data([1])], backend: backend)
        XCTAssertNotNil(engine)

        engine = nil

        XCTAssertEqual(backend.closeCount, 1)
    }

    func testPackAndABIErrorsAreTyped() {
        let backend = MockBackend()

        XCTAssertThrowsError(try RecallEngine(packs: [], backend: backend)) { error in
            XCTAssertEqual(error as? RecallError, .noPacks)
        }
        XCTAssertThrowsError(
            try RecallEngine(packs: [Data([1]), Data()], backend: backend)
        ) { error in
            XCTAssertEqual(error as? RecallError, .emptyPack(index: 1))
        }

        backend.abi = 2
        XCTAssertThrowsError(try RecallEngine(packs: [Data([1])], backend: backend)) { error in
            XCTAssertEqual(
                error as? RecallError,
                .incompatibleABI(expected: RecallEngine.supportedABIVersion, actual: 2)
            )
        }
        XCTAssertEqual(backend.openCount, 0)
    }

    func testNativeOpenFailureIncludesCodeAndThreadLocalMessage() {
        XCTAssertThrowsError(try RecallEngine(packs: [Data([0, 1, 2, 3])])) { error in
            guard case let .nativeFailure(operation, code, message) = error as? RecallError else {
                return XCTFail("expected nativeFailure, got \(error)")
            }
            XCTAssertEqual(operation, .open)
            XCTAssertEqual(code, .open)
            XCTAssertTrue(message?.contains("engine open failed") == true)
        }
    }

    func testPackReadFailureIsTyped() {
        let path = "/definitely/missing/verse-recall.pack"

        XCTAssertThrowsError(
            try RecallEngine(packURLs: [URL(fileURLWithPath: path)])
        ) { error in
            guard case let .packReadFailed(actualPath, _) = error as? RecallError else {
                return XCTFail("expected packReadFailed, got \(error)")
            }
            XCTAssertEqual(actualPath, path)
        }
    }

    func testInvalidQueriesAndOptionsNeverReachBackend() throws {
        let backend = MockBackend()
        backend.response = Self.responseJSON
        let engine = try RecallEngine(packs: [Data([1])], backend: backend)

        XCTAssertThrowsError(try engine.search(" \n")) { error in
            XCTAssertEqual(error as? RecallError, .invalidQuery)
        }
        XCTAssertThrowsError(try engine.search("query\0truncated")) { error in
            XCTAssertEqual(error as? RecallError, .invalidQuery)
        }
        XCTAssertThrowsError(
            try engine.search("query", options: SearchOptions(topK: 0))
        ) { error in
            XCTAssertEqual(error as? RecallError, .invalidTopK(0))
        }
        XCTAssertThrowsError(
            try engine.search("query", options: SearchOptions(topK: 101))
        ) { error in
            XCTAssertEqual(error as? RecallError, .invalidTopK(101))
        }
        XCTAssertThrowsError(
            try engine.search("query", options: SearchOptions(bookHint: "  "))
        ) { error in
            XCTAssertEqual(error as? RecallError, .invalidBookHint)
        }
        XCTAssertThrowsError(
            try engine.search("query", options: SearchOptions(bookHint: "John\0Luke"))
        ) { error in
            XCTAssertEqual(error as? RecallError, .invalidBookHint)
        }
        XCTAssertEqual(backend.searchCount, 0)
    }

    func testInvalidEmbeddingsNeverReachBackend() throws {
        let backend = MockBackend()
        backend.response = Self.responseJSON
        let engine = try RecallEngine(packs: [Data([1])], backend: backend)

        for embedding in [[], [0, 0], [.nan], [.infinity]] as [[Float]] {
            XCTAssertThrowsError(try engine.search("query", embedding: embedding)) { error in
                guard case .invalidEmbedding = error as? RecallError else {
                    return XCTFail("expected invalidEmbedding, got \(error)")
                }
            }
        }
        XCTAssertEqual(backend.searchCount, 0)
    }

    func testInvalidNativeJSONIsTyped() throws {
        let backend = MockBackend()
        backend.response = Data("not json".utf8)
        let engine = try RecallEngine(packs: [Data([1])], backend: backend)

        XCTAssertThrowsError(try engine.search("query")) { error in
            guard case .invalidResponse = error as? RecallError else {
                return XCTFail("expected invalidResponse, got \(error)")
            }
        }

        backend.infoResponse = Data("not json".utf8)
        XCTAssertThrowsError(try engine.info()) { error in
            guard case .invalidResponse = error as? RecallError else {
                return XCTFail("expected invalidResponse, got \(error)")
            }
        }
    }

    func testPassageRangesDecodeFromSnakeCase() throws {
        let json = """
        {
          "query":"the fall",
          "results":[{
            "ref":"Genesis 3:1",
            "text":"Now the serpent was more crafty...",
            "score":1.0,
            "confidence":"recovered",
            "lanes":{"a":0,"b":0,"c":0,"d":1},
            "matched_terms":[],
            "expanded_terms":["fall"],
            "passages":[{"start_ref":"Genesis 3:1","end_ref":"Genesis 3:24"}],
            "chips":["event"]
          }]
        }
        """

        let result = try JSONDecoder().decode(SearchResponse.self, from: Data(json.utf8))

        XCTAssertEqual(
            result.results[0].passages,
            [PassageRange(startReference: "Genesis 3:1", endReference: "Genesis 3:24")]
        )
    }

    func testPortableQueryAdapterTransformsAndFailsClosed() throws {
        let adapter = try QueryAdapter(
            data: Self.queryAdapterData(
                input: 2,
                output: 3,
                matrix: [0, 1, 0, 1, 0, 1]
            )
        )
        XCTAssertEqual(adapter.inputDimensions, 2)
        XCTAssertEqual(adapter.outputDimensions, 3)
        let transformed = try adapter.apply(to: [3, 4])
        XCTAssertEqual(transformed.count, 3)
        XCTAssertEqual(transformed[0], 0.624695, accuracy: 0.000001)
        XCTAssertEqual(transformed[1], 0.468521, accuracy: 0.000001)
        XCTAssertEqual(transformed[2], 0.624695, accuracy: 0.000001)

        XCTAssertThrowsError(try QueryAdapter(data: Data([1, 2, 3])))
        XCTAssertThrowsError(try adapter.apply(to: [1, 2, 3]))
        XCTAssertThrowsError(try adapter.apply(to: [0, 0]))
    }

    func testQueryAdapterMatchesSharedDoubleAccumulationGolden() throws {
        var repositoryRoot = URL(fileURLWithPath: #filePath)
        for _ in 0..<5 {
            repositoryRoot.deleteLastPathComponent()
        }
        let fixtureURL = repositoryRoot
            .appendingPathComponent("tests/fixtures/vrqadp01-golden-v1.json")
        let object = try JSONSerialization.jsonObject(with: Data(contentsOf: fixtureURL))
        let fixture = try XCTUnwrap(object as? [String: Any])
        XCTAssertEqual(
            fixture["schema"] as? String,
            "verse-recall-vrqadp01-arithmetic-v1"
        )
        let inputDimensions = try XCTUnwrap(fixture["input_dimensions"] as? NSNumber)
        let outputDimensions = try XCTUnwrap(fixture["output_dimensions"] as? NSNumber)
        let matrix = try XCTUnwrap(fixture["matrix"] as? [NSNumber]).map(\.floatValue)
        let embedding = try XCTUnwrap(fixture["embedding"] as? [NSNumber]).map(\.floatValue)
        let expectedBits = try XCTUnwrap(fixture["expected_f32_bits"] as? [String])

        let adapter = try QueryAdapter(
            data: Self.queryAdapterData(
                input: inputDimensions.uint32Value,
                output: outputDimensions.uint32Value,
                matrix: matrix
            )
        )
        let output = try adapter.apply(to: embedding)
        let actualBits = output.map { String(format: "%08x", $0.bitPattern) }
        XCTAssertEqual(actualBits, expectedBits)
    }

    private static func queryAdapterData(
        input: UInt32,
        output: UInt32,
        matrix: [Float]
    ) -> Data {
        var data = Data("VRQADP01".utf8)
        for value in [input, output] {
            let littleEndian = value.littleEndian
            withUnsafeBytes(of: littleEndian) { data.append(contentsOf: $0) }
        }
        for value in matrix {
            let littleEndian = value.bitPattern.littleEndian
            withUnsafeBytes(of: littleEndian) { data.append(contentsOf: $0) }
        }
        return data
    }

    private static let responseJSON = Data(
        """
        {
          "query":"lost sheep",
          "results":[{
            "ref":"Luke 15:4",
            "text":"Suppose one of you has a hundred sheep and loses one of them.",
            "score":1.0,
            "confidence":"related",
            "lanes":{"a":1,"b":0,"c":0,"d":0},
            "matched_terms":["lost","sheep"],
            "expanded_terms":[],
            "chips":["lost","sheep"]
          }]
        }
        """.utf8
    )

    private static let segmentPlanJSON = Data(
        """
        {
          "planner":"verse-recall-clause-v2",
          "segments":["lost sheep","prodigal son"]
        }
        """.utf8
    )

    private static let spaceID = try! EmbeddingSpaceID(
        bytes: [1] + Array(repeating: 0, count: 31)
    )

    fileprivate static let engineInfoJSON = Data(
        """
        {
          "abi_version":1,
          "translation_id":"BSB",
          "verse_count":31086,
          "lanes":["a","b","c","d"],
          "dense":{
            "version":2,
            "dims":384,
            "table_count":2,
            "table_roles":["verse","window"],
            "embedding_space_id":"0100000000000000000000000000000000000000000000000000000000000000"
          }
        }
        """.utf8
    )
}

private final class MockBackend: RecallBackend {
    var abi: UInt32 = RecallEngine.supportedABIVersion
    var response = Data()
    var infoResponse = RecallEngineTests.engineInfoJSON
    var planResponse = Data("{\"planner\":\"verse-recall-clause-v2\",\"segments\":[]}".utf8)
    var lastRequest: BackendSearchRequest?
    var lastSegmentRequest: BackendSegmentSearchRequest?
    var openCount = 0
    var searchCount = 0
    var closeCount = 0
    var infoCount = 0
    var planCount = 0
    var segmentSearchCount = 0
    var onSearch: (() -> Void)?

    func abiVersion() -> UInt32 {
        abi
    }

    func open(packs: [Data]) throws -> OpaquePointer {
        openCount += 1
        return OpaquePointer(bitPattern: openCount)!
    }

    func info(handle: OpaquePointer) throws -> Data {
        infoCount += 1
        return infoResponse
    }

    func plan(query: String) throws -> Data {
        planCount += 1
        return planResponse
    }

    func search(handle: OpaquePointer, request: BackendSearchRequest) throws -> Data {
        searchCount += 1
        lastRequest = request
        onSearch?()
        return response
    }

    func searchSegments(
        handle: OpaquePointer,
        request: BackendSegmentSearchRequest
    ) throws -> Data {
        segmentSearchCount += 1
        lastSegmentRequest = request
        onSearch?()
        return response
    }

    func close(handle: OpaquePointer) {
        closeCount += 1
    }
}

private struct StubProvider: RecallEmbeddingProvider {
    let vector: [Float]
    let embeddingSpaceID = try! EmbeddingSpaceID(
        bytes: [1] + Array(repeating: 0, count: 31)
    )

    func embedding(for query: String) async throws -> [Float] {
        vector
    }
}
