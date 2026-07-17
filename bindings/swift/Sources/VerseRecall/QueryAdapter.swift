import Foundation

/// Portable query-only transform used by asymmetric semantic packs.
///
/// `VRQADP01` is a row-major Float32 matrix. The host applies it after its
/// encoder and before calling semantic search; the content-addressed embedding
/// space ID binds the exact adapter bytes to the LVEC corpus.
public struct QueryAdapter: Sendable, Equatable {
    public let inputDimensions: Int
    public let outputDimensions: Int
    private let matrix: [Float]

    public init(data: Data) throws {
        let expectedMagic = Array("VRQADP01".utf8)
        guard data.count >= 16, Array(data.prefix(8)) == expectedMagic else {
            throw RecallError.invalidQueryAdapter(reason: "missing VRQADP01 header")
        }
        let input = Int(Self.readUInt32(data, offset: 8))
        let output = Int(Self.readUInt32(data, offset: 12))
        let (valueCount, countOverflow) = input.multipliedReportingOverflow(by: output)
        let (valueBytes, byteOverflow) = valueCount.multipliedReportingOverflow(by: 4)
        let (expectedBytes, headerOverflow) = valueBytes.addingReportingOverflow(16)
        guard
            input > 0,
            output > 0,
            !countOverflow,
            !byteOverflow,
            !headerOverflow,
            expectedBytes == data.count
        else {
            throw RecallError.invalidQueryAdapter(reason: "dimensions do not match byte length")
        }
        var decoded: [Float] = []
        decoded.reserveCapacity(valueCount)
        for index in 0..<valueCount {
            let bits = Self.readUInt32(data, offset: 16 + index * 4)
            let value = Float(bitPattern: bits)
            guard value.isFinite else {
                throw RecallError.invalidQueryAdapter(reason: "matrix contains a non-finite value")
            }
            decoded.append(value)
        }
        inputDimensions = input
        outputDimensions = output
        matrix = decoded
    }

    /// Apply the adapter and return an L2-normalized output vector.
    public func apply(to embedding: [Float]) throws -> [Float] {
        guard embedding.count == inputDimensions else {
            throw RecallError.invalidQueryAdapter(
                reason: "expected \(inputDimensions) input values, received \(embedding.count)"
            )
        }
        var inputNormSquared = 0.0
        for value in embedding {
            guard value.isFinite else {
                throw RecallError.invalidEmbedding(reason: "contains a non-finite value")
            }
            inputNormSquared += Double(value) * Double(value)
        }
        guard inputNormSquared > 0, inputNormSquared.isFinite else {
            throw RecallError.invalidEmbedding(reason: "must have a finite non-zero norm")
        }
        let inputScale = 1.0 / inputNormSquared.squareRoot()
        var output = Array(repeating: 0.0, count: outputDimensions)
        for inputIndex in 0..<inputDimensions {
            let inputValue = Double(embedding[inputIndex]) * inputScale
            let offset = inputIndex * outputDimensions
            for outputIndex in 0..<outputDimensions {
                output[outputIndex] += inputValue * Double(matrix[offset + outputIndex])
            }
        }
        let outputNormSquared = output.reduce(0.0) { $0 + $1 * $1 }
        guard outputNormSquared > 0, outputNormSquared.isFinite else {
            throw RecallError.invalidQueryAdapter(reason: "transform produced a zero/non-finite vector")
        }
        let outputScale = 1.0 / outputNormSquared.squareRoot()
        return output.map { Float($0 * outputScale) }
    }

    private static func readUInt32(_ data: Data, offset: Int) -> UInt32 {
        UInt32(data[offset])
            | UInt32(data[offset + 1]) << 8
            | UInt32(data[offset + 2]) << 16
            | UInt32(data[offset + 3]) << 24
    }
}
