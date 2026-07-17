# Release artifacts and portability evidence

This document records the retained compact profile measured on 2026-07-16.
Pack sizes are raw bytes, not compressed downloads or process RSS. The files
are modular so an application can choose a lexical, recall, or semantic tier.

For a one-page product verdict (what ships vs research vs unclaimed), see
[`STATUS.md`](./STATUS.md).

## Runtime packs

| File | Role | Bytes | MiB | SHA-256 |
|---|---|---:|---:|---|
| `data/bsb-full.pack` | canonical BSB `META + TEXT + LEX + POST` | 6,433,344 | 6.14 | `3782f460405389b0156e6f8e29df6b98e8b13d3118689f9afbca76e88d10b69b` |
| `data/bsb-expand.pack` | weighted `EXPAND` graph | 5,628,672 | 5.37 | `e7b26a3069e19a35f47eb389177cb867ab449f1e1955a8a1ecfdf43045206061` |
| `data/recall-overlay.pack` | KJV, sourced events/graph/topics, and reviewed corrections | 7,475,904 | 7.13 | `607b71dbe42f96ba47d79cec007ad86a653cd67051a9f105fd39ec812107b676` |
| `data/bsb-semantic.pack` | one int8 Arctic-M 256D vector per canonical verse | 7,958,272 | 7.59 | `9551d428dedde6168326e2dcc5c7818abcbd86de8334b9310b6c90a2cc0afd8d` |

Every pack targets BSB and the same 31,086 canonical coordinates. The dense
pack uses embedding-space ID:

```text
36db5d816aeac4c6f6631619d587d7caea5f829fcddfff51450d60823e58d744
```

The exact release also contains two hashed integrity companions:

| File | Role | Bytes | SHA-256 |
|---|---|---:|---|
| `data/bsb-expand.provenance.json` | source/model/parameter identity embedded by SHA-256 in `EXPAND` v2 | 667 | `2b45a28ad9426f73df4c2aef89501b2fda91dff2326d1059243cd5ccb97befd3` |
| `data/semantic-space-arctic-m-v1.5-m256.json` | dense encoder-space contract | 677 | `3742144c4fcba0ec13918788a196f8f796c1b5eae449b125fabbc6f6e5d04813` |

The checked-in
[`data/release-artifacts-v2.json`](./data/release-artifacts-v2.json) is the
machine-readable, exact six-artifact manifest. The verifier requires every
role and filename, rejects extra manifest fields and duplicate JSON keys,
parses the binary structures, requires lexical normalizer v2 in both LEX and
EXPAND, requires the same identity in expansion provenance, and checks the
`EXPAND`-to-provenance binding. A pack and manifest that are consistently
rehashed under a stale normalizer are still rejected.
Validate a populated release directory with:

```bash
python3 scripts/verify_release_artifacts.py
```

## Distribution profiles

| Profile | Included packs | Raw bytes | MiB |
|---|---|---:|---:|
| Lexical | base + expansion | 12,062,016 | 11.50 |
| Recall | lexical + corrected KJV/knowledge overlay | 19,537,920 | 18.63 |
| Semantic engine data | all four packs | 27,496,192 | 26.22 |

All six release artifacts, including the two integrity companions, total
27,497,536 bytes. The companions are release-verification inputs rather than
runtime engine buffers.

The Rust engine does not contain an inference runtime. A host that receives
query embeddings from a service needs only the relevant pack profile. Fully
offline semantic search also needs the pinned encoder assets:

