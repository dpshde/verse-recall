import Foundation

/// Stable failures surfaced by the Swift wrapper.
public enum RecallError: Error, Sendable, Equatable {
    case noPacks
    case emptyPack(index: Int)
    case packReadFailed(path: String, description: String)
    case incompatibleABI(expected: UInt32, actual: UInt32)
    case openFailed(code: CErrorCode)
    case closed
    case invalidQuery
    case invalidTopK(Int)
    case invalidBookHint
    case invalidEmbedding(reason: String)
    case invalidEmbeddingSpaceID(reason: String)
    case invalidQuerySegments(reason: String)
    case invalidQueryAdapter(reason: String)
    case searchFailed
    case invalidResponse(description: String)
    case nativeFailure(operation: NativeOperation, code: CErrorCode, message: String?)

    public enum NativeOperation: String, Sendable, Equatable {
        case open
        case search
        case semanticSearch = "semantic-search"
        case queryPlan = "query-plan"
        case engineInfo = "engine-info"
    }

    public enum CErrorCode: Int32, Sendable, Equatable {
        case ok = 0
        case null = 1
        case open = 2
        case utf8 = 3
        case `internal` = 4
        case invalidArgument = 5
        case search = 6
        case embedding = 7
        case embeddingSpace = 8
        case panic = 9
        case json = 10
        case unknown = -1
    }
}

extension RecallError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .noPacks:
            return "At least one verse-recall pack is required."
        case let .emptyPack(index):
            return "Pack at index \(index) is empty."
        case let .packReadFailed(path, description):
            return "Could not read pack at \(path): \(description)"
        case let .incompatibleABI(expected, actual):
            return "The native recall ABI is \(actual); this SDK requires \(expected)."
        case let .openFailed(code):
            return "The native engine could not open the supplied packs (C error \(code.rawValue))."
        case .closed:
            return "The recall engine is closed."
        case .invalidQuery:
            return "The query must contain at least one non-whitespace character."
        case let .invalidTopK(value):
            return "topK must be in 1...\(SearchOptions.maximumTopK); received \(value)."
        case .invalidBookHint:
            return "bookHint must be nil or contain at least one non-whitespace character."
        case let .invalidEmbedding(reason):
            return "The query embedding is invalid: \(reason)."
        case let .invalidEmbeddingSpaceID(reason):
            return "The embedding-space ID is invalid: \(reason)."
        case let .invalidQuerySegments(reason):
            return "The query segments are invalid: \(reason)."
        case let .invalidQueryAdapter(reason):
            return "The query adapter is invalid: \(reason)."
        case .searchFailed:
            return "The native engine could not complete the search."
        case let .invalidResponse(description):
            return "The native engine returned invalid JSON: \(description)"
        case let .nativeFailure(operation, code, message):
            let detail = message.map { ": \($0)" } ?? ""
            return "Native \(operation.rawValue) failed with C error \(code.rawValue)\(detail)"
        }
    }
}
