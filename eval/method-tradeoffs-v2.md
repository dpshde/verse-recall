# Retrieval method trade-offs v2 — current release packs

This is a controlled ablation over eight already-revealed regression
suites with the **current** normalizer-v2 packs. It is product-regression
and model-selection evidence, **not** a sealed generalization claim.

**Retained candidate:** Guarded hybrid Arctic-M 256 + passage PVEC (`arctic_passage`).
**Majority win-rate:** 51/60
= **85.0%** (majority; ties excluded).

Win rule: on each comparable cell, the retained method strictly beats the
competitor on that metric. Size cells apply only when suite-macro R@3 is
within 1 absolute point; smaller engine-data wins.

## Overall matrix

| Method | Engine MiB | Suite-macro R@1 | R@3 | R@10 | Family R@3 | Family R@10 | Meaning R@3 | Multi-intent R@3 | HN | Safety | Role |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| Classic keyword overlap | 6.14 | 46.37% | 51.44% | 61.22% | 46.23% | 55.52% | 12.90% | 0.00% | 80.95% | n/a | traditional baseline |
| Indexed BM25 | 6.14 | 54.13% | 66.69% | 71.90% | 60.12% | 65.17% | 51.61% | 28.57% | 85.71% | 99.29% | traditional foundation |
| BM25 + expansion | 11.50 | 52.73% | 66.87% | 72.71% | 60.21% | 66.03% | 54.84% | 35.71% | 85.71% | 99.29% | traditional IR extension |
| Lexical + KJV/source overlay | 13.26 | 63.44% | 73.15% | 77.73% | 65.84% | 70.26% | 51.61% | 28.57% | 85.71% | 100.00% | traditional rules + graph |
| Encoder-free hybrid | 18.63 | 61.96% | 73.44% | 78.97% | 66.15% | 71.51% | 54.84% | 35.71% | 85.71% | 100.00% | hybrid without vectors |
| Dense Arctic-M 256 alone | 13.72 | 64.73% | 78.72% | 88.62% | 71.06% | 81.07% | 70.97% | 28.57% | 85.71% | 99.29% | vector alternative |
| Guarded hybrid Arctic-M 256 | 26.22 | 67.27% | 81.87% | 91.28% | 73.84% | 83.17% | 67.74% | 28.57% | 85.71% | 100.00% | compact release candidate |
| Guarded hybrid Arctic-M 256 + passage PVEC | 27.56 | 67.81% | 82.80% | 92.27% | 74.78% | 84.11% | 67.74% | 21.43% | 85.71% | 100.00% | retained release candidate |
| Guarded hybrid MiniLM-384 | 41.40 | 62.13% | 74.34% | 87.40% | 67.57% | 79.30% | 54.84% | 21.43% | 95.24% | 100.00% | encoder alternative |

## Head-to-head vs retained

