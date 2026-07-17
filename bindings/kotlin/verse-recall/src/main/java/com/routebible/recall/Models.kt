package com.routebible.recall

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder

/** Search controls shared by lexical and semantic queries. */
public data class SearchOptions(
    public val topK: Int = DEFAULT_TOP_K,
    public val bookHint: String? = null,
) {
    public companion object {
        public const val DEFAULT_TOP_K: Int = 10
        public const val MAX_TOP_K: Int = 100
    }
}

/** Stable confidence category emitted by the engine. */
@Serializable
public enum class Confidence {
    @SerialName("recovered")
    RECOVERED,

    @SerialName("related")
    RELATED,

    @SerialName("often-misremembered")
    OFTEN_MISREMEMBERED,
}

/** Per-lane evidence scores. Scores are response-local and not thresholds. */
@Serializable
public data class LaneScores(
    public val a: Float = 0f,
    public val b: Float = 0f,
    public val c: Float = 0f,
    public val d: Float = 0f,
)

/** Canonical range for a structured story, event, or concept result. */
@Serializable
public data class PassageRange(
    @SerialName("start_ref")
    public val startReference: String,
    @SerialName("end_ref")
    public val endReference: String,
)

/** One ranked verse or structured passage result. */
@Serializable
public data class SearchResult(
    @SerialName("ref")
    public val reference: String,
    public val text: String,
    /** Opaque ordering score; do not persist or compare across engine versions. */
    public val score: Float,
    public val confidence: Confidence,
    public val lanes: LaneScores,
    @SerialName("matched_terms")
    public val matchedTerms: List<String> = emptyList(),
    @SerialName("expanded_terms")
    public val expandedTerms: List<String> = emptyList(),
    public val passages: List<PassageRange> = emptyList(),
    public val chips: List<String> = emptyList(),
)

/** Complete response from one search invocation. */
@Serializable
public data class SearchResponse(
    public val query: String,
    public val results: List<SearchResult>,
)

/** Corpus-independent canonical clause plan for one query. */
@Serializable
public data class QuerySegmentPlan(
    public val planner: String,
    public val segments: List<String>,
) {
    public companion object {
        public const val PLANNER_ID: String = "verse-recall-clause-v2"
        public const val MAX_SEGMENTS: Int = 4
    }
}

/** A canonical clause paired with a host embedding for that exact text. */
public class QuerySegmentEmbedding(
    public val text: String,
    embedding: FloatArray,
) {
    private val storage: FloatArray = embedding.copyOf()

    /** Defensive copy of the vector supplied at construction. */
    public val embedding: FloatArray
        get() = storage.copyOf()

    internal fun nativeEmbedding(): FloatArray = storage.copyOf()

    override fun equals(other: Any?): Boolean =
        other is QuerySegmentEmbedding && text == other.text && storage.contentEquals(other.storage)

    override fun hashCode(): Int = 31 * text.hashCode() + storage.contentHashCode()

    override fun toString(): String = "QuerySegmentEmbedding(text=$text, dimensions=${storage.size})"
}

/**
 * Opaque identity of the complete semantic encoder contract used by an LVEC v2/v3 pack.
 *
 * The ID is exactly 32 bytes and must not be all zero. It includes the model, tokenizer, pooling,
 * normalization, projection, and query-prefix contract; it is not merely a model name.
 */
@Serializable(with = EmbeddingSpaceIdSerializer::class)
public class EmbeddingSpaceId(bytes: ByteArray) {
    private val storage: ByteArray = bytes.copyOf()

    init {
        if (storage.size != BYTE_COUNT) {
            throw RecallInvalidArgumentException(
                "embeddingSpaceId must contain exactly $BYTE_COUNT bytes; received ${storage.size}",
            )
        }
        if (storage.all { it == 0.toByte() }) {
            throw RecallInvalidArgumentException("embeddingSpaceId must not be all zero")
        }
    }

    public constructor(hex: String) : this(decodeHex(hex))

    /** Defensive copy suitable for persistence or transport. */
    public val bytes: ByteArray
        get() = storage.copyOf()

    /** Canonical lowercase hexadecimal representation used by engine metadata. */
    public val hex: String
        get() = buildString(BYTE_COUNT * 2) {
            storage.forEach { byte ->
                val value = byte.toInt() and 0xff
                append(HEX_DIGITS[value ushr 4])
                append(HEX_DIGITS[value and 0x0f])
            }
        }

    internal fun nativeBytes(): ByteArray = storage.copyOf()

    override fun equals(other: Any?): Boolean =
        other is EmbeddingSpaceId && storage.contentEquals(other.storage)

    override fun hashCode(): Int = storage.contentHashCode()

    override fun toString(): String = hex

    public companion object {
        public const val BYTE_COUNT: Int = 32
        private const val HEX_DIGITS: String = "0123456789abcdef"

        private fun decodeHex(hex: String): ByteArray {
            if (hex.length != BYTE_COUNT * 2) {
                throw RecallInvalidArgumentException(
                    "embeddingSpaceId must contain exactly ${BYTE_COUNT * 2} hexadecimal characters",
                )
            }
            return ByteArray(BYTE_COUNT) { index ->
                val offset = index * 2
                hex.substring(offset, offset + 2).toIntOrNull(16)?.toByte()
                    ?: throw RecallInvalidArgumentException("embeddingSpaceId is not hexadecimal")
            }
        }
    }
}

/** String serializer matching the lowercase hexadecimal form in native engine metadata. */
public object EmbeddingSpaceIdSerializer : KSerializer<EmbeddingSpaceId> {
    override val descriptor: SerialDescriptor =
        PrimitiveSerialDescriptor("EmbeddingSpaceId", PrimitiveKind.STRING)

    override fun serialize(
        encoder: Encoder,
        value: EmbeddingSpaceId,
    ) {
        encoder.encodeString(value.hex)
    }

    override fun deserialize(decoder: Decoder): EmbeddingSpaceId =
        try {
            EmbeddingSpaceId(decoder.decodeString())
        } catch (error: RecallInvalidArgumentException) {
            throw SerializationException("Invalid embedding-space ID", error)
        }
}

/** Optional Lane C metadata advertised by the opened engine. */
@Serializable
public data class DenseEngineInfo(
    public val version: Int,
    public val dims: Int,
    @SerialName("table_count")
    public val tableCount: Int,
    /** Aligned representation roles in table order. */
    @SerialName("table_roles")
    public val tableRoles: List<String> = emptyList(),
    /** Present for LVEC v2/v3 and absent for legacy LVEC v1. */
    @SerialName("embedding_space_id")
    public val embeddingSpaceId: EmbeddingSpaceId? = null,
)

/** Immutable runtime capabilities for one opened collection of packs. */
@Serializable
public data class EngineInfo(
    @SerialName("abi_version")
    public val abiVersion: Int,
    @SerialName("translation_id")
    public val translationId: String,
    @SerialName("verse_count")
    public val verseCount: Int,
    /** Lowercase lane identifiers (`a` through `d`) available at runtime. */
    public val lanes: List<String>,
    public val dense: DenseEngineInfo? = null,
)
