#include <jni.h>

#include <cstdint>
#include <cstring>
#include <limits>
#include <vector>

#include "recall.h"

namespace {

constexpr jsize kOpenResultSize = 2;
constexpr jsize kMaxQuerySegments = 4;

recall_engine* from_handle(jlong handle) {
    return reinterpret_cast<recall_engine*>(static_cast<intptr_t>(handle));
}

jlong to_handle(recall_engine* engine) {
    return static_cast<jlong>(reinterpret_cast<intptr_t>(engine));
}

jlongArray open_result(JNIEnv* env, recall_engine* engine, recall_err error) {
    jlongArray result = env->NewLongArray(kOpenResultSize);
    if (result == nullptr) {
        if (engine != nullptr) {
            recall_close(engine);
        }
        return nullptr;
    }
    const jlong values[kOpenResultSize] = {
        to_handle(engine),
        static_cast<jlong>(error),
    };
    env->SetLongArrayRegion(result, 0, kOpenResultSize, values);
    if (env->ExceptionCheck() == JNI_TRUE) {
        if (engine != nullptr) {
            recall_close(engine);
        }
        return nullptr;
    }
    return result;
}

bool write_error(JNIEnv* env, jintArray output, recall_err error) {
    if (output == nullptr || env->GetArrayLength(output) < 1) {
        return false;
    }
    const jint value = static_cast<jint>(error);
    env->SetIntArrayRegion(output, 0, 1, &value);
    return env->ExceptionCheck() != JNI_TRUE;
}

bool copy_utf8(JNIEnv* env, jbyteArray input, std::vector<char>* output) {
    if (input == nullptr || output == nullptr) {
        return false;
    }
    const jsize size = env->GetArrayLength(input);
    if (size < 0) {
        return false;
    }

    output->resize(static_cast<size_t>(size) + 1U);
    if (size > 0) {
        env->GetByteArrayRegion(
            input,
            0,
            size,
            reinterpret_cast<jbyte*>(output->data()));
        if (env->ExceptionCheck() == JNI_TRUE) {
            return false;
        }
        if (std::memchr(output->data(), '\0', static_cast<size_t>(size)) != nullptr) {
            return false;
        }
    }
    output->back() = '\0';
    return true;
}

bool copy_bytes(JNIEnv* env, jbyteArray input, std::vector<uint8_t>* output) {
    if (input == nullptr || output == nullptr) {
        return false;
    }
    const jsize size = env->GetArrayLength(input);
    if (size < 0) {
        return false;
    }
    output->resize(static_cast<size_t>(size));
    if (size > 0) {
        env->GetByteArrayRegion(
            input,
            0,
            size,
            reinterpret_cast<jbyte*>(output->data()));
    }
    return env->ExceptionCheck() != JNI_TRUE;
}

bool copy_floats(JNIEnv* env, jfloatArray input, std::vector<float>* output) {
    if (input == nullptr || output == nullptr) {
        return false;
    }
    const jsize size = env->GetArrayLength(input);
    if (size <= 0) {
        return false;
    }
    output->resize(static_cast<size_t>(size));
    env->GetFloatArrayRegion(input, 0, size, output->data());
    return env->ExceptionCheck() != JNI_TRUE;
}

jstring new_utf8_string(JNIEnv* env, const char* value) {
    if (value == nullptr) {
        return nullptr;
    }
    const size_t byte_count = std::strlen(value);
    if (byte_count > static_cast<size_t>(std::numeric_limits<jsize>::max())) {
        return nullptr;
    }

    jbyteArray bytes = env->NewByteArray(static_cast<jsize>(byte_count));
    if (bytes == nullptr) {
        return nullptr;
    }
    if (byte_count > 0) {
        env->SetByteArrayRegion(
            bytes,
            0,
            static_cast<jsize>(byte_count),
            reinterpret_cast<const jbyte*>(value));
        if (env->ExceptionCheck() == JNI_TRUE) {
            env->DeleteLocalRef(bytes);
            return nullptr;
        }
    }

    jclass standard_charsets = env->FindClass("java/nio/charset/StandardCharsets");
    if (standard_charsets == nullptr) {
        env->DeleteLocalRef(bytes);
        return nullptr;
    }
    jfieldID utf8_field = env->GetStaticFieldID(
        standard_charsets,
        "UTF_8",
        "Ljava/nio/charset/Charset;");
    if (utf8_field == nullptr) {
        env->DeleteLocalRef(standard_charsets);
        env->DeleteLocalRef(bytes);
        return nullptr;
    }
    jobject utf8 = env->GetStaticObjectField(standard_charsets, utf8_field);
    if (utf8 == nullptr) {
        env->DeleteLocalRef(standard_charsets);
        env->DeleteLocalRef(bytes);
        return nullptr;
    }

    jclass string_class = env->FindClass("java/lang/String");
    if (string_class == nullptr) {
        env->DeleteLocalRef(utf8);
        env->DeleteLocalRef(standard_charsets);
        env->DeleteLocalRef(bytes);
        return nullptr;
    }
    jmethodID constructor = env->GetMethodID(
        string_class,
        "<init>",
        "([BLjava/nio/charset/Charset;)V");
    if (constructor == nullptr) {
        env->DeleteLocalRef(string_class);
        env->DeleteLocalRef(utf8);
        env->DeleteLocalRef(standard_charsets);
        env->DeleteLocalRef(bytes);
        return nullptr;
    }

    auto result = static_cast<jstring>(env->NewObject(string_class, constructor, bytes, utf8));
    env->DeleteLocalRef(string_class);
    env->DeleteLocalRef(utf8);
    env->DeleteLocalRef(standard_charsets);
    env->DeleteLocalRef(bytes);
    return result;
}

jstring copy_and_free(JNIEnv* env, char* value) {
    if (value == nullptr) {
        return nullptr;
    }
    jstring result = new_utf8_string(env, value);
    recall_free_str(value);
    return result;
}

recall_opts make_options(
    jint top_k,
    const std::vector<char>& book_hint,
    bool has_book_hint) {
    recall_opts options{};
    options.top_k = top_k > 0 ? static_cast<uint32_t>(top_k) : 0U;
    options.book_hint = has_book_hint ? book_hint.data() : nullptr;
    options.flags = 0U;
    return options;
}

bool copy_query_and_hint(
    JNIEnv* env,
    jbyteArray query_utf8,
    jbyteArray book_hint_utf8,
    std::vector<char>* query,
    std::vector<char>* book_hint,
    bool* has_book_hint) {
    if (!copy_utf8(env, query_utf8, query)) {
        return false;
    }
    *has_book_hint = book_hint_utf8 != nullptr;
    return !*has_book_hint || copy_utf8(env, book_hint_utf8, book_hint);
}

bool copy_segments(
    JNIEnv* env,
    jobjectArray segment_texts_utf8,
    jobjectArray segment_embeddings,
    std::vector<std::vector<char>>* texts,
    std::vector<std::vector<float>>* embeddings) {
    if (segment_texts_utf8 == nullptr || segment_embeddings == nullptr ||
        texts == nullptr || embeddings == nullptr) {
        return false;
    }
    const jsize text_count = env->GetArrayLength(segment_texts_utf8);
    const jsize embedding_count = env->GetArrayLength(segment_embeddings);
    if (env->ExceptionCheck() == JNI_TRUE || text_count <= 0 ||
        text_count > kMaxQuerySegments || text_count != embedding_count) {
        return false;
    }

    texts->resize(static_cast<size_t>(text_count));
    embeddings->resize(static_cast<size_t>(embedding_count));
    for (jsize index = 0; index < text_count; ++index) {
        auto text = static_cast<jbyteArray>(
            env->GetObjectArrayElement(segment_texts_utf8, index));
        auto embedding = static_cast<jfloatArray>(
            env->GetObjectArrayElement(segment_embeddings, index));
        if (env->ExceptionCheck() == JNI_TRUE || text == nullptr || embedding == nullptr) {
            if (text != nullptr) {
                env->DeleteLocalRef(text);
            }
            if (embedding != nullptr) {
                env->DeleteLocalRef(embedding);
            }
            return false;
        }
        const bool copied =
            copy_utf8(env, text, &(*texts)[static_cast<size_t>(index)]) &&
            copy_floats(env, embedding, &(*embeddings)[static_cast<size_t>(index)]);
        env->DeleteLocalRef(text);
        env->DeleteLocalRef(embedding);
        if (!copied) {
            return false;
        }
    }
    return true;
}

}  // namespace

