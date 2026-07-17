import Foundation

/// Thread-safe owner of one native verse-recall engine.
///
/// `close()` is deterministic and idempotent. It is safe to search and close
/// from different threads: native calls are serialized so the handle cannot be
/// freed while a query is in progress.
public final class RecallEngine: @unchecked Sendable {
    public static let supportedABIVersion: UInt32 = 1

    private let lock = NSLock()
    private let backend: RecallBackend
    private var handle: OpaquePointer?

    public convenience init(packs: [Data]) throws {
        try self.init(packs: packs, backend: NativeRecallBackend())
    }

    public convenience init(packURLs: [URL]) throws {
        var packs: [Data] = []
        packs.reserveCapacity(packURLs.count)
        for url in packURLs {
            do {
                packs.append(try Data(contentsOf: url, options: [.mappedIfSafe]))
            } catch {
                throw RecallError.packReadFailed(
                    path: url.path,
                    description: String(describing: error)
                )
            }
        }
        try self.init(packs: packs)
    }

    init(
        packs: [Data],
        backend: RecallBackend,
        expectedABIVersion: UInt32 = RecallEngine.supportedABIVersion
    ) throws {
        guard !packs.isEmpty else {
            throw RecallError.noPacks
        }
        if let index = packs.firstIndex(where: \.isEmpty) {
            throw RecallError.emptyPack(index: index)
        }

        let actualABI = backend.abiVersion()
        guard actualABI == expectedABIVersion else {
            throw RecallError.incompatibleABI(expected: expectedABIVersion, actual: actualABI)
        }

        self.backend = backend
        handle = try backend.open(packs: packs)
    }

    deinit {
        close()
    }

    public var isClosed: Bool {
        lock.lock()
        defer { lock.unlock() }
        return handle == nil
    }

    public func close() {
        lock.lock()
        defer { lock.unlock() }
        guard let handle else { return }
        self.handle = nil
        backend.close(handle: handle)
    }

    public func search(
        _ query: String,
        options: SearchOptions = SearchOptions()
    ) throws -> SearchResponse {
        try performSearch(
            query,
            options: options,
            embedding: nil,
            embeddingSpaceID: nil
        )
    }

    /// Run the corpus-independent canonical query-clause planner. No engine or
    /// pack is opened or consulted.
    public static func planQuerySegments(_ query: String) throws -> QuerySegmentPlan {
        try planQuerySegments(query, backend: NativeRecallBackend())
    }

    /// Search with a whole-query vector and independently embedded canonical
    /// clauses. The plan is recomputed before entering retrieval; arbitrary,
    /// reordered, missing, or extra clause texts fail closed.
    public func search(
        _ query: String,
        embedding: [Float],
        segments: [QuerySegmentEmbedding],
        embeddingSpaceID: EmbeddingSpaceID,
        options: SearchOptions = SearchOptions()
    ) throws -> SearchResponse {
        try Self.validate(query: query, options: options)
        try Self.validate(embedding: embedding)
        try Self.validate(segments: segments)

        lock.lock()
        defer { lock.unlock() }
        guard let handle else {
            throw RecallError.closed
        }

        let plan = try Self.planQuerySegments(query, backend: backend)
        let suppliedTexts = segments.map(\.text)
        guard plan.segments == suppliedTexts else {
            throw RecallError.invalidQuerySegments(
                reason: "clause texts must exactly match the canonical ordered plan"
            )
        }

        let bytes = try backend.searchSegments(
            handle: handle,
            request: BackendSegmentSearchRequest(
                query: query,
                options: options,
                wholeEmbedding: embedding,
                segments: segments,
                embeddingSpaceID: embeddingSpaceID
            )
        )
        return try Self.decodeSearch(bytes)
    }

    /// Legacy LVEC v1 semantic search. LVEC v2 packs reject this path because
    /// they require a verifiable embedding-space ID.
    public func search(
        _ query: String,
        embedding: [Float],
        options: SearchOptions = SearchOptions()
    ) throws -> SearchResponse {
        try performSearch(
            query,
            options: options,
            embedding: embedding,
            embeddingSpaceID: nil
        )
    }

    /// Search an LVEC v2 pack with a host-computed semantic query vector and
    /// its required encoder-contract identity.
    public func search(
        _ query: String,
        embedding: [Float],
        embeddingSpaceID: EmbeddingSpaceID,
        options: SearchOptions = SearchOptions()
    ) throws -> SearchResponse {
        try performSearch(
            query,
            options: options,
            embedding: embedding,
            embeddingSpaceID: embeddingSpaceID
        )
    }

