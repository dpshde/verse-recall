package com.routebible.recall

/** Root of all expected Verse Recall Android failures. */
public sealed class RecallException(
    message: String,
    cause: Throwable? = null,
) : RuntimeException(message, cause)

public class RecallInvalidArgumentException(
    message: String,
) : RecallException(message)

/** Invalid or incompatible portable `VRQADP01` query transform. */
public class RecallQueryAdapterException(
    message: String,
) : RecallException(message)

public class RecallNativeLibraryException(
    message: String,
    cause: Throwable? = null,
) : RecallException(message, cause)

public class RecallAbiMismatchException(
    public val expected: Int,
    public val actual: Int,
) : RecallException("Unsupported Verse Recall native ABI $actual; expected $expected")

/** Stable `recall_err` value returned by the C ABI. */
public enum class RecallNativeError(public val code: Int) {
    OK(0),
    NULL_ARGUMENT(1),
    OPEN_FAILED(2),
    INVALID_UTF8(3),
    INTERNAL(4),
    INVALID_ARGUMENT(5),
    SEARCH_FAILED(6),
    INVALID_EMBEDDING(7),
    EMBEDDING_SPACE_MISMATCH(8),
    PANIC_CONTAINED(9),
    JSON_FAILED(10),
    UNKNOWN(-1),
    ;

    public companion object {
        internal fun fromCode(code: Int): RecallNativeError =
            entries.firstOrNull { it.code == code } ?: UNKNOWN
    }
}

public class RecallOpenException(
    public val nativeError: RecallNativeError,
    public val nativeErrorCode: Int,
    public val diagnostic: String? = null,
) : RecallException(
    buildString {
        append("Could not open Verse Recall packs: ${nativeError.name} (native code $nativeErrorCode)")
        diagnostic?.let { append(": $it") }
    },
)

public class RecallSearchException(
    message: String,
    cause: Throwable? = null,
    public val nativeError: RecallNativeError? = null,
    public val nativeErrorCode: Int? = null,
    public val diagnostic: String? = null,
) : RecallException(message, cause)

public class RecallEngineInfoException(
    public val nativeError: RecallNativeError,
    public val nativeErrorCode: Int,
    public val diagnostic: String? = null,
) : RecallException(
    buildString {
        append("Could not read Verse Recall engine metadata: ${nativeError.name} (native code $nativeErrorCode)")
        diagnostic?.let { append(": $it") }
    },
)

public class RecallDecodingException(
    message: String,
    cause: Throwable,
) : RecallException(message, cause)

public class RecallClosedException : RecallException("RecallEngine is closed")
