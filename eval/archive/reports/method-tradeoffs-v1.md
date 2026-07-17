# Retrieval method trade-offs — diverse-query bake-off

> **Superseded for current packs by
> [`method-tradeoffs-v2.md`](./method-tradeoffs-v2.md)** (2026-07-17 live
> re-run, majority head-to-head scoring). This v1 document remains historical
> encoder-selection narrative.

## Decision

Retain **Snowflake Arctic-embed-m-v1.5, canonical verse-only, Matryoshka
256D** as the compact semantic baseline. Keep indexed lexical, weighted
expansion, hidden KJV, and reviewed source evidence alongside it. Retain the
query-only clause planner and intent-balanced result composition.

This is a model and compatibility decision, not a generalized-quality claim.
All encoder comparisons used eight already-revealed suites. The retained
candidate's suite-macro R@1/3/10 is **70.44/84.20/91.40** and its equal-weight
family-cell R@3/10 is **80.22/89.25**. It is below the generalization release
gate, and every encoder tested remains below 100% hard-negative ordering.

## Scope and non-leakage

The bake-off used only these revealed suites: semantic quality v2,
tip-of-the-tongue, overlay adversarial, overfit diagnostic, overfit
confirmation, revealed holdout v2, family regression, and red-team prompts.
No quarantined or then-sealed holdout was used to select an encoder,
representation, adapter, or fusion rule. No prompt text, fragment, gold
reference, or failure was copied into production code or data.

The accompanying JSON preserves the historical artifact identities used by
its original BGE-era ablation. Those hashes are evidence for that experiment,
not a current release inventory; `data/release-artifacts-v2.json` is the only
current release manifest.

Suite macro gives each suite equal weight. Family-cell macro gives each
retrieval-bearing `(suite, family)` cell equal weight, preventing a large easy
slice from hiding a rarer family.

## Traditional retrieval profiles

| Method | Runtime data | Strengths | Weaknesses | Position |
|---|---:|---|---|---|
| Raw keyword overlap | base only, 6.14 MiB | transparent and deterministic; good exact-token baseline | no robust phrase weighting, translation bridge, events, or meaning-only recall | diagnostic only |
| Indexed BM25 | base only, 6.14 MiB | compact, fast, excellent near-quotes | fails zero-overlap paraphrases and translation-specific wording | required foundation |
| BM25 + weighted expansion | 11.50 MiB | portable correction of modest vocabulary drift | synonym neighborhoods add noise and do not ground narratives | retained |
| Lexical + KJV/source overlay | 18.63 MiB | strongest translation, named-event, entity, and correction behavior without an encoder | source licensing/review burden; sparse knowledge is not semantic similarity | retained |
| Study Bible cross-reference promotion | graph plus source/target postings | broad, source-derived related-passage coverage without query aliases | generic promotion and dense-top-N filtering produced no consistent cutoff gain and displaced stronger heads | rejected |
| Dense retrieval alone | base + 7.59 MiB LVEC | generic low-overlap candidates without query aliases | loses translation, graph, direct phrase, and safety advantages | diagnostic only |
| Guarded hybrid | all packs, 26.22 MiB | complementary quote, translation, event, and semantic coverage | larger; requires a host encoder; still weak on arbitrary meaning and multi-intent | retained baseline |

An earlier controlled 108-prompt ablation showed the same complementarity:
the strongest encoder-free hybrid had better head rank than its dense hybrid,
while dense retrieval bought deeper semantic candidates. Those prompts are now
regression evidence. The table is not reused as proof that the new Arctic
candidate generalizes.

The Study database also exposes 404,501 cross-reference rows. A generic
experiment expanded dense candidates through those source/target edges and,
in a stricter variant, allowed only graph neighbors already present in the
dense top-N. Neither strategy improved broad revealed cutoffs; fixed graph
insertion displaced valid lexical/dense heads in several families. The graph
therefore remains a research signal rather than another runtime vote. No
query-to-cross-reference edge was selected from a failed prompt.

## Verse-only encoder comparison

All rows use canonical BSB verse vectors, the same engine, depth, clause
planner, and non-vector packs. Metrics are macro averages over the eight
revealed suites used for selection.

