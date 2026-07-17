package com.routebible.recall

import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.sqrt

/**
 * Portable query-only transform for asymmetric semantic packs.
 *
 * `VRQADP01` stores a row-major Float32 matrix. Apply it after the host encoder
 * and before semantic search. The dense pack's content-addressed embedding
 * space ID binds the exact adapter bytes to the corpus vectors.
 */
public class QueryAdapter private constructor(
    public val inputDimensions: Int,
    public val outputDimensions: Int,
    private val matrix: FloatArray,
) {
    /** Parse an adapter from the buffer's current `position..limit` range. */
    public constructor(source: ByteBuffer) : this(parse(source))

    /** Parse an adapter from an owned copy of [bytes]. */
    public constructor(bytes: ByteArray) : this(ByteBuffer.wrap(bytes))

    private constructor(parsed: ParsedAdapter) : this(
        inputDimensions = parsed.inputDimensions,
        outputDimensions = parsed.outputDimensions,
        matrix = parsed.matrix,
    )

    /** Apply the matrix and return an L2-normalized output embedding. */
    public fun apply(embedding: FloatArray): FloatArray {
        if (embedding.size != inputDimensions) {
            throw RecallQueryAdapterException(
                "expected $inputDimensions input values, received ${embedding.size}",
            )
        }

        var inputNormSquared = 0.0
        for (value in embedding) {
            if (!value.isFinite()) {
                throw RecallInvalidArgumentException("embedding must contain only finite values")
            }
            inputNormSquared += value.toDouble() * value.toDouble()
        }
        if (!(inputNormSquared > 0.0) || !inputNormSquared.isFinite()) {
            throw RecallInvalidArgumentException("embedding must have a finite non-zero norm")
        }

        val inputScale = 1.0 / sqrt(inputNormSquared)
        val output = DoubleArray(outputDimensions)
        for (inputIndex in 0 until inputDimensions) {
            val inputValue = embedding[inputIndex].toDouble() * inputScale
            val rowOffset = inputIndex * outputDimensions
            for (outputIndex in 0 until outputDimensions) {
                output[outputIndex] +=
                    inputValue * matrix[rowOffset + outputIndex].toDouble()
            }
        }

        var outputNormSquared = 0.0
        for (value in output) outputNormSquared += value * value
        if (!(outputNormSquared > 0.0) || !outputNormSquared.isFinite()) {
            throw RecallQueryAdapterException("transform produced a zero or non-finite vector")
        }
        val outputScale = 1.0 / sqrt(outputNormSquared)
        return FloatArray(outputDimensions) { index ->
            (output[index] * outputScale).toFloat()
        }
    }

    private data class ParsedAdapter(
        val inputDimensions: Int,
        val outputDimensions: Int,
        val matrix: FloatArray,
    )

    private companion object {
        private val MAGIC: ByteArray = "VRQADP01".encodeToByteArray()
        private const val HEADER_BYTES: Int = 16
        private const val FLOAT_BYTES: Long = 4L

        private fun parse(source: ByteBuffer): ParsedAdapter {
            val buffer = source.slice().order(ByteOrder.LITTLE_ENDIAN)
            if (buffer.remaining() < HEADER_BYTES) {
                throw RecallQueryAdapterException("missing VRQADP01 header")
            }

            val magic = ByteArray(MAGIC.size)
            buffer.get(magic)
            if (!magic.contentEquals(MAGIC)) {
                throw RecallQueryAdapterException("missing VRQADP01 header")
            }

            val inputLong = buffer.int.toLong() and 0xffff_ffffL
            val outputLong = buffer.int.toLong() and 0xffff_ffffL
            val valueCount = checkedMultiply(inputLong, outputLong)
            val expectedBytes = checkedAdd(
                HEADER_BYTES.toLong(),
                checkedMultiply(valueCount, FLOAT_BYTES),
            )
            if (
                inputLong == 0L ||
                outputLong == 0L ||
                inputLong > Int.MAX_VALUE ||
                outputLong > Int.MAX_VALUE ||
                valueCount > Int.MAX_VALUE ||
                expectedBytes != source.remaining().toLong()
            ) {
                throw RecallQueryAdapterException("dimensions do not match byte length")
            }

            val matrix = FloatArray(valueCount.toInt()) { index ->
                val value = buffer.float
                if (!value.isFinite()) {
                    throw RecallQueryAdapterException(
                        "matrix contains a non-finite value at index $index",
                    )
                }
                value
            }
            return ParsedAdapter(inputLong.toInt(), outputLong.toInt(), matrix)
        }

        /** API-21-safe checked arithmetic for the non-negative framing dimensions. */
        private fun checkedMultiply(
            left: Long,
            right: Long,
        ): Long {
            if (left != 0L && right > Long.MAX_VALUE / left) {
                throw RecallQueryAdapterException("adapter dimensions overflow")
            }
            return left * right
        }

        /** API-21-safe checked arithmetic for the non-negative framing lengths. */
        private fun checkedAdd(
            left: Long,
            right: Long,
        ): Long {
            if (right > Long.MAX_VALUE - left) {
                throw RecallQueryAdapterException("adapter dimensions overflow")
            }
            return left + right
        }
    }
}
