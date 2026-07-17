# verse-recall

Portable, offline tip-of-the-tongue scripture retrieval. A fragment, a
remembered KJV line, a named event, or a low-overlap paraphrase returns ranked
canonical BSB verses and structured passage ranges.

The runtime is deterministic CPU over caller-provided, read-only pack buffers.
It performs no file or network I/O and bundles no model runtime. Rust, C,
Swift, Kotlin/Android, and Web/WASM use the same engine and pack formats; the
host owns semantic query encoding.

| Start here | Doc |
|---|---|
| Product status | [`STATUS.md`](./STATUS.md) |
| Contribute | [`CONTRIBUTING.md`](./CONTRIBUTING.md) |
| Architecture | [`DESIGN.md`](./DESIGN.md) · [`docs/adr/`](./docs/adr/README.md) |
| Packs & licenses | [`ARTIFACTS.md`](./ARTIFACTS.md) · [`NOTICE`](./NOTICE) |
| Search-quality policy | [`AGENTS.md`](./AGENTS.md) |

## Quick try (checked-in packs)

```bash
cargo build --release -p recall-cli
cargo run -p recall-cli --release -- search \
  --pack data/bsb-full.pack \
  --overlay data/bsb-expand.pack \
  --overlay data/recall-overlay.pack \
  "lost sheep"

# Optional local HTML UI (recall tier)
python3 scripts/serve_search_ui.py --port 4173
```

## Current quality position

Lexical recall, KJV drift, source-backed events, reviewed corrections,
entity/event/topic ingestion, confidence safety, and portable execution are
implemented. **Generalized semantic quality is not yet resolved.** The last
valid sealed family-disjoint batch scored R@1/3/10 **39.1/56.5/78.3**, below
the R@3 >= 85% and R@10 >= 95% release gates. Later perfect or near-perfect
suites are already-revealed compatibility floors, not proof of transfer.