| Competitor | Metric | Retained | Competitor | Winner |
|---|---|---:|---:|---|
| keyword | suite_macro_r1 | 67.81% | 46.37% | retained |
| keyword | suite_macro_r3 | 82.80% | 51.44% | retained |
| keyword | suite_macro_r10 | 92.27% | 61.22% | retained |
| keyword | family_cell_r3 | 74.78% | 46.23% | retained |
| keyword | family_cell_r10 | 84.11% | 55.52% | retained |
| keyword | meaning_only_r3 | 67.74% | 12.90% | retained |
| keyword | multi_intent_r3 | 21.43% | 0.00% | retained |
| keyword | hard_negative_rate | 85.71% | 80.95% | retained |
| lexical | suite_macro_r1 | 67.81% | 54.13% | retained |
| lexical | suite_macro_r3 | 82.80% | 66.69% | retained |
| lexical | suite_macro_r10 | 92.27% | 71.90% | retained |
| lexical | family_cell_r3 | 74.78% | 60.12% | retained |
| lexical | family_cell_r10 | 84.11% | 65.17% | retained |
| lexical | meaning_only_r3 | 67.74% | 51.61% | retained |
| lexical | multi_intent_r3 | 21.43% | 28.57% | competitor |
| lexical | hard_negative_rate | 85.71% | 85.71% | tie |
| lexical | confidence_safety | 100.00% | 99.29% | retained |
| expanded | suite_macro_r1 | 67.81% | 52.73% | retained |
| expanded | suite_macro_r3 | 82.80% | 66.87% | retained |
| expanded | suite_macro_r10 | 92.27% | 72.71% | retained |
| expanded | family_cell_r3 | 74.78% | 60.21% | retained |
| expanded | family_cell_r10 | 84.11% | 66.03% | retained |
| expanded | meaning_only_r3 | 67.74% | 54.84% | retained |
| expanded | multi_intent_r3 | 21.43% | 35.71% | competitor |
| expanded | hard_negative_rate | 85.71% | 85.71% | tie |
| expanded | confidence_safety | 100.00% | 99.29% | retained |
| knowledge | suite_macro_r1 | 67.81% | 63.44% | retained |
| knowledge | suite_macro_r3 | 82.80% | 73.15% | retained |
| knowledge | suite_macro_r10 | 92.27% | 77.73% | retained |
| knowledge | family_cell_r3 | 74.78% | 65.84% | retained |
| knowledge | family_cell_r10 | 84.11% | 70.26% | retained |
| knowledge | meaning_only_r3 | 67.74% | 51.61% | retained |
| knowledge | multi_intent_r3 | 21.43% | 28.57% | competitor |
| knowledge | hard_negative_rate | 85.71% | 85.71% | tie |
| knowledge | confidence_safety | 100.00% | 100.00% | tie |
| hybrid_no_dense | suite_macro_r1 | 67.81% | 61.96% | retained |
| hybrid_no_dense | suite_macro_r3 | 82.80% | 73.44% | retained |
| hybrid_no_dense | suite_macro_r10 | 92.27% | 78.97% | retained |
| hybrid_no_dense | family_cell_r3 | 74.78% | 66.15% | retained |
| hybrid_no_dense | family_cell_r10 | 84.11% | 71.51% | retained |
| hybrid_no_dense | meaning_only_r3 | 67.74% | 54.84% | retained |
| hybrid_no_dense | multi_intent_r3 | 21.43% | 35.71% | competitor |
| hybrid_no_dense | hard_negative_rate | 85.71% | 85.71% | tie |
| hybrid_no_dense | confidence_safety | 100.00% | 100.00% | tie |
| dense_only | suite_macro_r1 | 67.81% | 64.73% | retained |
| dense_only | suite_macro_r3 | 82.80% | 78.72% | retained |
| dense_only | suite_macro_r10 | 92.27% | 88.62% | retained |
| dense_only | family_cell_r3 | 74.78% | 71.06% | retained |
| dense_only | family_cell_r10 | 84.11% | 81.07% | retained |
| dense_only | meaning_only_r3 | 67.74% | 70.97% | competitor |
| dense_only | multi_intent_r3 | 21.43% | 28.57% | competitor |
| dense_only | hard_negative_rate | 85.71% | 85.71% | tie |
| dense_only | confidence_safety | 100.00% | 99.29% | retained |
| arctic_full | suite_macro_r1 | 67.81% | 67.27% | retained |
| arctic_full | suite_macro_r3 | 82.80% | 81.87% | retained |
| arctic_full | suite_macro_r10 | 92.27% | 91.28% | retained |
| arctic_full | family_cell_r3 | 74.78% | 73.84% | retained |
| arctic_full | family_cell_r10 | 84.11% | 83.17% | retained |
| arctic_full | meaning_only_r3 | 67.74% | 67.74% | tie |
| arctic_full | multi_intent_r3 | 21.43% | 28.57% | competitor |
| arctic_full | hard_negative_rate | 85.71% | 85.71% | tie |
| arctic_full | confidence_safety | 100.00% | 100.00% | tie |
| arctic_full | engine_data_size_near_quality | 27.56 MiB | 26.22 MiB | competitor |
| minilm_full | suite_macro_r1 | 67.81% | 62.13% | retained |
| minilm_full | suite_macro_r3 | 82.80% | 74.34% | retained |
| minilm_full | suite_macro_r10 | 92.27% | 87.40% | retained |
| minilm_full | family_cell_r3 | 74.78% | 67.57% | retained |
| minilm_full | family_cell_r10 | 84.11% | 79.30% | retained |
| minilm_full | meaning_only_r3 | 67.74% | 54.84% | retained |
| minilm_full | multi_intent_r3 | 21.43% | 21.43% | tie |
| minilm_full | hard_negative_rate | 85.71% | 95.24% | competitor |
| minilm_full | confidence_safety | 100.00% | 100.00% | tie |