| Encoder | Dimensions | Encoder assets | LVEC | Macro R@1 | R@3 | R@10 | Family R@3 | Family R@10 | Decision |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| BGE-small v1.5 | 384 | 32.75 MiB | 11.38 MiB | 59.16 | 75.11 | 89.87 | 72.82 | 88.40 | reject breadth |
| Arctic-S | 384 | 33.12 MiB | 11.38 MiB | 67.89 | 81.79 | 90.56 | 77.57 | 87.85 | smaller fallback, not selected |
| Arctic-M | 128 | 105.72 MiB | 3.79 MiB | 60.62 | 71.92 | 87.53 | 68.07 | 85.59 | reject quality; encoder size unchanged |
| **Arctic-M** | **256** | **105.72 MiB** | **7.59 MiB** | **70.44** | **84.20** | **91.40** | **80.22** | **89.25** | **retain compact baseline** |
| MiniLM embedding | 384 | 22.40 MiB | 11.38 MiB | 64.24 | 78.21 | 88.47 | 74.69 | 85.75 | reject breadth |
| Nomic v1.5 | 256 | 131.61 MiB | 7.59 MiB | 62.73 | 80.08 | 89.69 | not selected | not selected | reject: larger and weaker macro transfer |
| all-MPNet-base-v2 | 768 | 105.47 MiB | 22.77 MiB | 62.78 | 76.62 | 89.19 | 72.82 | 87.23 | reject: weaker at every pooled release cutoff and 3x LVEC |

Arctic-M 256D wins seven of eight R@3 suite comparisons against MiniLM; the
isolated MiniLM win occurs on revealed v2. Nomic improves some individual
suites but is weaker on the equal-weight macro and has a larger quantized
encoder. Arctic-M 128D saves only 3.80 MiB of LVEC because it does not shrink
the 105.72 MiB host encoder.

The later Apache-2.0 all-MPNet-base-v2 check was independently motivated by
its 1B-pair sentence/paragraph training corpus and used all 31,086 BSB verses.
With the retained bounded semantic-tail schedule it produced pooled product
R@1/3/10 **61.4/75.0/88.6**, versus Arctic's **66.7/80.3/90.9**, while its
23,874,304-byte LVEC was three times larger. The complete identity and
reproduction result are recorded in
[`encoder-bakeoff-mpnet-v1.md`](./encoder-bakeoff-mpnet-v1.md).

On revealed query-type slices, the selected encoder reaches meaning-only
R@1/3/10 **51.8/71.4/78.6** and multi-intent **0/40/60**. Entity/event is
**74.5/90.9/100**, translation **92.9/97.6/97.6**, and phrase
**90/96.7/100**. The unevenness is the reason a new sealed batch, not the macro
average, controls any resolution claim.

## Representation and fusion experiments

| Candidate | Measured benefit | Cost or failure | Decision |
|---|---|---|---|
| Arctic-M verse + old global window | suite-macro R@3 86.53 vs 84.20; revealed multi-intent R@3 60 vs 40 | doubles LVEC to 15.18 MiB; hard-negative ordering falls from 90.5% to 85.7%; several slices regress | reject |
| Enriched canonical/KJV/labels/passages document | some entity/event head gains | suite R@3/10 falls to 83.17/90.79; meaning R@3 loses 7.1 points | reject |
| Sparse Study Note `CLEX/CPOST` | comprehensive 91.4% aligned note coverage; interpretable `context:study` evidence | no stable cutoff gain; adds about 3.92 MiB and independent context votes risk double counting | reject release fusion; keep diagnostic/build support |
| Dense Study-note second table | uses 28,405 source-authored notes through explicit LVEC-v3 roles | regresses multiple suites, including semantic, tip-of-the-tongue, confirmation, and red-team | reject |
| Dense rank summation | rewards agreement between vector views | suppresses strong candidates unique to one legitimate view | reject |
| Unrestricted all-lane RRF | conventional, easy to explain | old semantic regression fell 77.5/85/95 to 75/82.5/87.5; hard negatives fell to 93.8% | reverted |
| Guarded whole + clause composition | query-only and verse-independent; improves distinct-clause coverage monotonically | does not yet solve broad multi-intent recall | retain generic mechanism |

More context is not automatically better. Canonical verse text is a clean,
uniform target; concatenated commentary changes genre and can bury the verse's
central action. An auxiliary source must improve diverse families without
creating a second vote for the same semantic lane.

## Study Bible precomputed-vector alignment

The Study database contains 1,536D precomputed verse vectors. A
query-independent rectangular orthogonal Procrustes experiment aligned all
31,086 canonical coordinates from Arctic 256D query space into that corpus
space. It used no evaluation prompt or gold answer.

The source-only whole-book holdout passed its preregistered feasibility gate:
within-book R@1/3/10 **88.7/95.3/98.6** and a 4,096-candidate global sample
**70.1/79.1/86.7**. That was a coordinate-alignment result, not query
retrieval proof. Revealed final-fusion transfer regressed broadly:

