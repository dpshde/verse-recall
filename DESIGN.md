# verse-recall design

Portable tip-of-the-tongue scripture retrieval over a canonical BSB corpus.

This document describes **how the system works today**. Load-bearing *why*
choices are recorded as Architecture Decision Records under
[`docs/adr/`](./docs/adr/README.md). Prefer a new ADR when changing portable
boundaries, pack formats, fusion policy, encoder contracts, trust rules, or
eval-driven runtime data policy.

## Goals and boundaries

1. **Embeddable** in native, iOS, Android, browser, and worker processes.
2. **Offline and deterministic** in `recall-core`: pure CPU over validated,
   caller-owned bytes, with no file, SQLite, model, or network dependency.
3. **Build-time intelligence** for corpus ingestion, embeddings, alignment,
   source review, and quantization.
4. **Modular degradation**: lexical, expansion, source overlay, and dense packs
   are independently optional.
5. **Stable portability boundary**: Rust core, C/JSON ABI, Swift/Kotlin
   wrappers, and WebAssembly use identical retrieval semantics.
6. **Generalization over examples**: production behavior comes from
   query-independent mechanisms and identified source datasets, never from
   evaluation prompts or gold references.

Reference parsing and displaying multiple translations are host concerns.
Ref-aligned translations may be searched as hidden evidence, but every result
returns canonical BSB text.

## Architecture

```text
build time
  BSB -> TEXT + compact LEX/POST
  vocabulary relationships -> weighted EXPAND graph
  enriched BSB events + KJV + Selah topics + Study graph
      + reviewed source corrections -> validated VROVL001 overlay
  Arctic-M v1.5 canonical BSB verses -> normalized 256D int8 LVEC

runtime
  host supplies base plus optional EXPAND / VROVL / LVEC byte buffers
  query -> query-only whole/clause plan (at most four useful clauses)
        -> A indexed lexical
        -> B expansion + selective hidden-translation matching
        -> C content-addressed dense retrieval + constrained topic agreement
        -> D source-backed events/corrections and explanatory graph data
        -> trust-aware fusion + intent-balanced clause composition
        -> ranked BSB refs/text, passages, evidence, and confidence
```

The retained semantic artifact is one canonical-verse table. Verse windows,
enriched dense documents, independent sparse Study Note fusion, a second dense
Study-note table, Study/OpenAI vector alignment, source-trained query
adapters, and optional range-bound `PVEC` passage vectors are supported
research paths or rejected candidates, not hidden parts of the release
profile. `PVEC` may load beside LVEC for ablations; it is permanently
`Related` evidence and cannot mint recovery without independent verse-vector
support.

## Query planning and multi-intent composition

The planner is query-only. It has no access to packs, verses, embeddings,
evaluation families, or gold answers. It returns the complete query plus zero
to four independently meaningful clauses. Splits preserve negation and are
rejected when a resulting fragment is too weak. Version two preserves clause
source order and lets composition distinguish independent, alternative, and
subordinate boundaries without changing the public JSON shape. Rust, Python,
JavaScript, Swift, and host bindings share conformance fixtures for planner
identity `verse-recall-clause-v2`.

The host embeds the whole query and every accepted clause with the same
content-addressed encoder contract. Retrieval is performed independently for
each representation. Composition retains the whole-query head, each clause's
best distinct result, and a bounded round-robin of distinct secondary results.
The depth grows only with clause count. This prevents one dominant clause from
silently erasing another memory without adding verse-specific split rules.

Clause coverage is a generic mechanism, but generalized multi-intent quality
is still unresolved. Revealed slices remain weak, so the planner is retained
for safety and monotonic coverage rather than advertised as a solved result.

## Recall overlay and trust hierarchy

The runtime accepts one canonical `META + TEXT` identity and optional
section-only packs. `VROVL001` records use `OVERLAY:<canonical>` and resolve
targets by normalized canonical reference, never row position.

Sources have different authority:

1. **Bounded BSB events** may create a direct result and structured passage,
   but remain `Related`. Unsupported, fragmented, singleton, or overbroad
   event ranges are quarantined at build time. Provenance-free direct concept
   mappings are rejected by the release builder.
2. **Hidden translations** require a selective contiguous phrase. Exact
   matches can claim recovery only with adequate corpus coverage, low
   ambiguity, and cross-translation agreement. A generic bounded-fuzzy path
   allows one substitution only for queries of at least five words, with at
   least four exact tokens and no more than 20% mismatch; it is always
   `Related`, and repeated or ambiguous surfaces are suppressed.
3. **Selah topics** cover all supplied source rows but are heuristic. A topic
   is normally score-zero evidence. Scored topic agreement requires every
   short-label atom to be independently supported and corroborated by dense
   retrieval inside the validated passage. It remains `Related`.
4. **Study Bible/Theographic people, places, and events** are score-zero,
   passage-grouped explanation. They can annotate an independently retrieved
   result but cannot create a candidate, change rank, or raise confidence.
   Source record types are immutable at engine-open time: canonical text,
   overlapping topics, or other graph records cannot promote a graph shadow or
   manufacture a ranked alias.