## Per-suite R@3 (retained profile)

| Suite | R@1 | R@3 | R@10 | Safety |
|---|---:|---:|---:|---:|
| Semantic quality v2 | 70.00% | 77.50% | 90.00% | 100.00% |
| Tip of the tongue | 75.00% | 85.71% | 96.43% | 100.00% |
| Overlay adversarial | 75.86% | 89.66% | 96.55% | 100.00% |
| Overfit diagnostic | 57.14% | 75.00% | 85.71% | 100.00% |
| Overfit confirmation | 82.14% | 96.43% | 100.00% | 100.00% |
| Revealed holdout v2 | 65.22% | 73.91% | 86.96% | 100.00% |
| Family regression | 60.00% | 88.00% | 92.00% | 100.00% |
| Red-team prompts | 57.14% | 76.19% | 90.48% | 100.00% |

## Strengths and weaknesses

| Method | Strengths | Weaknesses |
|---|---|---|
| Classic keyword overlap | Transparent exact-token baseline. | No phrase model, translation bridge, or meaning-only recall. |
| Indexed BM25 | Compact deterministic near-quote recall. | Fails zero-overlap paraphrases. |
| BM25 + expansion | Portable vocabulary drift correction. | Synonym noise; no narrative grounding. |
| Lexical + KJV/source overlay | Translation, events, entities, corrections without encoder. | Sparse knowledge is not semantic similarity. |
| Encoder-free hybrid | Strong offline quotes/translation/events without encoder cost. | Shallow meaning-only tail. |
| Dense Arctic-M 256 alone | Generic low-overlap candidates. | Loses translation, graph, and phrase advantages alone. |
| Guarded hybrid Arctic-M 256 | Complementary quote, translation, event, and semantic coverage. | Larger; host encoder required; multi-intent still incomplete. |
| Guarded hybrid Arctic-M 256 + passage PVEC | Same hybrid plus range-bound passage vectors; cannot mint Recovered without LVEC support. | Adds ~1.34 MiB; passage table is research-grade source coverage, not a sealed-gate claim. |
| Guarded hybrid MiniLM-384 | Smaller encoder footprint than Arctic-M. | Weaker suite-macro transfer than Arctic-M 256 on revealed suites. |

## Decision

Retain **Guarded hybrid Arctic-M 256 + passage PVEC** as the compact semantic release profile.
It wins **51** of **60**
strict head-to-head cells (ties excluded) against traditional profiles
and the MiniLM hybrid alternative under the same packs and depth.
Suite-macro R@3 remains within the compatibility floor of the prior
Arctic-M 256 documented baseline (84.20% ± 2 points) when measured
under this harness's suite weighting.

This does **not** claim generalized semantic quality. Sealed
family-disjoint holdout evidence remains the release gate.

## Reproduction

- Embedding space (Arctic): `36db5d816aeac4c6f6631619d587d7caea5f829fcddfff51450d60823e58d744`
- Evaluator: `/Users/user/Developer/verse-recall/target/release/recall-eval` sha256 `0b9972d3a9407cb1da0683bb0bd055f45a5767cf1e9fecb2d0c69de1eee32d42`
- Artifact manifest: see JSON `artifacts`
- Command: `python3 scripts/evaluate_method_tradeoffs_v2.py`
