# Resolution evidence v2 — corrected status

This document records the v2 audit and is superseded for current artifacts by
[`generalization-resolution-v3.md`](./generalization-resolution-v3.md).
Its central conclusion remains binding: already-revealed green suites cannot
establish generalized semantic quality.

## Governing v2 holdout result

The last valid sealed v2 first shot scored:

- R@1 **39.1%**;
- R@3 **56.5%** (target 85%);
- R@10 **78.3%** (target 95%);
- confidence safety **100%**;
- meaning-only R@3/10 **37.5/62.5%**; and
- multi-intent R@3/10 **0/50%**.

That result invalidated the earlier statement that a 77.5/85/95 revealed
regression suite had resolved semantic quality. The former BGE two-table
candidate and its perfect five-prompt frontier are historical regression
evidence only; BGE is no longer the retained model.

## Current status by problem

| Problem | General mechanism | Current evidence | Status |
|---|---|---|---|
| Meaning-only recall | content-addressed Arctic-M v1.5, canonical verse-only, Matryoshka 256D | best revealed suite-macro R@3/10 84.20/91.40; family-cell 80.22/89.25; no new sealed pass in this report | **Not resolved** |
| Multi-intent memory | query-only whole/clause planner plus distinct primary and bounded secondary composition | generic tests protect monotonic coverage; revealed multi-intent R@3/10 remains 40/60 | **Not resolved** |
| Exact and near-quote stability | indexed BM25 and strong-evidence head protection | quote/phrase regressions remain compatibility floors; dense candidates cannot displace an exact direct recovery | Implemented |
| Translation drift | 31,085/31,086-coordinate hidden KJV plus conservative exact and one-substitution fuzzy matching | broad revealed translation slices remain strong; fuzzy evidence is always `Related` and ambiguity is suppressed | Implemented for KJV; ESV license-gated |
| Named concepts/events | 247 bounded BSB events and 450 Study graph event shadows; provenance-free mappings retired | structured passages and event regressions remain strong; 155 uncertain BSB events are quarantined | Implemented for ingested source coverage |
| Topic breadth | all 6,687 Selah topics through a constrained multi-cue/dense agreement path | all supplied targets resolve; topic evidence cannot become authoritative alone | Implemented conservatively |
| Entity/place coverage | 3,067 people and 1,264 places from the Study/Theographic graph | build manifest reports missing and noncanonical rows; runtime graph data is score-zero explanation | Implemented for ingested coverage |
| False details and correction labels | separately reviewed BibleAquifer corpus with exact full-query matching | all 442 mined candidates reviewed; 20 approved records, 42 claim surfaces, 15 target books; source/runtime verifier covers every approved surface | Source corpus implemented; not a claim of every misconception |
| Confidence safety | rank and confidence are separate; only sufficiently strong evidence can claim recovery | v2 sealed safety 100%; revealed compatibility suites continue to protect empty-gold and wrong-answer behavior | Implemented, must pass each new holdout |
| Artifact integrity | four modular, content-addressed packs; encoder identity fails closed | current engine data 26.19 MiB; exact hashes in `ARTIFACTS.md` | Implemented |
| Cross-platform use | shared Rust core, C ABI, Swift, Kotlin/JNI, and JavaScript/WASM | same pack bytes, planner contract, JSON result shape, and embedding-space checks | Implemented; Android device proof needs provisioned tooling |

## What changed after v2

1. The BGE 384D verse/window release candidate was retired. A broad
   encoder/representation bake-off selected Arctic-M 256D verse-only as the
   strongest compact baseline across eight revealed suites and family cells.
2. Generic whole-query plus clause planning and intent-balanced secondary
   coverage replaced reliance on a single whole-query representation for
   multi-intent memories.
3. Prompt-specific/built-in phrase rules were removed. Runtime knowledge now
   enters through source data with provenance, typed authority, and measured
   coverage.
4. A comprehensive correction pipeline reviewed source-only BibleAquifer
   evidence. Failed eval prompts and gold references were not sources.
5. Sparse and dense Study Note fusion, enriched documents, window tables,
   generic cross-encoders, Nomic, full/PCA Study-vector alignment, all-lane
   RRF, and linear/nonlinear query adapters were rejected when their gains did
   not survive breadth, safety, source-gate, size, or portability checks.
6. Quarantined holdouts are explicitly recorded in
   [`holdout-contamination-ledger.md`](./holdout-contamination-ledger.md) and
   cannot support any claim.

## Remaining shortcomings

1. No result in this document passes a fresh sealed family-disjoint semantic
   gate. Generalized semantic quality remains unresolved.
2. Revealed meaning-only and multi-intent slices remain below target even for
   the best compact encoder.
3. Hard-negative ordering is below the required 100% across the encoder
   bake-off; a new candidate must pass it without suppressing valid recall.
4. The fully offline Arctic profile is about 131.91 MiB including the pinned
   quantized encoder and tokenizer. The 26.19 MiB engine-data profile assumes
   host/server query inference.
5. ESV remains an adapter boundary only; no ESV text is distributed without a
   separate Crossway license.

The correct release language is: source integrity, lexical/translation recall,
reviewed event/topic/entity coverage, correction provenance, safety policy,
and portability are substantially implemented; **generalized meaning-only and
multi-intent retrieval are not proven resolved**.
