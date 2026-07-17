import CVerseRecall
import Foundation

struct BackendSearchRequest {
    let query: String
    let options: SearchOptions
    let embedding: [Float]?
    let embeddingSpaceID: EmbeddingSpaceID?
}

struct BackendSegmentSearchRequest {
    let query: String
    let options: SearchOptions
    let wholeEmbedding: [Float]
    let segments: [QuerySegmentEmbedding]
    let embeddingSpaceID: EmbeddingSpaceID
}

protocol RecallBackend: AnyObject {
    func abiVersion() -> UInt32
    func open(packs: [Data]) throws -> OpaquePointer
    func info(handle: OpaquePointer) throws -> Data
    func plan(query: String) throws -> Data
    func search(handle: OpaquePointer, request: BackendSearchRequest) throws -> Data
    func searchSegments(
        handle: OpaquePointer,
        request: BackendSegmentSearchRequest
    ) throws -> Data
    func close(handle: OpaquePointer)
}

final class NativeRecallBackend: RecallBackend {
    func abiVersion() -> UInt32 {
        recall_abi_version()
    }

    func open(packs: [Data]) throws -> OpaquePointer {
        // NSData supplies stable byte addresses for the duration of this call.
        // The C API copies every pack before returning.
        let stablePacks = packs.map { $0 as NSData }
        let pointers: [UnsafePointer<UInt8>?] = stablePacks.map { data in
            data.bytes.assumingMemoryBound(to: UInt8.self)
        }
        let lengths = stablePacks.map(\.length)
        var error = recall_err(rawValue: 0)

        let engine = pointers.withUnsafeBufferPointer { pointerBuffer in
            lengths.withUnsafeBufferPointer { lengthBuffer in
                recall_open_v2(
                    pointerBuffer.baseAddress,
                    lengthBuffer.baseAddress,
                    stablePacks.count,
                    &error
                )
            }
        }

        return try withExtendedLifetime(stablePacks) {
            guard let engine else {
                throw Self.nativeFailure(operation: .open, error: error)
            }
            return engine
        }
    }

    func info(handle: OpaquePointer) throws -> Data {
        var error = recall_err(rawValue: 0)
        guard let result = recall_engine_info_json(handle, &error) else {
            throw Self.nativeFailure(operation: .engineInfo, error: error)
        }
        return try Self.copyAndFree(result)
    }

    func plan(query: String) throws -> Data {
        var error = recall_err(rawValue: 0)
        let result = query.withCString { queryPointer in
            recall_plan_query_segments_json(queryPointer, &error)
        }
        guard let result else {
            throw Self.nativeFailure(operation: .queryPlan, error: error)
        }
        return try Self.copyAndFree(result)
    }

    func search(handle: OpaquePointer, request: BackendSearchRequest) throws -> Data {
        var error = recall_err(rawValue: 0)
        let result: UnsafeMutablePointer<CChar>? = request.query.withCString { queryPointer in
            withOptions(request.options) { optionsPointer in
                if let embedding = request.embedding {
                    return embedding.withUnsafeBufferPointer { vector in
                        if let spaceID = request.embeddingSpaceID {
                            return spaceID.bytes.withUnsafeBufferPointer { space in
                                recall_search_with_embedding_json_v2(
                                    handle,
                                    queryPointer,
                                    space.baseAddress,
                                    space.count,
                                    vector.baseAddress,
                                    vector.count,
                                    optionsPointer,
                                    &error
                                )
                            }
                        }
                        // Compatibility path for LVEC v1. Version 2 packs
                        // deliberately reject this legacy symbol in the core.
                        return recall_search_with_embedding_json(
                            handle,
                            queryPointer,
                            vector.baseAddress,
                            vector.count,
                            optionsPointer
                        )
                    }
                }
                return recall_search_json_v2(handle, queryPointer, optionsPointer, &error)
            }
        }

        guard let result else {
            let operation: RecallError.NativeOperation = request.embedding == nil
                ? .search
                : .semanticSearch
            // The legacy LVEC v1 function has no error out-parameter, but it
            // still records a detailed thread-local message. Preserve a typed
            // semantic error code for that compatibility path.
            if request.embedding != nil, request.embeddingSpaceID == nil {
                let message = Self.lastErrorMessage()
                throw RecallError.nativeFailure(
                    operation: operation,
                    code: .embedding,
                    message: message
                )
            }
            throw Self.nativeFailure(operation: operation, error: error)
        }
        return try Self.copyAndFree(result)
    }