extern "C" JNIEXPORT jint JNICALL
Java_com_routebible_recall_JniNativeBridge_nativeAbiVersion(JNIEnv*, jobject) {
    return static_cast<jint>(recall_abi_version());
}

extern "C" JNIEXPORT jlongArray JNICALL
Java_com_routebible_recall_JniNativeBridge_nativeOpen(
    JNIEnv* env,
    jobject,
    jobjectArray packs) {
    try {
        if (packs == nullptr) {
            return open_result(env, nullptr, RECALL_ERR_NULL);
        }

        const jsize count = env->GetArrayLength(packs);
        if (count <= 0) {
            return open_result(env, nullptr, RECALL_ERR_NULL);
        }

        std::vector<const uint8_t*> pointers;
        std::vector<size_t> lengths;
        pointers.reserve(static_cast<size_t>(count));
        lengths.reserve(static_cast<size_t>(count));

        for (jsize index = 0; index < count; ++index) {
            jobject buffer = env->GetObjectArrayElement(packs, index);
            if (buffer == nullptr) {
                return open_result(env, nullptr, RECALL_ERR_NULL);
            }
            auto* address = static_cast<const uint8_t*>(env->GetDirectBufferAddress(buffer));
            const jlong capacity = env->GetDirectBufferCapacity(buffer);
            env->DeleteLocalRef(buffer);
            if (address == nullptr || capacity <= 0) {
                return open_result(env, nullptr, RECALL_ERR_NULL);
            }
            pointers.push_back(address);
            lengths.push_back(static_cast<size_t>(capacity));
        }

        recall_err error = RECALL_ERR_INTERNAL;
        recall_engine* engine = recall_open_v2(
            pointers.data(),
            lengths.data(),
            pointers.size(),
            &error);
        return open_result(env, engine, error);
    } catch (...) {
        return open_result(env, nullptr, RECALL_ERR_PANIC);
    }
}