5. **Reviewed corrections** are exact full-query claims backed by a cited
   BibleAquifer row and an identified source-review approval. The retained v2
   corpus records `codex-source-review-v2`, not an independent human editorial
   review. Approved records may emit
   `OftenMisremembered`. Fuzzy, partial, semantic, or benchmark-derived
   correction matching is forbidden.
6. **Ordinary lexical, expansion, and dense candidates** are ranked evidence.
   They cannot override a stronger source-backed resolver merely by
   accumulating incomparable raw scores.

Multiple implementations of one conceptual lane are consolidated before
fusion. Final ties use canonical verse index, making repeated processes
deterministic. Public scores are order-only and must not be persisted or
thresholded across engine versions.

There are no built-in acceptance-example phrase tables. Source curation is
allowed only through reviewed, provenance-bearing datasets compiled through
the same generic overlay path.

### Translation license boundary

KJV is the public-domain proving lane. ESV can use the same ref-aligned adapter
only when a host has rights to construct and distribute that offline index.
Displaying BSB does not make a hidden ESV copy unlicensed. No ESV text or
generated ESV overlay is committed by default; see
[Crossway permissions](https://www.crossway.org/permissions/) and the
[ESV API terms](https://api.esv.org/).

## Pack format

`.pack` files are little-endian with 64-byte-aligned sections. The header and
section table validate format version, bounds, uniqueness, reserved fields,
and declared lengths before a lane opens.

| Section | Role | Retained measured size |
|---|---|---:|
| `META` | translation, verse count, defaults | tens of bytes |
| `TEXT` | canonical refs and BSB text | part of 6,433,344-byte base |
| `LEX + POST` | term dictionary, lengths, and BM25 postings | part of base |
| `EXPAND` | weighted, typed neighbor groups with embedded provenance digest | 5,628,672-byte pack |
| `PHRASE` | typed `VROVL001` sourced events/topics, alternate text, graph, corrections | 7,475,904-byte pack |
| `LVEC` | aligned int8 dense tables plus encoder identity | 7,958,272-byte 256D verse-only pack |
| `CLEX + CPOST` | optional aligned source context | supported, not retained in release |

The canonical base is 6,433,344 bytes. Base plus expansion is 12,062,016
bytes; adding the current overlay is 19,537,920 bytes; all four retained packs
are 27,496,192 bytes. The expansion pack's embedded SHA-256 binds its 667-byte
provenance companion; the 677-byte dense-space contract is also independently
hashed in the six-artifact release manifest.

### Lexical normalizer identity

LEX, optional CLEX, and EXPAND terms are produced by the same deliberately
small runtime stemmer. Its current identity is `normalizer_version = 2`.
LEX/CLEX store that identity in their section header; release EXPAND v2 stores
it both in the section header and in the SHA-256-bound provenance manifest.
`recall-core` and `recall-pack` assert the identity matches at compile time,
and readers reject a section carrying any other value. A stemmer behavior
change therefore requires a version increment and a rebuild of every lexical
and expansion artifact; mixing old postings or neighbors with a new query
normalizer fails closed instead of silently losing recall.

The library can still parse the provenance-less EXPAND v1 envelope when it was
built with normalizer v2, for explicit compatibility use. The release verifier
requires EXPAND v2 plus its matching provenance companion.

### Dense vector body

LVEC v2 stores one canonical verse table and optionally a legacy window table:

```text
magic "LVC1"
u16 version
u16 dimensions
u32 verse_count
u8 table_count
reserved bytes
32-byte embedding_space_id
row-major int8 tables
```

LVEC v3 adds explicit table roles (`verse`, `window`, or `study-note`) so an
auxiliary representation cannot be misinterpreted. The retained one-table
Arctic pack remains v2-compatible. Both formats fail closed on dimensions,
row count, malformed lengths, invalid roles, nonzero reserved data, or an
embedding-space mismatch. Legacy v1 can be opened only through its explicit
compatibility API because it cannot identify the query encoder.

Input rows are L2-normalized and symmetrically quantized. The retained
embedding-space ID binds:

- quantized Snowflake Arctic-embed-m-v1.5 model bytes;
- tokenizer bytes;
- CLS pooling;
- maximum length 128;
- query prefix `Represent this sentence for searching relevant passages: `;
- Matryoshka truncation to 256 dimensions; and
- float32 L2 normalization.

The identifier is
`36db5d816aeac4c6f6631619d587d7caea5f829fcddfff51450d60823e58d744`.

### Query-only adapter contract

`VRQADP01` is an optional row-major float32 matrix applied after the base
encoder and before final normalization. It may be rectangular. The adapter
SHA-256, direction (`post-encoder-query-only-v1`), and dimensions become part
of a different embedding-space identity. JavaScript and Swift include parser
and application helpers; C and Kotlin hosts can implement the same documented
matrix multiplication without changing the engine ABI.

JavaScript, Swift, and Kotlin ship the same checked parser/application helper;
C hosts can apply the documented matrix before calling the unchanged embedding
API. Format support does not select an adapter. The evaluated source-trained
linear and nonlinear candidates were rejected; the retained compact release
uses no query adapter.

## Runtime retrieval pipeline

1. **Normalize** Unicode-safe text into surface tokens and conservative stems.
2. **Plan** whole query and independently useful clauses without corpus access.
3. **Lane A** runs BM25 over `LEX + POST`; legacy text-only packs use a slower
   scan fallback.
4. **Lane B** applies grouped `EXPAND` evidence and exact/bounded-fuzzy hidden
   translation matching. One source stem contributes at most one inferred cue.
5. **Lane C** validates and scans the dense table when the host supplied the
   correct whole/clause embeddings. Strong direct or high-coverage lexical
   recovery protects the head. Optional context can corroborate but cannot
   cast an independent vote or displace dense rank one.
6. **Lane D** resolves bounded source events and exact reviewed corrections;
   lower-trust graph evidence annotates only scored results.
7. **Fuse** fixed-depth lane candidates with trust tier, agreement, and bounded
   rank-based composition. A surface-overlap hard negative cannot win simply
   by accumulating Lane A/B weight in semantic mode.
8. **Compose clauses** with distinct primary and bounded secondary coverage.
9. **Return** BSB text, canonical refs, structured passages, matched/expanded
   terms, lane evidence, and `Recovered`, `Related`, or
   `OftenMisremembered` confidence.

Independent ordinary all-lane RRF and raw dense-rank summation were evaluated
and reverted because they regressed family breadth or hard-negative ordering.
The fusion policy is therefore conservative and evidence-gated, not an
assumption that more votes are always better.

## Portable API

The C ABI opens one or more byte buffers, exposes engine metadata, and searches
with or without a validated query embedding. Additive calls accept the
embedding-space ID and planned clause vectors. Results and diagnostics use a
stable JSON shape, and panics are contained at the FFI boundary. Pack buffers
may be released after a successful open.

The same semantics are exposed through:

- Rust `recall_core::Engine`;
- C/Objective-C/JNI `recall.h`;
- SwiftPM and XCFramework assembly;
- Kotlin/JNI and Android AAR assembly; and
- `wasm-bindgen` plus the JavaScript/TypeScript wrapper.

The core does not dictate ONNX Runtime, Core ML, TFLite, WebGPU, or a server
encoder. That separation keeps retrieval portable while allowing products to
choose their inference and download budget.

## Source coverage and build-only data

The 679,141,376-byte `study_bible.db` is opened read-only during builds. It is
not shipped or loaded at runtime. Current measured coverage includes:

- 3,067 people, 1,264 places, and 450 graph events;
- nonempty Study Note context for 28,405/31,086 canonical coordinates;
- all 6,687 supplied Selah topics;
- 31,085/31,086 KJV coordinates; and
- 46 reviewed BibleAquifer correction records with 94 exact claim surfaces
  across 21 canonical books.

Study Note text, precomputed vectors, and raw graph tables do not enter the
runtime merely because they exist. Each candidate representation is evaluated
independently. The retained runtime contains only records whose source role and
authority can be enforced.

## Evaluation and kill criteria

Every prompt has category, family, split, gold references, and optional
passage, hard-negative, and confidence requirements. The evaluator reports
R@1/3/10/50/100, MRR, category/family slices, confidence safety, hard-negative
ordering, required correction labels, and structured-passage coverage.

Generalized quality requires a sealed, family-disjoint first shot with:

- R@3 >= 85%;
- R@10 >= 95%;
- 100% confidence safety;
- 100% applicable hard-negative ordering;
- 100% required labels and structured passages; and
- no material regression in exact quotes, translation recall, events,
  latency, size, or portability.

The last valid sealed result was **39.1/56.5/78.3** at R@1/3/10 and failed.
The retained Arctic candidate's eight-suite revealed macro is
**70.44/84.20/91.40**, while family-cell R@3/10 is **80.22/89.25**. Revealed
suites are compatibility floors only. Generalized semantic quality remains
unresolved until a new untouched batch passes every gate.

### Red-team loop

1. Freeze family definitions and a disjoint holdout before selecting a
   candidate.
2. Tune only on development and already-revealed regressions.
3. Freeze code, data, weights, model, and artifact identities.
4. Verify no normalized query, family, gold passage, or overlap crosses split
   roles.
5. Reveal once with all existing corpora registered.
6. Save immutable JSON and Markdown first-shot reports.
7. Immediately retire the revealed batch to regression.
8. Do not patch its misses; freeze a new disjoint holdout for the next
   candidate.
9. Treat 100% as a trigger for a rarer and more diverse red-team batch.

Quarantined splits are recorded without prompt contents in
[eval/holdout-contamination-ledger.md](./eval/holdout-contamination-ledger.md).
The anti-hardcoding policy in [AGENTS.md](./AGENTS.md) governs every change.

## Validation

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
python3 -m unittest discover -s tests
cd bindings/js && npm test
```

Apple and Android toolchain-specific acceptance instructions live in their
binding `VERIFICATION.md` files. Current artifact and quality evidence is in
[ARTIFACTS.md](./ARTIFACTS.md) and
[eval/generalization-resolution-v3.md](./eval/generalization-resolution-v3.md).