    /// Compute a semantic vector outside the core and search with it.
    public func search(
        _ query: String,
        using provider: any RecallEmbeddingProvider,
        options: SearchOptions = SearchOptions()
    ) async throws -> SearchResponse {
        try Self.validate(query: query, options: options)
        let embedding = try await provider.embedding(for: query)
        return try search(
            query,
            embedding: embedding,
            embeddingSpaceID: provider.embeddingSpaceID,
            options: options
        )
    }

    /// Return immutable pack and lane capabilities reported by the native
    /// engine, including LVEC dimensions and the required v2 space ID.
    public func info() throws -> EngineInfo {
        lock.lock()
        defer { lock.unlock() }
        guard let handle else {
            throw RecallError.closed
        }
        let bytes = try backend.info(handle: handle)
        do {
            return try JSONDecoder().decode(EngineInfo.self, from: bytes)
        } catch {
            throw RecallError.invalidResponse(description: String(describing: error))
        }
    }

    private func performSearch(
        _ query: String,
        options: SearchOptions,
        embedding: [Float]?,
        embeddingSpaceID: EmbeddingSpaceID?
    ) throws -> SearchResponse {
        try Self.validate(query: query, options: options)
        if let embedding {
            try Self.validate(embedding: embedding)
        }

        lock.lock()
        defer { lock.unlock() }
        guard let handle else {
            throw RecallError.closed
        }
        let bytes = try backend.search(
            handle: handle,
            request: BackendSearchRequest(
                query: query,
                options: options,
                embedding: embedding,
                embeddingSpaceID: embeddingSpaceID
            )
        )
        return try Self.decodeSearch(bytes)
    }

    private static func validate(query: String, options: SearchOptions) throws {
        guard !query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              !query.contains("\0")
        else {
            throw RecallError.invalidQuery
        }
        guard (1...SearchOptions.maximumTopK).contains(options.topK) else {
            throw RecallError.invalidTopK(options.topK)
        }
        if let hint = options.bookHint {
            guard !hint.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                  !hint.contains("\0")
            else {
                throw RecallError.invalidBookHint
            }
        }
    }

    private static func validate(embedding: [Float]) throws {
        guard !embedding.isEmpty else {
            throw RecallError.invalidEmbedding(reason: "the vector is empty")
        }
        guard embedding.allSatisfy(\.isFinite) else {
            throw RecallError.invalidEmbedding(reason: "every component must be finite")
        }
        var squaredNorm: Double = 0
        for component in embedding {
            let value = Double(component)
            squaredNorm += value * value
        }
        guard squaredNorm.isFinite, squaredNorm > 0 else {
            throw RecallError.invalidEmbedding(reason: "the vector norm must be finite and nonzero")
        }
    }

    private static func validate(segments: [QuerySegmentEmbedding]) throws {
        guard (1...QuerySegmentPlan.maximumSegments).contains(segments.count) else {
            throw RecallError.invalidQuerySegments(
                reason: "expected between 1 and \(QuerySegmentPlan.maximumSegments) clauses"
            )
        }
        for (index, segment) in segments.enumerated() {
            guard !segment.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                  !segment.text.contains("\0")
            else {
                throw RecallError.invalidQuerySegments(reason: "clause \(index) has invalid text")
            }
            do {
                try validate(embedding: segment.embedding)
            } catch {
                throw RecallError.invalidQuerySegments(
                    reason: "clause \(index) has an invalid embedding"
                )
            }
        }
    }

    private static func decodeSearch(_ bytes: Data) throws -> SearchResponse {
        do {
            return try JSONDecoder().decode(SearchResponse.self, from: bytes)
        } catch {
            throw RecallError.invalidResponse(description: String(describing: error))
        }
    }

    static func planQuerySegments(
        _ query: String,
        backend: RecallBackend
    ) throws -> QuerySegmentPlan {
        try validate(query: query, options: SearchOptions())
        let bytes = try backend.plan(query: query)
        let plan: QuerySegmentPlan
        do {
            plan = try JSONDecoder().decode(QuerySegmentPlan.self, from: bytes)
        } catch {
            throw RecallError.invalidResponse(description: String(describing: error))
        }
        guard plan.planner == QuerySegmentPlan.plannerID,
              plan.segments.isEmpty || (2...QuerySegmentPlan.maximumSegments).contains(plan.segments.count),
              plan.segments.allSatisfy({ segment in
                  !segment.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty &&
                      !segment.contains("\0")
              })
        else {
            throw RecallError.invalidResponse(description: "invalid canonical query plan")
        }
        return plan
    }
}