extern "C" JNIEXPORT jstring JNICALL
Java_com_routebible_recall_JniNativeBridge_nativeEngineInfo(
    JNIEnv* env,
    jobject,
    jlong handle,
    jintArray out_error) {
    try {
        recall_err error = RECALL_ERR_INTERNAL;
        char* json = recall_engine_info_json(from_handle(handle), &error);
        if (!write_error(env, out_error, error)) {
            recall_free_str(json);
            return nullptr;
        }
        return copy_and_free(env, json);
    } catch (...) {
        write_error(env, out_error, RECALL_ERR_PANIC);
        return nullptr;
    }
}

extern "C" JNIEXPORT jstring JNICALL
Java_com_routebible_recall_JniNativeBridge_nativePlanQuerySegments(
    JNIEnv* env,
    jobject,
    jbyteArray query_utf8,
    jintArray out_error) {
    try {
        std::vector<char> query;
        if (!copy_utf8(env, query_utf8, &query)) {
            write_error(env, out_error, RECALL_ERR_INVALID_ARGUMENT);
            return nullptr;
        }

        recall_err error = RECALL_ERR_INTERNAL;
        char* json = recall_plan_query_segments_json(query.data(), &error);
        if (!write_error(env, out_error, error)) {
            recall_free_str(json);
            return nullptr;
        }
        return copy_and_free(env, json);
    } catch (...) {
        write_error(env, out_error, RECALL_ERR_PANIC);
        return nullptr;
    }
}

extern "C" JNIEXPORT jstring JNICALL
Java_com_routebible_recall_JniNativeBridge_nativeSearch(
    JNIEnv* env,
    jobject,
    jlong handle,
    jbyteArray query_utf8,
    jint top_k,
    jbyteArray book_hint_utf8,
    jintArray out_error) {
    try {
        std::vector<char> query;
        std::vector<char> book_hint;
        bool has_book_hint = false;
        if (!copy_query_and_hint(
                env,
                query_utf8,
                book_hint_utf8,
                &query,
                &book_hint,
                &has_book_hint)) {
            write_error(env, out_error, RECALL_ERR_INVALID_ARGUMENT);
            return nullptr;
        }

        const recall_opts options = make_options(top_k, book_hint, has_book_hint);
        recall_err error = RECALL_ERR_INTERNAL;
        char* json = recall_search_json_v2(
            from_handle(handle), query.data(), &options, &error);
        if (!write_error(env, out_error, error)) {
            recall_free_str(json);
            return nullptr;
        }
        return copy_and_free(env, json);
    } catch (...) {
        write_error(env, out_error, RECALL_ERR_PANIC);
        return nullptr;
    }
}

extern "C" JNIEXPORT jstring JNICALL
Java_com_routebible_recall_JniNativeBridge_nativeSearchWithEmbedding(
    JNIEnv* env,
    jobject,
    jlong handle,
    jbyteArray query_utf8,
    jfloatArray embedding,
    jint top_k,
    jbyteArray book_hint_utf8,
    jintArray out_error) {
    try {
        std::vector<char> query;
        std::vector<char> book_hint;
        bool has_book_hint = false;
        std::vector<float> vector;
        if (!copy_query_and_hint(
                env,
                query_utf8,
                book_hint_utf8,
                &query,
                &book_hint,
                &has_book_hint) ||
            !copy_floats(env, embedding, &vector)) {
            write_error(env, out_error, RECALL_ERR_EMBEDDING);
            return nullptr;
        }

        const recall_opts options = make_options(top_k, book_hint, has_book_hint);
        char* json = recall_search_with_embedding_json(
            from_handle(handle),
            query.data(),
            vector.data(),
            vector.size(),
            &options);
        const recall_err error = json == nullptr ? RECALL_ERR_EMBEDDING : RECALL_OK;
        if (!write_error(env, out_error, error)) {
            recall_free_str(json);
            return nullptr;
        }
        return copy_and_free(env, json);
    } catch (...) {
        write_error(env, out_error, RECALL_ERR_PANIC);
        return nullptr;
    }
}

