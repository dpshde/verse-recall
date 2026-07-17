cmake_minimum_required(VERSION 3.22.1)

project(verse_recall_android LANGUAGES CXX)

if(NOT DEFINED RECALL_REPO_ROOT)
    message(FATAL_ERROR "RECALL_REPO_ROOT must point to the verse-recall repository")
endif()

if(NOT DEFINED RECALL_RUST_OUTPUT)
    message(FATAL_ERROR "RECALL_RUST_OUTPUT must point to generated Rust Android libraries")
endif()

set(RECALL_FFI_INCLUDE "${RECALL_REPO_ROOT}/crates/recall-ffi/include")
set(RECALL_FFI_LIBRARY "${RECALL_RUST_OUTPUT}/${ANDROID_ABI}/librecall_ffi.so")

if(NOT EXISTS "${RECALL_FFI_INCLUDE}/recall.h")
    message(FATAL_ERROR "Missing C ABI header: ${RECALL_FFI_INCLUDE}/recall.h")
endif()

if(NOT EXISTS "${RECALL_FFI_LIBRARY}")
    message(FATAL_ERROR
        "Missing ${RECALL_FFI_LIBRARY}; run bindings/kotlin/scripts/build-rust-android.sh first")
endif()

add_library(recall_ffi SHARED IMPORTED)
set_target_properties(recall_ffi PROPERTIES IMPORTED_LOCATION "${RECALL_FFI_LIBRARY}")

add_library(recall_jni SHARED recall_jni.cpp)
target_compile_features(recall_jni PRIVATE cxx_std_17)
target_compile_options(recall_jni PRIVATE -Wall -Wextra -Werror)
target_include_directories(recall_jni PRIVATE "${RECALL_FFI_INCLUDE}")
target_link_libraries(recall_jni PRIVATE recall_ffi)
