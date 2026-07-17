# Verse Recall for Android

Android AAR bindings for the portable `verse-recall` engine. The package contains:

- an idiomatic, thread-safe Kotlin `RecallEngine` that implements `Closeable`;
- typed response/capability models and typed lifecycle/open/search/decoding errors;
- JNI glue over the hardened additive v2 `recall.h` C ABI, including thread-local diagnostics;
- `arm64-v8a`, `armeabi-v7a`, and `x86_64` Rust build integration through `cargo-ndk`;
- an injectable native boundary, so all Kotlin behavior is unit-testable without loading an `.so`.

Bible packs and an optional query-embedding model are deliberately not bundled in the AAR. Applications decide which licensed packs to ship or download. The native engine copies pack bytes while opening, so callers can release their input buffers once `RecallEngine.open` returns.

## Prerequisites

- JDK 17
- Android SDK 35
- Android NDK `27.2.12479018`
- CMake 3.22.1
- Rust plus Android targets
- [`cargo-ndk`](https://github.com/bbqsrc/cargo-ndk)
- Gradle 8.9 or a compatible wrapper supplied by the consuming checkout

Install the Rust-side prerequisites once:

```sh
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
cargo install cargo-ndk
```

The project intentionally does not check in the binary `gradle-wrapper.jar`. From this directory, either use an installed Gradle or generate the wrapper once:

```sh
gradle wrapper --gradle-version 8.9
```

## Build and test

The AAR build invokes `scripts/build-rust-android.sh` before CMake, producing Rust libraries only in the ignored `verse-recall/src/main/jniLibs/` directory.

```sh
./gradlew :verse-recall:testDebugUnitTest
./gradlew :verse-recall:assembleRelease
```

To build the Rust libraries independently:

```sh
./scripts/build-rust-android.sh
```

The release AAR is written to `verse-recall/build/outputs/aar/verse-recall-release.aar`. Inspect it with:

```sh
unzip -l verse-recall/build/outputs/aar/verse-recall-release.aar
./scripts/verify-aar.sh
```

It should contain both `librecall_jni.so` and `librecall_ffi.so` for every configured ABI.

## Usage

```kotlin
import com.routebible.recall.RecallEngine
import com.routebible.recall.SearchOptions

val canonicalPack: ByteArray = assets.open("bsb-full.pack").use { it.readBytes() }
val expansionPack: ByteArray = assets.open("bsb-expand.pack").use { it.readBytes() }
val overlayPack: ByteArray = assets.open("recall-overlay.pack").use { it.readBytes() }
val semanticPack: ByteArray = assets.open("bsb-semantic.pack").use { it.readBytes() }

RecallEngine.open(
    listOf(canonicalPack, expansionPack, overlayPack, semanticPack),
).use { engine ->
    val response = engine.search(
        query = "the fall",
        options = SearchOptions(topK = 5),
    )
    response.results.forEach { result ->
        println("${result.reference}: ${result.text}")
    }
}
```

For already-mapped or direct buffers, use `RecallEngine.openBuffers`. Only each buffer's current `position..limit` range is read; the caller's position is not changed.

Semantic search remains host-owned:

```kotlin
val embedding: FloatArray = embeddingProvider.embed("strength in weakness")
val dense = requireNotNull(engine.info().dense)
val embeddingSpaceId = requireNotNull(dense.embeddingSpaceId)

val response = engine.searchWithEmbedding(
    query = "strength in weakness",
    embedding = embedding,
    embeddingSpaceId = embeddingSpaceId,
    options = SearchOptions(topK = 10),
)
```

For compound memories, plan clauses without opening a corpus, embed the whole
query and every canonical clause with that same encoder, and submit them as one
bounded request:

```kotlin
val query = "lost sheep and prodigal son"
val plan = RecallEngine.planQuerySegments(query)
require(plan.segments.isNotEmpty()) // otherwise use ordinary semantic search

val response = engine.searchWithSegmentEmbeddings(
    query = query,
    wholeEmbedding = embeddingProvider.embed(query),
    segments = plan.segments.map { text ->
        QuerySegmentEmbedding(text, embeddingProvider.embed(text))
    },
    embeddingSpaceId = embeddingSpaceId,
    options = SearchOptions(topK = 10),
)
```

The conservative planner returns zero or two to four clauses; zero means no
safe split was found. Planner identity `verse-recall-clause-v2` preserves source
order. Segmented search recomputes the plan before JNI and requires the exact
ordered texts. The JNI and Rust boundaries validate the one-to-four bound,
every vector, and the embedding-space ID again, so stale or arbitrary clause
payloads fail closed.

The release Arctic-M pack consumes the encoder output directly. For a future
asymmetric pack whose contract explicitly names a `VRQADP01` query transform,
apply the portable matrix after encoder inference and before native search:

```kotlin
val adapter = QueryAdapter(adapterBytes)
val adaptedEmbedding = adapter.apply(rawEncoderEmbedding)
```

`QueryAdapter` also accepts the current `position..limit` range of a
`ByteBuffer` without mutating the caller's position. It rejects bad framing,
overflowing or mismatched dimensions, non-finite values, zero-norm input, and
zero/non-finite output. The exact adapter bytes are bound into the LVEC
embedding-space ID.

`EmbeddingSpaceId` is an immutable, validated 32-byte encoder-contract fingerprint. It covers the model, tokenizer, pooling, normalization, optional projection, and query prefix. LVEC v2 search fails closed with `EMBEDDING_SPACE_MISMATCH` when the supplied fingerprint differs, even if vector dimensions match. `engine.info()` also exposes LVEC version, dimensions, table count, and the required fingerprint, so hosts can choose the correct encoder before computing a query vector.

The overload without `embeddingSpaceId` remains source-compatible for legacy LVEC v1 packs. LVEC v2 packs deliberately reject that unverifiable route; new integrations should always supply the ID.

## Lifecycle and concurrency

`RecallEngine` serializes native operations. A search cannot race `close`, repeated `close` calls are harmless, and every operation after close throws `RecallClosedException`. Prefer Kotlin's `use` block or close the engine from the owning Android lifecycle component.

The AAR has no Android `Context` dependency. Packs may come from assets, files, memory maps, network caches, or dependency injection.

## Errors

All public failures derive from `RecallException`:

- `RecallInvalidArgumentException`
- `RecallQueryAdapterException`
- `RecallNativeLibraryException`
- `RecallAbiMismatchException`
- `RecallOpenException`
- `RecallSearchException`
- `RecallEngineInfoException`
- `RecallDecodingException`
- `RecallClosedException`

`RecallOpenException`, `RecallSearchException`, and `RecallEngineInfoException` preserve the exact stable C error code plus the native thread-local diagnostic. `RecallNativeError` covers all current codes: null/open/UTF-8/internal, invalid argument, search, invalid embedding, embedding-space mismatch, contained panic, and JSON serialization failure. Diagnostics are captured immediately on the calling thread before another native operation can replace them.

## Shrinker support

`consumer-rules.pro` is embedded in the AAR and preserves the private JNI entry points. Public models use Kotlin serialization rather than reflection, so consumers do not need blanket keep rules.

## Publishing

The module applies `maven-publish` and exposes the release component as:

```text
com.routebible:verse-recall-android:0.1.0
```

For local Maven testing:

```sh
./gradlew :verse-recall:publishToMavenLocal
```

Before publishing, run tests, inspect all ABI entries in the AAR, and run at least one instrumented open/search/close smoke test on both an arm64 device and an x86_64 emulator.
