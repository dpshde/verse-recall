import Foundation

/// Search-time controls shared by lexical and semantic queries.
public struct SearchOptions: Sendable, Equatable {
    public static let maximumTopK = 100

    /// Maximum number of results. The public SDK deliberately caps this to
    /// keep accidental UI queries bounded.
    public var topK: Int
    /// Optional soft book or testament prior, for example `"John"` or `"NT"`.
    public var bookHint: String?

    public init(topK: Int = 10, bookHint: String? = nil) {
        self.topK = topK
        self.bookHint = bookHint
    }
}

public struct LaneScores: Codable, Sendable, Equatable {
    public let a: Float
    public let b: Float
    public let c: Float
    public let d: Float

    public init(a: Float, b: Float, c: Float, d: Float) {
        self.a = a
        self.b = b
        self.c = c
        self.d = d
    }
}

public enum RecallConfidence: String, Codable, Sendable, Equatable {
    case recovered
    case related
    case oftenMisremembered = "often-misremembered"
}

public struct PassageRange: Codable, Sendable, Equatable {
    public let startReference: String
    public let endReference: String

    public init(startReference: String, endReference: String) {
        self.startReference = startReference
        self.endReference = endReference
    }

    enum CodingKeys: String, CodingKey {
        case startReference = "start_ref"
        case endReference = "end_ref"
    }
}

public struct SearchResult: Codable, Sendable, Equatable {
    public let reference: String
    public let text: String
    /// Opaque response-local ordering score. Do not persist or compare it
    /// across engine versions.
    public let score: Float
    public let confidence: RecallConfidence
    public let lanes: LaneScores
    public let matchedTerms: [String]
    public let expandedTerms: [String]
    public let passages: [PassageRange]
    public let chips: [String]

    public init(
        reference: String,
        text: String,
        score: Float,
        confidence: RecallConfidence,
        lanes: LaneScores,
        matchedTerms: [String],
        expandedTerms: [String],
        passages: [PassageRange] = [],
        chips: [String]
    ) {
        self.reference = reference
        self.text = text
        self.score = score
        self.confidence = confidence
        self.lanes = lanes
        self.matchedTerms = matchedTerms
        self.expandedTerms = expandedTerms
        self.passages = passages
        self.chips = chips
    }

    enum CodingKeys: String, CodingKey {
        case reference = "ref"
        case text
        case score
        case confidence
        case lanes
        case matchedTerms = "matched_terms"
        case expandedTerms = "expanded_terms"
        case passages
        case chips
    }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        reference = try values.decode(String.self, forKey: .reference)
        text = try values.decode(String.self, forKey: .text)
        score = try values.decode(Float.self, forKey: .score)
        confidence = try values.decode(RecallConfidence.self, forKey: .confidence)
        lanes = try values.decode(LaneScores.self, forKey: .lanes)
        matchedTerms = try values.decode([String].self, forKey: .matchedTerms)
        expandedTerms = try values.decode([String].self, forKey: .expandedTerms)
        passages = try values.decodeIfPresent([PassageRange].self, forKey: .passages) ?? []
        chips = try values.decode([String].self, forKey: .chips)
    }
}

public struct SearchResponse: Codable, Sendable, Equatable {
    public let query: String
    public let results: [SearchResult]

    public init(query: String, results: [SearchResult]) {
        self.query = query
        self.results = results
    }
}

/// Result of the corpus-independent canonical clause planner.
///
/// A valid plan may contain no clauses when the query cannot be split safely.
/// Segmented semantic search accepts only the exact ordered clauses returned
/// for the original query and is bounded to four clauses.
public struct QuerySegmentPlan: Codable, Sendable, Equatable {
    public static let plannerID = "verse-recall-clause-v2"
    public static let maximumSegments = 4

    public let planner: String
    public let segments: [String]

    public init(planner: String, segments: [String]) {
        self.planner = planner
        self.segments = segments
    }
}

/// One canonical query clause paired with the host embedding for that exact
/// text. The native core independently verifies the clause plan.
public struct QuerySegmentEmbedding: Codable, Sendable, Equatable {
    public let text: String
    public let embedding: [Float]

