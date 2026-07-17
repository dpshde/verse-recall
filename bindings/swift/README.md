# VerseRecall for Swift

An idiomatic SwiftPM wrapper around the portable `recall-ffi` C ABI. It runs
the same deterministic packs used by Rust and Web and supports lexical,
translation/concept, and host-embedded semantic searches on iOS and macOS.

## API

```swift
import VerseRecall

let engine = try RecallEngine(packURLs: [
    Bundle.main.url(forResource: "bsb-full", withExtension: "pack")!,
    Bundle.main.url(forResource: "bsb-expand", withExtension: "pack")!,
    Bundle.main.url(forResource: "recall-overlay", withExtension: "pack")!,
    Bundle.main.url(forResource: "bsb-semantic", withExtension: "pack")!,
])
defer { engine.close() }

let lexical = try engine.search(
    "the fall",
    options: SearchOptions(topK: 5, bookHint: "Genesis")
)

let info = try engine.info()
guard let spaceID = info.dense?.embeddingSpaceID else {
    fatalError("This installation has no LVEC v2 pack")
}
let semantic = try engine.search(
    "God brought everything into being",
    embedding: queryVector,
    embeddingSpaceID: spaceID,
    options: SearchOptions(topK: 5)
)
```

Compound memories can be split with the same corpus-independent planner used
by Rust, Web, and Android. Planner identity `verse-recall-clause-v2` preserves
clause source order, and stale plans from another version fail closed. Embed
the whole query and every returned clause with the identical encoder contract,
then submit all vectors together:

```swift
let query = "lost sheep and prodigal son"
let provider = ArcticProvider(embeddingSpaceID: spaceID)
let plan = try RecallEngine.planQuerySegments(query)
guard !plan.segments.isEmpty else {
    // The conservative planner found no safe split; use ordinary semantic search.
    fatalError("query is not safely segmentable")
}

var clauses: [QuerySegmentEmbedding] = []
for text in plan.segments {
    clauses.append(
        QuerySegmentEmbedding(text: text, embedding: try await provider.embedding(for: text))
    )
}
let response = try engine.search(
    query,
    embedding: try await provider.embedding(for: query),
    segments: clauses,
    embeddingSpaceID: provider.embeddingSpaceID,
    options: SearchOptions(topK: 10)
)
```

The planner emits zero or two to four clauses; zero means the query should stay
whole. Segmented search reruns the native planner and accepts only its exact
ordered clause texts. Empty,
reordered, extra, non-finite, or zero-norm clause vectors fail before retrieval,
and the core verifies the plan and embedding-space ID again.

`RecallEngine` is thread-safe. Searches and `close()` are serialized so a
native handle cannot be freed during a query. `close()` is idempotent, and
`deinit` provides a final safety net.

An application-owned encoder can conform to `RecallEmbeddingProvider`:

```swift
struct ArcticProvider: RecallEmbeddingProvider {
    let embeddingSpaceID: EmbeddingSpaceID

    func embedding(for query: String) async throws -> [Float] {
        // Core ML or ONNX Runtime inference using the LVEC pack's exact model,
        // pooling, normalization, and query-prefix contract.
    }
}

let result = try await engine.search(
    "meaning-only query",
    using: ArcticProvider(embeddingSpaceID: spaceID)
)
```

The release Arctic-M contract uses the encoder output directly. If a future
asymmetric LVEC contract explicitly includes a `VRQADP01` transform, the SDK
can parse and apply its row-major Float32 matrix before search:

```swift
let adapter = try QueryAdapter(data: adapterData)
let adaptedEmbedding = try adapter.apply(to: rawEncoderEmbedding)
```

`QueryAdapter` rejects malformed dimensions and non-finite values and returns
an L2-normalized vector. The exact adapter hash belongs to the embedding-space
ID; it must never be treated as an unversioned tuning file.

The SDK validates nonempty finite embeddings with a nonzero norm and a 32-byte
nonzero `EmbeddingSpaceID`. The native core validates the exact dimensions and
fails closed unless the ID matches the LVEC v2 pack. The overload without an
ID remains available solely for legacy LVEC v1 packs, which cannot prove model
compatibility.

`info()` reports the translation, verse count, active lanes, dense dimensions,
table count, LVEC version, and required embedding-space ID. Native v2 calls
return explicit `RecallError.CErrorCode` values and retain a thread-local error
message, which the Swift wrapper captures in `RecallError.nativeFailure`.

## SwiftPM integration

Run the assembly script from the repository root:

```bash
bindings/swift/scripts/build-xcframework.sh
```

It builds the compact `recall-ffi` dynamic library for every installed Apple
Rust target, rewrites its install name to a relocatable `@rpath` value, and
creates `Artifacts/CVerseRecall.xcframework`. Once present, `Package.swift`
automatically uses that binary target. Without it, the package uses a system
library target named `CVerseRecall`, which is useful for local development when
`librecall_ffi` is available on the linker's library path.

For a complete universal Apple artifact, install these Rust targets first:

```bash
rustup target add \
  aarch64-apple-darwin x86_64-apple-darwin \
  aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
```

The dynamic distribution avoids shipping Rust's large intermediate static
archives; the current arm64 macOS + iOS-device artifact is about 1.97 MiB raw
and 853 KiB zipped.
The assembly is clean and ordered, combines matching platform architectures
with `lipo`, canonicalizes Xcode's nondeterministic slice ordering, and writes
SHA-256 evidence to
`Artifacts/CVerseRecall.build-manifest.txt`. Pack files stay outside the SDK so
applications can choose Lite, Recall, or Semantic payloads independently.
Both build scripts fail immediately if the packaged C header has drifted from
the canonical `crates/recall-ffi/include/recall.h` ABI.

## Development verification

```bash
bindings/swift/scripts/test-local.sh
```

The script builds the debug native library, configures its link and runtime
paths, runs an actual query against `data/bsb-full.pack`, and executes the
Swift test suite. Additional tests inject a mock backend to cover ABI
checks, pack validation, JSON models, option/embedding validation, lexical and
semantic/segmented routing, canonical planner validation, provider integration,
deterministic closure, and malformed responses without depending on a
particular corpus pack.

The public SDK requires ABI version `1`; the hardened calls are additive and do
not break the original ABI symbols. Native open/search/semantic/info codes,
panic containment failures, and all host validation failures are exposed as
typed `RecallError` values.
