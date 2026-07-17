# Android binding verification

Verification refreshed on 2026-07-16. Generated binaries and downloaded test tools were kept in `/tmp`; no `.jar`, `.so`, or `.aar` is checked into the repository.

## Passed

| Check | Evidence |
|---|---|
| Kotlin production compilation | Kotlin 2.1.10 compiled all five production source files for JVM 17 with the Kotlin serialization compiler plugin and `-Werror`; exit 0. |
| Kotlin unit compilation | Kotlin 2.1.10 compiled the test source with the production jar as a friend path and `-Werror`; exit 0. |
| Kotlin behavior | JUnit 4.13.2: `OK (22 tests)`. Coverage includes direct-buffer ranges, strict `VRQADP01` parsing/application, overflow rejection, typed JSON decoding, legacy, LVEC-v2, and canonical segmented semantic forwarding, immutable 32-byte space-ID validation, `verse-recall-clause-v2` identity/count/order failures, engine metadata and metadata failure paths, exact C error mapping through code 10, TLS diagnostics, zero-norm rejection, idempotent close, ABI mismatch, malformed responses, and linkage failures. |
| JNI source quality | Apple Clang compiled `recall_jni.cpp` with `-std=c++17 -Wall -Wextra -Werror`; exit 0, including the array-of-clause pointer-lifetime bridge. |
| Native link contract | JNI linked against the current debug `recall-ffi` library. `nm` found all 11 JNI exports, including `nativePlanQuerySegments` and `nativeSearchWithSegmentEmbeddingsV2`, plus both corresponding C ABI dependencies. |
| End-to-end host v2 smoke | Public Kotlin `RecallEngine.open` loaded the current `bsb-full.pack`, `recall-overlay.pack`, and `bsb-semantic.pack` together through freshly relinked JNI. Lexical search returned `Psalm 23:1`; LVEC-v2 accepted the advertised 256D ID; a same-dimension wrong ID failed closed; and a two-clause segmented search completed through the public API. |
| Android API floor | The module declares minSdk 21. `VRQADP01` length validation uses local checked non-negative `Long` arithmetic rather than `Math.addExact`/`multiplyExact`, whose long overloads require API 24. Bytecode inspection found no references to those newer methods. |
| Shell/build hygiene | Both shell scripts pass `bash -n`; repository sources contain no trailing whitespace; generated scripts are executable. |
| Native host sizes | Kotlin production jar: 106,242 bytes; JNI host dylib: 149,600 bytes. These are diagnostic host figures, not Android AAR claims. |

## Android cross-build pending on toolchain availability

This machine does not have Gradle, `sdkmanager`, `adb`, an Android SDK/NDK, Android environment variables, or `cargo-ndk`. The Rust build script therefore fails early and correctly with:

```text
error: cargo-ndk is required; install it with 'cargo install cargo-ndk'
```

Consequently, no Android `.so` or release AAR size is claimed from this environment. On a provisioned Android builder, the remaining release evidence is:

```sh
cd bindings/kotlin
./gradlew :verse-recall:testDebugUnitTest
./gradlew :verse-recall:assembleRelease
./scripts/verify-aar.sh
```

The verifier requires `classes.jar` plus `librecall_ffi.so` and `librecall_jni.so` for `arm64-v8a`, `armeabi-v7a`, and `x86_64`.