The retained semantic baseline is
[`Snowflake/snowflake-arctic-embed-m-v1.5`](https://huggingface.co/Snowflake/snowflake-arctic-embed-m-v1.5),
canonical verse-only and Matryoshka-truncated to 256 dimensions. It was selected across
eight revealed suites and family-weighted slices, not by repairing individual
misses. It is materially better than the retired BGE baseline, but it still
misses the generalization gate. See
[STATUS.md](./STATUS.md) and
[generalization-resolution-v3.md](./eval/generalization-resolution-v3.md).

## What ships

| Lane | Purpose | Runtime data |
|---|---|---|
| A | Exact and partial wording | Compact `TEXT + LEX + POST` BM25 index |
| B | Weighted lexical neighbors and translation drift | `EXPAND` graph plus ref-aligned hidden KJV text |
| C | Meaning-only recall | Optional content-addressed, int8 `LVEC`; retained pack is one 256D vector per canonical verse |
| D | Bounded events, graph/topic evidence, and reviewed corrections | Typed `VROVL001` overlay built from identified source datasets |

There are no benchmark-derived runtime aliases or built-in prompt-specific
phrase rules. Graph source types are immutable at engine-open time, so
score-zero Study records cannot become ranked events or generated aliases. The
query-only planner can emit the whole query plus as many as four independently
meaningful clauses. Generic intent-balanced composition reserves coverage for
distinct clauses while keeping strong lexical results at the head. Hidden
translations and graph sources never replace BSB display text, and
low-authority evidence cannot claim `Recovered` by itself.

## Runtime payload

| Profile | Included packs | Raw bytes | MiB |
|---|---|---:|---:|
| Lexical | base + expansion | 12,062,016 | 11.50 |
| Recall | lexical + corrected knowledge/KJV overlay | 19,537,920 | 18.63 |
| Semantic engine data | all four packs | 27,496,192 | 26.22 |

Fully offline semantic search also needs the host's encoder assets. The pinned
quantized Arctic-M ONNX file is 110,145,162 bytes and its tokenizer is 711,649
bytes. The four packs plus those assets total 138,353,003 bytes; including the
two hashed JSON integrity companions makes the complete offline payload set
138,354,347 bytes (about 131.94 MiB), excluding the self-manifest and
evaluation profile. A server-provided query embedding needs
only the 26.22 MiB engine-data profile.

Exact identities, coverage, licensing, and portability boundaries are in
[ARTIFACTS.md](./ARTIFACTS.md).

## Workspace

```text
crates/
  recall-core/     planner, retrieval lanes, fusion, confidence policy
  recall-pack/     validated binary readers and writers
  recall-packer/   build-time BSB, expansion, overlay, and LVEC compiler
  recall-ffi/      hardened C ABI for native, Apple, and Android hosts
  recall-wasm/     wasm-bindgen API
  recall-cli/      inspect/search utility
  recall-eval/     deterministic retrieval and safety evaluator
bindings/
  swift/           SwiftPM package and Apple XCFramework assembly
  kotlin/          Kotlin API, JNI bridge, and Android AAR project
  js/              npm/TypeScript wrapper and deterministic WASM build
data/              canonical source, reviewed data, contracts, and packs
eval/              regressions, immutable reports, and generalization audits
scripts/           deterministic source-ingestion and embedding tools
```

## Quick start

Build the canonical 31,086-verse BSB index and search it:

```bash
python3 scripts/ingest_bsb.py \
  --input /path/to/enriched-bsb.jsonl \
  --output data/bsb.jsonl

cargo run -p recall-packer --release -- pack \
  --input data/bsb.jsonl \
  --output data/bsb-full.pack \
  --translation BSB

cargo run -p recall-cli --release -- search \
  --pack data/bsb-full.pack \
  "lost sheep"
```

The checked-in lexical and expansion artifacts use normalizer identity v2.
Changing stemming behavior requires incrementing that identity and rebuilding
base, context, and expansion packs together; stale or mixed identities are
rejected when the engine opens them and by the release verifier.

Build the comprehensive source overlay from enriched BSB event annotations,
all Selah topic rows, the Study Bible graph, nearly complete ref-aligned KJV,
and separately reviewed BibleAquifer correction evidence:

```bash
python3 scripts/build_recall_overlay.py \
  --canonical /path/to/enriched-bsb.jsonl \
  --canonical-topics /path/to/canonical-topics.browser.json \
  --topic-verse-rankings /path/to/topic-verse-rankings.browser.json \
  --study-bible-db /path/to/study_bible.db \
  --reviewed-corrections data/reviewed-corrections-v2.jsonl \
  --alternate KJV=/path/to/kjv.jsonl \
  --output /tmp/recall-overlay.jsonl

cargo run -p recall-packer --release -- overlay \
  --input /tmp/recall-overlay.jsonl \
  --output data/recall-overlay.pack
```

The 679,141,376-byte `study_bible.db` is build-only. The application receives
only validated, compact records. Study Notes cover 28,405 of 31,086 canonical
coordinates (91.4%), while the graph build imports 3,067 people, 1,264 places,
and 450 events with explicit exclusions. All 6,687 supplied Selah topics are
resolved. The Codex source-reviewed correction corpus contains 46 approved
source records and 94 globally unique exact claim surfaces across 21 canonical
books; the other 396 mined candidates remain non-runtime. See
[data/CORRECTIONS.md](./data/CORRECTIONS.md) and
[data/OVERLAYS.md](./data/OVERLAYS.md).

## Semantic Lane C

Corpus vectors are built offline. The retained release pack uses canonical
BSB verses only; enriched documents, verse windows, independent sparse Study
Note fusion, and a second Study-note vector table were measured and rejected
because they did not transfer consistently across revealed families.

```bash
python3 scripts/build_semantic_vectors.py space-id \
  --model-dir /path/to/snowflake-arctic-embed-m-v1.5-onnx \
  --pooling cls \
  --max-length 128 \
  --truncate-dims 256 \
  --query-prefix "Represent this sentence for searching relevant passages: " \
  --output data/semantic-space-arctic-m-v1.5-m256.json

python3 scripts/build_semantic_vectors.py corpus \
  --model-dir /path/to/snowflake-arctic-embed-m-v1.5-onnx \
  --input data/bsb.jsonl \
  --pooling cls \
  --max-length 128 \
  --truncate-dims 256 \
  --verse-output /tmp/arctic-m256-bsb-verse.f32

cargo run -p recall-packer --release -- semantic \
  --verse-vectors /tmp/arctic-m256-bsb-verse.f32 \
  --dims 256 \
  --translation BSB \
  --embedding-space-id 36db5d816aeac4c6f6631619d587d7caea5f829fcddfff51450d60823e58d744 \
  --output data/bsb-semantic.pack
```

The embedding-space ID binds the model bytes, tokenizer, CLS pooling, query
prefix, maximum length, 256D truncation, and L2 normalization. A same-sized
vector from another encoder fails closed. Optional query-only linear adapters
use the portable `VRQADP01` format and become part of a new content-addressed
space contract; the source-trained adapters evaluated so far were rejected and
are not required by the retained pack.

## Evaluation discipline

Already-revealed results are used only as regression and compatibility floors.
The retained Arctic candidate's eight-suite macro is R@1/3/10
**70.44/84.20/91.40**; equal-weight family-cell R@3/10 is
**80.22/89.25**. Meaning-only R@3/10 is **71.4/78.6**, and multi-intent
R@3/10 is **40/60** on those revealed slices. These numbers show improvement,
but they do not satisfy the release claim.

Every candidate must be frozen before a new family-disjoint holdout is
revealed once. The reveal must reach R@3 >= 85%, R@10 >= 95%, 100% confidence
safety, 100% applicable hard-negative ordering, and 100% required labels, with
no material quote, translation, event, latency, size, or portability
regression. Contaminated splits are quarantined in
[holdout-contamination-ledger.md](./eval/holdout-contamination-ledger.md).

For the evidence and rejected alternatives, see:

- [architecture decision log](./docs/adr/README.md)
- [generalization resolution v3](./eval/generalization-resolution-v3.md)
- [retrieval method trade-offs v2](./eval/method-tradeoffs-v2.md)
- [overfitting audit](./eval/overfit-audit-v1.md)
- [evaluation contract](./eval/README.md)
- [engine design](./DESIGN.md)

## Platform entry points

- Rust: `recall_core::Engine`
- C/Objective-C/JNI: [`recall.h`](./crates/recall-ffi/include/recall.h)
- Swift: [`bindings/swift`](./bindings/swift/README.md)
- Kotlin/Android: [`bindings/kotlin`](./bindings/kotlin/README.md)
- JavaScript/TypeScript/WASM: [`bindings/js`](./bindings/js/README.md)

Hosts supply the canonical pack plus any optional expansion, recall, and dense
packs as byte buffers. The pure query planner is mirrored across host bindings,
and the engine accepts whole-query and clause embeddings from the same
validated space. Reference parsing remains a host concern such as
[`grab-bcv`](https://github.com/dpshde/grab-bcv).

## License

Engine and tooling code are MIT. Generated data retains source rights. BSB is
the canonical display corpus; the project's KJV source is public domain;
Theographic and BibleAquifer-derived records require their source attribution
and compatible terms. The retained Snowflake encoder is an independent
Apache-2.0 host/build asset and is not vendored here.

ESV is an adapter boundary only. Crossway's storage and quotation limits mean
that no ESV text or generated ESV overlay is distributed by this repository;
an application may provide one only under an appropriate license. See
[ARTIFACTS.md](./ARTIFACTS.md#data-and-license-boundaries).