| Revealed suite | Arctic-M 256D R@1/3/10 | Study/OpenAI 1536D R@1/3/10 |
|---|---:|---:|
| Semantic | 70.0 / 82.5 / 87.5 | 57.5 / 77.5 / 87.5 |
| Tip-of-the-tongue | 75.0 / 89.3 / 96.4 | 57.1 / 71.4 / 82.1 |
| Diagnostic | 57.1 / 67.9 / 78.6 | 46.4 / 64.3 / 78.6 |
| Confirmation | 82.1 / 96.4 / 100 | 57.1 / 64.3 / 92.9 |
| Revealed v2 | 65.2 / 73.9 / 78.3 | 43.5 / 56.5 / 78.3 |
| Family regression | 76.0 / 96.0 / 100 | 80.0 / 88.0 / 100 |
| Red-team | 58.7 / 74.6 / 90.5 | 42.9 / 57.1 / 82.5 |
| Overlay adversarial | 79.3 / 96.6 / 100 | 86.2 / 89.7 / 100 |

The full pack is 47,748,352 bytes plus a 1,572,880-byte query transform:
41,362,960 bytes (39.45 MiB) more than the compact dense pack and six times
its vector dimensions. It also carries an additional OpenAI-vector provenance and
distribution-review boundary. It is rejected despite strong source alignment.

A preregistered Study PCA 1,536D-to-256D variant failed before any prompt
reveal: its global source retrieval was only R@1/3/10 **35.0/46.7/58.4**. It
was not promoted, persisted as a release candidate, or tuned on prompts.

## Query-adapter experiments

| Candidate | Supervision and source gate | Prompt transfer | Portability/cost | Decision |
|---|---|---|---|---|
| Linear Procrustes/ridge `VRQADP01` | source-authored Study Note query vectors to canonical verses; source R@3 28.1->31.4, R@10 42.6->46.1 | semantic 62.5/72.5/82.5, diagnostic 42.9/53.6/75.0, red-team 39.7/57.1/85.7: all broad regressions versus Arctic | portable matrix; content-addressed; JS/Swift helpers | reject weights, retain format/tooling |
| Compact nonlinear residual adapter | whole-book Study-note supervision; aggregate R@3 35.4->36.5 and R@10 49.8->51.3 | no untouched prompt evidence; source-heldout partition also selected the candidate/epoch | 66,720-byte artifact; deterministic NumPy reference | reject release candidate; research only |

The nonlinear result is deliberately not promoted merely because it is small.
Its supervision is explanatory prose, not arbitrary user queries, and its
source gain is too weak to justify another production representation.

## Cross-encoder reranking

Generic MS MARCO rerankers were tested over Arctic top-50 candidates.
MiniLM-L6 RRF left pooled R@3 unchanged at 78.15% and raised R@10 only
2.52 points, while semantic hard-negative ordering fell from 93.75% to 87.5%.
At four CPU threads it cost 194.4 ms mean / 284.0 ms p95 per 50 candidates and
added 22.80 MiB. TinyBERT-L2 was faster (17.9 / 28.5 ms) and 4.99 MiB but
lowered pooled R@1/3/10. Three-verse MiniLM context was slower and did not
improve R@3. Both are rejected.

Reranking can reorder only retrieved candidates. The principal failures are
candidate and representation coverage, so paying substantial pairwise latency
does not repair the frontier.

## Portability, size, and latency trade

The retained engine data is 26.22 MiB. A host/service embedding deployment
needs no model in the client. A fully offline Arctic deployment is about
131.94 MiB for packs plus pinned quantized model/tokenizer. JavaScript/WASM,
Swift, Kotlin/JNI, C, and Rust all use the same packs and planner contract.

In the pre-v4 bake-off on one development Mac, 220 warmed single queries measured Arctic ONNX
tokenization/inference median **3.347 ms**, p95 **6.137 ms**. Persistent native
all-lane 256D search measured median **139.366 ms**, p95 **341.633 ms**. These
figures predate the current base/overlay identities; they are hardware-specific
historical scale and must not be presented as current-v4 or device/Web guarantees.

## Conclusion

The hybrid architecture is stronger than any traditional or dense-only method
because each source solves a different query family. Arctic-M 256D is the best
compact representation tested across revealed breadth, but neither it nor a
larger alternative has demonstrated generalized semantic superiority. The
release position is therefore modular and honest:

- ship lexical/recall profiles where size or encoder availability dominates;
- use the Arctic semantic profile where deeper low-overlap recall is worth the
  host-encoder cost;
- retain source-backed corrections and comprehensive graph/topic ingestion
  under conservative trust rules; and
- make no generalized semantic claim until a new sealed family-disjoint first
  shot passes every recall, safety, hard-negative, and label gate.
