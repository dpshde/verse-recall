# @selah-tools/verse-recall

JS/TS host wrapper for the verse-recall WASM engine.

```js
import { VerseRecall, fetchPack } from "@selah-tools/verse-recall";

const packs = await Promise.all([
  fetchPack("/packs/bsb-full.pack"),
  fetchPack("/packs/bsb-expand.pack"),
  fetchPack("/packs/recall-overlay.pack"),
  fetchPack("/packs/bsb-semantic.pack"),
]);
const eng = await VerseRecall.create(packs);
try {
  const { results } = await eng.search("lost sheep", { topK: 5 });
} finally {
  eng.dispose();
}
```

With the optional `LVEC` semantic pack loaded, compute the query vector in the
host using the exact content-addressed encoder contract and call:

```js
const metadata = await eng.denseMetadata();
const { results } = await eng.searchWithEmbeddingInSpace(
  "everything started because God brought it into being",
  metadata.embeddingSpaceId,
  queryEmbedding,
  { topK: 5 },
);
```

The 64-character ID fingerprints the ONNX graph and external weights,
tokenizer, pooling, prefixes, projection, maximum length, and normalization.
The ID-less `searchWithEmbedding()` call remains available only for legacy
LVEC v1 artifacts and is not the release integration path.

`queryEmbedding` must be a `Float32Array`. The core validates its finite values,
exact dimension, and nonzero norm before searching.

Most packs, including the release Arctic-M pack, use the encoder output
directly. An experimental or domain-aligned contract may additionally name a
content-addressed `VRQADP01` query-only transform. Apply it in the host before
search; never apply it to corpus vectors:

```js
import { applyQueryAdapter, parseQueryAdapter } from "@selah-tools/verse-recall";

const adapter = parseQueryAdapter(await fetchPack("/models/domain.query-adapter"));
const queryEmbedding = applyQueryAdapter(rawEncoderEmbedding, adapter);
```

Parsing rejects bad magic, dimensions, lengths, and non-finite matrix values.
Application validates the input dimension and L2-normalizes the result. The
adapter SHA-256 is part of the LVEC embedding-space ID, so using the wrong
transform fails closed even when its dimensions happen to match.

Compound memories use the same query-only planner in Rust, Web, and native
hosts. Planner identity `verse-recall-clause-v2` preserves clause source order;
stale plans from another planner version are rejected. Planning does not open
a pack or inspect the corpus:

```js
import { planQuerySegments } from "@selah-tools/verse-recall";

const query = "lost sheep and another remembered scene about a prodigal son";
const plan = await planQuerySegments(query);
const segmentVectors = await Promise.all(
  plan.segments.map((text) => embedWithMatchingEncoder(text)),
);
const { results } = await eng.searchWithSegmentEmbeddingsInSpace(
  query,
  metadata.embeddingSpaceId,
  queryEmbedding,
  plan.segments.map((text, index) => ({
    text,
    embedding: segmentVectors[index],
  })),
  { topK: 10 },
);
```

The planner emits zero segments when a split would create short fragments or
cannot be represented safely in the four-segment runtime bound. In that case,
use the ordinary whole-query semantic call.

The WASM module loads lazily on the first `init()` or search. `create()` is the
one-call convenience API; constructing first and calling `init()` later remains
supported. Concurrent initialization calls share one attempt, and `dispose()`
releases the wasm-bindgen engine deterministically.

Build and test locally:

```bash
cd bindings/js
npm run build:wasm   # requires wasm-pack on PATH
npm test
npm run test:wasm    # set VERSE_RECALL_PACK to exercise the real module
```

The deterministic build script generates `pkg/recall_wasm.js`, TypeScript
declarations, and `pkg/recall_wasm_bg.wasm`, then records their SHA-256 hashes
in `pkg/build-manifest.json`. `npm pack` rebuilds and tests those artifacts.
The build removes wasm-pack's generated `pkg/.gitignore`; otherwise npm would
silently exclude the complete WASM payload from the publish tarball.
The generated low-level bindings are also exported as
`@selah-tools/verse-recall/raw` for specialized hosts.

`VerseRecallInitError`, `VerseRecallSearchError`, and `VerseRecallFetchError`
carry stable `code` fields for programmatic handling. Pack buffers are copied
by the Rust engine during initialization and may be released by the host after
`init()` resolves.

Reference parsing stays in [grab-bcv](https://www.npmjs.com/package/grab-bcv).