extern "C" JNIEXPORT jstring JNICALL
Java_com_routebible_recall_JniNativeBridge_nativeSearchWithEmbeddingV2(
    JNIEnv* env,
    jobject,
    jlong handle,
    jbyteArray query_utf8,
    jbyteArray embedding_space_id,
    jfloatArray embedding,
    jint top_k,
    jbyteArray book_hint_utf8,
    jintArray out_error) {
    try {
        std::vector<char> query;
        std::vector<char> book_hint;
        bool has_book_hint = false;
        std::vector<uint8_t> space_id;
        std::vector<float> vector;
        if (!copy_query_and_hint(
                env,
                query_utf8,
                book_hint_utf8,
                &query,
                &book_hint,
                &has_book_hint) ||
            !copy_bytes(env, embedding_space_id, &space_id) ||
            !copy_floats(env, embedding, &vector)) {
            write_error(env, out_error, RECALL_ERR_EMBEDDING);
            return nullptr;
        }

        const recall_opts options = make_options(top_k, book_hint, has_book_hint);
        recall_err error = RECALL_ERR_INTERNAL;
        char* json = recall_search_with_embedding_json_v2(
            from_handle(handle),
            query.data(),
            space_id.data(),
            space_id.size(),
            vector.data(),
            vector.size(),
            &options,
            &error);
        if (!write_error(env, out_error, error)) {
            recall_free_str(json);
            return nullptr;
        }
        return copy_and_free(env, json);
    } catch (...) {
        write_error(env, out_error, RECALL_ERR_PANIC);
        return nullptr;
    }
}

extern "C" JNIEXPORT jstring JNICALL
Java_com_routebible_recall_JniNativeBridge_nativeSearchWithSegmentEmbeddingsV2(
    JNIEnv* env,
    jobject,
    jlong handle,
    jbyteArray query_utf8,
    jbyteArray embedding_space_id,
    jfloatArray whole_embedding,
    jobjectArray segment_texts_utf8,
    jobjectArray segment_embeddings,
    jint top_k,
    jbyteArray book_hint_utf8,
    jintArray out_error) {
    try {
        std::vector<char> query;
        std::vector<char> book_hint;
        bool has_book_hint = false;
        std::vector<uint8_t> space_id;
        std::vector<float> whole_vector;
        std::vector<std::vector<char>> segment_texts;
        std::vector<std::vector<float>> segment_vectors;
        if (!copy_query_and_hint(
                env,
                query_utf8,
                book_hint_utf8,
                &query,
                &book_hint,
                &has_book_hint) ||
            !copy_bytes(env, embedding_space_id, &space_id) ||
            !copy_floats(env, whole_embedding, &whole_vector) ||
            !copy_segments(
                env,
                segment_texts_utf8,
                segment_embeddings,
                &segment_texts,
                &segment_vectors)) {
            write_error(env, out_error, RECALL_ERR_EMBEDDING);
            return nullptr;
        }

        std::vector<recall_query_segment> raw_segments;
        raw_segments.reserve(segment_texts.size());
        for (size_t index = 0; index < segment_texts.size(); ++index) {
            raw_segments.push_back(recall_query_segment{
                segment_texts[index].data(),
                segment_vectors[index].data(),
                segment_vectors[index].size(),
            });
        }

        const recall_opts options = make_options(top_k, book_hint, has_book_hint);
        recall_err error = RECALL_ERR_INTERNAL;
        char* json = recall_search_with_segment_embeddings_json_v2(
            from_handle(handle),
            query.data(),
            space_id.data(),
            space_id.size(),
            whole_vector.data(),
            whole_vector.size(),
            raw_segments.data(),
            raw_segments.size(),
            &options,
            &error);
        if (!write_error(env, out_error, error)) {
            recall_free_str(json);
            return nullptr;
        }
        return copy_and_free(env, json);
    } catch (...) {
        write_error(env, out_error, RECALL_ERR_PANIC);
        return nullptr;
    }
}

extern "C" JNIEXPORT jstring JNICALL
Java_com_routebible_recall_JniNativeBridge_nativeLastErrorMessage(JNIEnv* env, jobject) {
    try {
        return copy_and_free(env, recall_last_error_message());
    } catch (...) {
        return nullptr;
    }
}

extern "C" JNIEXPORT void JNICALL
Java_com_routebible_recall_JniNativeBridge_nativeClearLastError(JNIEnv*, jobject) {
    recall_clear_last_error();
}

extern "C" JNIEXPORT void JNICALL
Java_com_routebible_recall_JniNativeBridge_nativeClose(
    JNIEnv*,
    jobject,
    jlong handle) {
    try {
        recall_close(from_handle(handle));
    } catch (...) {
        // Rust already catches panics; Closeable has no failure result to propagate here.
    }
}
