# Swift SDK verification

Verified on 2026-07-16 with Swift 6.3.3 and Xcode 26.6.

| Resolution | Evidence |
|---|---|
| SwiftPM package and native C integration | `swift test -Xswiftc -warnings-as-errors` passed 22/22 tests with `VERSE_RECALL_TEST_PACK=data/bsb-full.pack`, from both cold and warm caches. The integration test opened the real normalizer-v2 pack through the current hardened dylib and recovered Psalm 23:1. Source-library tests use a dedicated scratch directory and prioritize `target/debug` in the dynamic-library path, so an older assembled XCFramework cannot shadow the just-built FFI library. The stale dylib was deliberately left present during this proof. |
| Binary-target distribution | A fresh two-slice XCFramework was assembled in `/private/tmp`; both macOS arm64 and iOS-device arm64 libraries export `recall_plan_query_segments_json` and `recall_search_with_segment_embeddings_json_v2`, and both packaged headers exactly match the canonical C header SHA-256. |
| Codable API contract | Tests decoded ordinary hits, absent optional passages, structured Genesis 3:1–24 passages, confidence values, and snake-case JSON fields. |
| Lifecycle and thread-safe ownership | Tests proved `close()` is idempotent, waits for an in-flight native search, makes post-close searches typed failures, and that `deinit` releases an unclosed native handle exactly once. The implementation holds one lock across each native call. |
| Typed safety failures | Tests cover no/empty/corrupt packs, missing pack URLs, ABI mismatch, empty/NUL queries, invalid `topK`, empty/NUL book hints, empty/zero/non-finite embeddings, malformed space IDs, closed engines, malformed native JSON, and propagation of C error codes plus thread-local diagnostics. |
| Semantic host API | Tests cover legacy LVEC v1, LVEC v2 with a required 32-byte `EmbeddingSpaceID`, hex/JSON identity round trips, the async `RecallEmbeddingProvider` route, strict portable `VRQADP01` query transforms, canonical `verse-recall-clause-v2` planning, and fail-closed segmented forwarding of whole/clause vectors plus the space ID. The planner and segmented C symbols are present in both freshly assembled Apple slices. |
| Engine capabilities | `info()` decodes live translation, verse count, lane availability, dense version/dimensions/table count, and v2 space ID. Tests cover `dense: null` and complete v2 metadata. |
| Hardened C boundary | `cargo test -p recall-ffi` passed 13/13 tests covering legacy compatibility, segmented search, bounded `topK`, null/open/UTF-8/option failures, dimensions, NaN/zero norm, space mismatch, TLS isolation, and panic containment. Strict Clippy passed. |
| C consumer | Hardened smoke and `examples/c/main.c` compiled as C11 with `-Wall -Wextra -Werror`; the linked example reported ABI 1 and exited successfully. |
| Strict Swift concurrency | The complete 22-test target passed with warnings as errors; the SDK's locking and `Sendable` boundaries remain covered by the concurrency tests. |
| Release compilation | `swift build -c release` passed against the binary target. |
| iOS portability | SwiftPM cross-build for `arm64-apple-ios13.0` against the iPhoneOS 26.5 SDK passed, including the generated iOS dylib slice. |
| Relocatable binary | `otool -D` reports `@rpath/librecall_ffi.dylib` for macOS and iOS, rather than a Cargo build-directory path. |
| No third-party native runtime | `otool -L` reports only the dylib itself and Apple's `/usr/lib/libSystem.B.dylib` for both slices. |
| Compact distribution | The current arm64 macOS + iOS-device XCFramework is 2,020 KiB (873,168-byte zip): 1,016,224-byte macOS dylib and 1,008,200-byte iOS dylib. Intermediate static archives are not packaged. |
| Reproducible assembly evidence | The builder now canonicalizes Xcode's nondeterministic slice ordering. Two consecutive unchanged builds produced identical `CVerseRecall.build-manifest.txt` files, including every artifact SHA-256 and toolchain version. |
| ABI drift protection | Both build scripts compare the packaged `recall.h` byte-for-byte with the canonical FFI header and fail before compilation if they differ. |

The installed Rust targets provided macOS arm64 and iOS-device arm64 slices.
The assembly script also supports macOS x86_64 and iOS simulator arm64/x86_64,
combining same-platform architectures with `lipo`; those toolchain targets were
not installed on this verification host, so those slices were not fabricated
or claimed as tested.