    func searchSegments(
        handle: OpaquePointer,
        request: BackendSegmentSearchRequest
    ) throws -> Data {
        var error = recall_err(rawValue: 0)
        let result: UnsafeMutablePointer<CChar>? = request.query.withCString { queryPointer in
            withOptions(request.options) { optionsPointer in
                request.embeddingSpaceID.bytes.withUnsafeBufferPointer { space in
                    request.wholeEmbedding.withUnsafeBufferPointer { wholeVector in
                        withRawSegments(request.segments) { rawSegments in
                            rawSegments.withUnsafeBufferPointer { segmentBuffer in
                                recall_search_with_segment_embeddings_json_v2(
                                    handle,
                                    queryPointer,
                                    space.baseAddress,
                                    space.count,
                                    wholeVector.baseAddress,
                                    wholeVector.count,
                                    segmentBuffer.baseAddress,
                                    segmentBuffer.count,
                                    optionsPointer,
                                    &error
                                )
                            }
                        }
                    }
                }
            }
        }

        guard let result else {
            throw Self.nativeFailure(operation: .semanticSearch, error: error)
        }
        return try Self.copyAndFree(result)
    }

    func close(handle: OpaquePointer) {
        recall_close(handle)
    }

    private func withOptions<Result>(
        _ options: SearchOptions,
        _ body: (UnsafePointer<recall_opts>) throws -> Result
    ) rethrows -> Result {
        if let hint = options.bookHint {
            return try hint.withCString { hintPointer in
                var raw = recall_opts(
                    top_k: UInt32(options.topK),
                    book_hint: hintPointer,
                    flags: 0
                )
                return try withUnsafePointer(to: &raw, body)
            }
        }
        var raw = recall_opts(
            top_k: UInt32(options.topK),
            book_hint: nil,
            flags: 0
        )
        return try withUnsafePointer(to: &raw, body)
    }

    /// Keep each String and Array storage closure alive until the C call
    /// returns. Building the raw array only at the recursion base prevents
    /// escaped pointers while supporting the fixed one-to-four segment bound.
    private func withRawSegments<Result>(
        _ segments: [QuerySegmentEmbedding],
        index: Int = 0,
        accumulated: [recall_query_segment] = [],
        _ body: ([recall_query_segment]) -> Result
    ) -> Result {
        guard index < segments.count else {
            return body(accumulated)
        }
        let segment = segments[index]
        return segment.text.withCString { textPointer in
            segment.embedding.withUnsafeBufferPointer { vector in
                var next = accumulated
                next.append(
                    recall_query_segment(
                        text_utf8: textPointer,
                        embedding: vector.baseAddress,
                        embedding_len: vector.count
                    )
                )
                return withRawSegments(
                    segments,
                    index: index + 1,
                    accumulated: next,
                    body
                )
            }
        }
    }

    private static func errorCode(_ value: recall_err) -> RecallError.CErrorCode {
        RecallError.CErrorCode(rawValue: Int32(bitPattern: value.rawValue)) ?? .unknown
    }

    private static func nativeFailure(
        operation: RecallError.NativeOperation,
        error: recall_err
    ) -> RecallError {
        RecallError.nativeFailure(
            operation: operation,
            code: errorCode(error),
            message: lastErrorMessage()
        )
    }

    private static func lastErrorMessage() -> String? {
        guard let message = recall_last_error_message() else { return nil }
        defer { recall_free_str(message) }
        return String(validatingUTF8: message)
    }

    private static func copyAndFree(_ result: UnsafeMutablePointer<CChar>) throws -> Data {
        defer { recall_free_str(result) }
        guard let json = String(validatingUTF8: result), let bytes = json.data(using: .utf8) else {
            throw RecallError.invalidResponse(description: "response was not UTF-8")
        }
        return bytes
    }
}