    public init(text: String, embedding: [Float]) {
        self.text = text
        self.embedding = embedding
    }
}

/// Opaque identity of the complete semantic encoder contract used to build an
/// LVEC v2/v3 pack. It is exactly 32 nonzero bytes and is normally exchanged as a
/// lowercase 64-character hexadecimal string.
public struct EmbeddingSpaceID: Codable, Sendable, Hashable {
    public static let byteCount = 32

    public let bytes: [UInt8]

    public init(bytes: [UInt8]) throws {
        guard bytes.count == Self.byteCount else {
            throw RecallError.invalidEmbeddingSpaceID(
                reason: "expected \(Self.byteCount) bytes, received \(bytes.count)"
            )
        }
        guard bytes.contains(where: { $0 != 0 }) else {
            throw RecallError.invalidEmbeddingSpaceID(reason: "the ID must not be all zero")
        }
        self.bytes = bytes
    }

    public init(hex: String) throws {
        guard hex.utf8.count == Self.byteCount * 2 else {
            throw RecallError.invalidEmbeddingSpaceID(
                reason: "expected \(Self.byteCount * 2) hexadecimal characters"
            )
        }
        var decoded: [UInt8] = []
        decoded.reserveCapacity(Self.byteCount)
        var index = hex.startIndex
        for _ in 0..<Self.byteCount {
            let next = hex.index(index, offsetBy: 2)
            guard let byte = UInt8(hex[index..<next], radix: 16) else {
                throw RecallError.invalidEmbeddingSpaceID(reason: "the ID is not hexadecimal")
            }
            decoded.append(byte)
            index = next
        }
        try self.init(bytes: decoded)
    }

    public var hex: String {
        bytes.map { String(format: "%02x", $0) }.joined()
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        try self.init(hex: container.decode(String.self))
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(hex)
    }
}

/// Optional Lane C metadata advertised by the opened engine.
public struct DenseEngineInfo: Codable, Sendable, Equatable {
    public let version: UInt16
    public let dims: Int
    public let tableCount: Int
    /// Aligned representation roles in table order (LVEC v3-aware runtimes).
    public let tableRoles: [String]?
    /// Present for LVEC v2/v3 and absent for legacy LVEC v1.
    public let embeddingSpaceID: EmbeddingSpaceID?

    public init(
        version: UInt16,
        dims: Int,
        tableCount: Int,
        tableRoles: [String]? = nil,
        embeddingSpaceID: EmbeddingSpaceID?
    ) {
        self.version = version
        self.dims = dims
        self.tableCount = tableCount
        self.tableRoles = tableRoles
        self.embeddingSpaceID = embeddingSpaceID
    }

    enum CodingKeys: String, CodingKey {
        case version
        case dims
        case tableCount = "table_count"
        case tableRoles = "table_roles"
        case embeddingSpaceID = "embedding_space_id"
    }
}

/// Stable runtime capabilities for one opened collection of packs.
public struct EngineInfo: Codable, Sendable, Equatable {
    public let abiVersion: UInt32
    public let translationID: String
    public let verseCount: Int
    /// Lowercase lane identifiers (`a`...`d`) currently available at runtime.
    public let lanes: [String]
    public let dense: DenseEngineInfo?

    public init(
        abiVersion: UInt32,
        translationID: String,
        verseCount: Int,
        lanes: [String],
        dense: DenseEngineInfo?
    ) {
        self.abiVersion = abiVersion
        self.translationID = translationID
        self.verseCount = verseCount
        self.lanes = lanes
        self.dense = dense
    }

    enum CodingKeys: String, CodingKey {
        case abiVersion = "abi_version"
        case translationID = "translation_id"
        case verseCount = "verse_count"
        case lanes
        case dense
    }
}

/// Host-owned semantic encoders conform to this protocol. The provider must
/// use the same model, pooling, normalization, and query prefix as the LVEC
/// pack loaded into the engine.
public protocol RecallEmbeddingProvider: Sendable {
    var embeddingSpaceID: EmbeddingSpaceID { get }
    func embedding(for query: String) async throws -> [Float]
}