| Host/build asset | Bytes | MiB | SHA-256 |
|---|---:|---:|---|
| quantized [`Snowflake Arctic-M v1.5`](https://huggingface.co/Snowflake/snowflake-arctic-embed-m-v1.5) ONNX | 110,145,162 | 105.04 | `a18f437b2466863901a0bdc14904cf93246f5ecce0b656fc773bc2b7b2f84f6e` |
| tokenizer | 711,649 | 0.68 | `91f1def9b9391fdabe028cd3f3fcc4efd34e5d1f08c3bf2de513ebb5911a1854` |

The four packs and those two assets total 138,353,003 bytes. Including the two
integrity companions makes the complete offline payload set 138,354,347 bytes,
about 131.94 MiB, excluding the self-manifest and evaluation profile. The
encoder contract fixes CLS pooling, maximum length 128, the query prefix,
Matryoshka truncation to 256 dimensions, and L2 normalization; see
[`data/semantic-space-arctic-m-v1.5-m256.json`](./data/semantic-space-arctic-m-v1.5-m256.json).
Same-dimensional vectors from a different contract fail closed.

The last pre-v4 development benchmark used 220 warmed single queries: Arctic
ONNX tokenization/inference measured median 3.347 ms and p95 6.137 ms, while a
persistent native all-lane 256D search measured median 139.366 ms and p95
341.633 ms. Those figures predate the current normalizer-v2 base/overlay
identities and are retained only as historical scale, not current-v4 or
target-device/WebAssembly promises. Fresh target profiling remains required.

## Overlay coverage and provenance

The deterministic overlay intermediate contains 42,847 JSONL rows including
its manifest. It is compiled into typed `VROVL001` records; the runtime never
loads the JSONL or source databases.

| Source | Measured build coverage | Runtime authority |
|---|---|---|
| BSB event annotations | 247 accepted from 402; 155 explicitly quarantined; 396 collision-checked signatures cover 195 events | exact bounded event, `Related` |
| Selah topical artifacts | 6,687/6,687 valid targets; 0 missing/skipped; 3,545 generated article aliases | constrained topic evidence, never authoritative alone |
| Study Bible/Theographic people | 3,067/3,069 entities; 445 source aliases | score-zero explanatory evidence; 0 runtime promotions |
| Study Bible/Theographic places | 1,264/1,274 entities | score-zero explanatory evidence; 0 runtime promotions or generated transition aliases |
| Study Bible/Theographic events | 450/450 events; 28 noncanonical mentions counted | all 450 remain score-zero event-shadow evidence; 0 runtime promotions |
| KJV | 31,085/31,086 canonical coordinates (99.9967%) | selective exact or bounded-fuzzy hidden-translation evidence; BSB remains display text |
| Codex source-reviewed BibleAquifer corrections | 46 approved records, 94 unique exact claim surfaces, 21 target books | exact `OftenMisremembered` evidence with provenance |

The correction miner examined all 102,673 BibleAquifer rows and found 442
explicit-correction-language candidates. The identified
`codex-source-review-v2` review approved 46 and rejected 396 using source
evidence only. This is not an independent human editorial review. Candidate
rows cannot enter the runtime.
End-to-end verification of the reviewed corpus exercises all 94 approved claim
surfaces and checks that each source-derived anchor is rank one with the
required correction label, provenance, and every declared structured passage.
This is coverage of the reviewed source corpus, not evidence that every human
misconception has been cataloged.

See [`data/CORRECTIONS.md`](./data/CORRECTIONS.md) and
[`data/OVERLAYS.md`](./data/OVERLAYS.md) for validation and trust policy.

## Study Bible build input

The pinned
[`study_bible.db`](https://studybible-mcp.fly.dev/download/study_bible.db)
from [`djayatillake/studybible-mcp`](https://github.com/djayatillake/studybible-mcp)
is 679,141,376 bytes (647.68 MiB), SHA-256
`b245186271abf8ae2644e11b98716b83b0728f05704e5358e70c949b8e8beca3`.
It is opened read-only at build time and is not an application dependency.

Its source-wide Study Notes align nonempty context to 28,405 of 31,086
canonical verses (91.4%); 2,681 rows remain aligned fallbacks. Sparse Study
Note context, dense note tables, enriched documents, and the database's
precomputed 1,536D verse vectors were evaluated as independent candidates.
None is in the retained compact profile: context fusion did not improve the
required cutoffs consistently, and the higher-dimensional Study-vector lane
regressed broad revealed transfer while adding 41,362,960 bytes (39.45 MiB)
over the compact dense pack and transform. The graph and reviewed corrections remain because they have explicit
source semantics and bounded authority.

## Query-adapter portability

`VRQADP01` is a little-endian row-major float32 query-only linear transform.
Its dimensions and SHA-256 are included in a new embedding-space contract.
Parsers and application helpers exist for host-side portable use in
JavaScript, Swift, and Kotlin; C hosts can apply the same documented matrix
before calling the unchanged embedding API. A separate compact nonlinear
research format also exists in build tooling.

No adapter is required by the retained Arctic-M pack. The source-trained
linear adapter and compact nonlinear adapter were rejected as release
candidates because their source-domain gains did not transfer or were too weak
to justify changing the production space. Format support is a capability, not
a claim that those candidate weights should ship.

## Platform matrix

All targets consume the same pack bytes and JSON result shape. They validate
format bounds, section uniqueness, canonical identity, vector dimensions, and
embedding-space identity before search.

| Target | Integration | Verified boundary |
|---|---|---|
| Rust/native | `recall-core`, `recall-pack`, `recall-ffi` | shared planner, packed lanes, fail-closed parsing, typed error boundary |
| C | `recall-ffi` plus `recall.h` | stable JSON ABI; caller supplies packs and optional whole/clause embeddings |
| Swift / iOS / macOS | SwiftPM wrapper and XCFramework assembly | 22/22 host tests; real normalizer-v2 pack opened through current dylib; isolated SwiftPM scratch prevents a stale XCFramework from shadowing source tests; target slices depend on provisioned Rust targets |
| JavaScript / TypeScript / Web | `wasm-bindgen` plus npm wrapper | `ArrayBuffer` packs, typed embeddings, query planner, and `VRQADP01` helper suitable for browser or worker |
| Kotlin / Android | Kotlin API, JNI, and AAR project | 22/22 host tests plus real base/overlay/LVEC-v2 JNI smoke; minSdk-21-safe adapter arithmetic; device/AAR proof still requires a provisioned Android SDK/NDK toolchain |

The host owns the encoder because ONNX/Core ML/WebGPU/TFLite choices differ by
platform. The engine remains portable even when a particular offline model is
too large for a product tier. Web clients can ship only the 11.50 MiB lexical
profile, while native clients or servers opt into the 26.22 MiB semantic data.
Those figures are pack data only. The verified generated Web binding adds a
601,163-byte WASM module; its complete generated `pkg` directory is 630,579
bytes before npm/tar compression.

## Retrieval evidence

The last valid sealed family-disjoint holdout remains the governing
generalization result: R@1/3/10 **39.1/56.5/78.3**, meaning-only
**12.5/37.5/62.5**, multi-intent **0/0/50**, and confidence safety **100%**.
That candidate failed the release gate.

The retained Arctic-M candidate was selected only with already-revealed
regression data. Across eight suites its macro R@1/3/10 is
**70.44/84.20/91.40** and its family-cell R@3/10 is **80.22/89.25**.
Meaning-only R@3/10 is **71.4/78.6** and multi-intent R@3/10 is **40/60**.
Those measurements establish compatibility and model-selection evidence, not
generalized superiority. See:

- [`eval/generalization-resolution-v3.md`](./eval/generalization-resolution-v3.md)
- [`eval/method-tradeoffs-v2.md`](./eval/method-tradeoffs-v2.md)
- Historical v1 matrix: [`eval/archive/reports/method-tradeoffs-v1.md`](./eval/archive/reports/method-tradeoffs-v1.md)
- [`eval/overfit-audit-v1.md`](./eval/overfit-audit-v1.md)
- [`eval/holdout-contamination-ledger.md`](./eval/holdout-contamination-ledger.md)

## Data and license boundaries

- Repository engine and tooling code are MIT.
- BSB is canonical display text and is used under its source permissions. The
  generated index does not relicense it.
- The project's KJV source is public domain; every result still maps to BSB.
- Selah browser artifacts retain their source-repository rights.
- Theographic graph data and BibleAquifer correction evidence use their
  source attribution and CC BY-SA 4.0 terms. The raw database is not
  redistributed merely because the engine is MIT.
- Snowflake Arctic-embed-m-v1.5 is an independent Apache-2.0 host/build asset.
- The Study database's precomputed OpenAI vectors are not part of the compact
  release and carry an additional provenance and distribution-review burden.
- ESV is a licensed adapter boundary only. No ESV text or generated ESV index
  belongs in this repository by default. See
  [Crossway permissions](https://www.crossway.org/permissions/) and
  [ESV API terms](https://api.esv.org/).

## Reproduce identities

```bash
stat -f '%N %z' \
  data/bsb-full.pack \
  data/bsb-expand.pack \
  data/recall-overlay.pack \
  data/bsb-semantic.pack

shasum -a 256 \
  data/bsb-full.pack \
  data/bsb-expand.pack \
  data/recall-overlay.pack \
  data/bsb-semantic.pack

python3 scripts/verify_release_artifacts.py
```
