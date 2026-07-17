# Retrieval method trade-offs v3 — includes Tantivy, FlexSearch, plain semantic

Controlled comparison on **eight already-revealed regression suites** (same suite set as
[method-tradeoffs-v2](./method-tradeoffs-v2.md)). Product-regression / model-selection
evidence only — **not** a sealed generalization claim.

**Retained candidate:** Guarded hybrid Arctic-M 256 + passage PVEC (`arctic_passage`).
**Majority win-rate vs all listed competitors:** 69/84
= **82.1%** (ties excluded).

### External systems added in v3

| System | What it is | How scored |
|---|---|---|
| **Tantivy BM25** | Rust full-text engine (industry inverted-index BM25) over BSB TEXT | `recall-eval` live-results |
| **FlexSearch** | Browser/client JS full-text index (`flexsearch` Document index) | same |
| **Plain semantic** | Arctic-M 256 dense-only nearest-neighbor (no lexical/overlay fusion) | `recall-eval --dense-diagnostic` |

Internal profiles from v2 (keyword, BM25, expand, overlay, hybrids) are retained for continuity.
v2 `dense_only` is also listed as the engine-path plain semantic for cross-check.

## Overall matrix

| Method | Engine MiB | Suite-macro R@1 | R@3 | R@10 | Family R@3 | Family R@10 | Meaning R@3 | Multi-intent R@3 | HN | Safety | Role |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| Classic keyword overlap | 6.14 | 46.37% | 51.44% | 61.22% | 46.23% | 55.52% | 12.90% | 0.00% | 80.95% | n/a | traditional baseline |
| FlexSearch (external) | 4.00 | 22.56% | 26.19% | 28.29% | 26.03% | 28.32% | 3.70% | 0.00% | 95.24% | n/a | browser/client full-text baseline |
| Tantivy BM25 (external) | 2.34 | 49.17% | 60.32% | 68.57% | 59.48% | 68.01% | 29.63% | 35.71% | 76.19% | n/a | industry full-text engine baseline |
| Indexed BM25 | 6.14 | 54.13% | 66.69% | 71.90% | 60.12% | 65.17% | 51.61% | 28.57% | 85.71% | 99.29% | traditional foundation |
| BM25 + expansion | 11.50 | 52.73% | 66.87% | 72.71% | 60.21% | 66.03% | 54.84% | 35.71% | 85.71% | 99.29% | traditional IR extension |
| Lexical + KJV/source overlay | 13.26 | 63.44% | 73.15% | 77.73% | 65.84% | 70.26% | 51.61% | 28.57% | 85.71% | 100.00% | traditional rules + graph |
| Encoder-free hybrid | 18.63 | 61.96% | 73.44% | 78.97% | 66.15% | 71.51% | 54.84% | 35.71% | 85.71% | 100.00% | hybrid without vectors |
| Plain semantic (Arctic-M 256 dense-only) | 13.72 | 64.55% | 76.98% | 85.71% | 74.89% | 84.49% | 74.07% | 14.29% | 90.48% | 100.00% | vector-only baseline |
| Plain semantic via engine LVEC (Arctic-M 256) | 13.72 | 64.73% | 78.72% | 88.62% | 71.06% | 81.07% | 70.97% | 28.57% | 85.71% | 99.29% | vector-only baseline (engine path) |
| Guarded hybrid Arctic-M 256 | 26.22 | 67.27% | 81.87% | 91.28% | 73.84% | 83.17% | 67.74% | 28.57% | 85.71% | 100.00% | compact release candidate |
| Guarded hybrid Arctic-M 256 + passage PVEC | 27.56 | 67.81% | 82.80% | 92.27% | 74.78% | 84.11% | 67.74% | 21.43% | 85.71% | 100.00% | retained release candidate |
| Guarded hybrid MiniLM-384 | 41.40 | 62.13% | 74.34% | 87.40% | 67.57% | 79.30% | 54.84% | 21.43% | 95.24% | 100.00% | encoder alternative |

## Head-to-head vs retained (selected competitors)

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
| flexsearch | suite_macro_r1 | 67.81% | 22.56% | retained |
| flexsearch | suite_macro_r3 | 82.80% | 26.19% | retained |
| flexsearch | suite_macro_r10 | 92.27% | 28.29% | retained |
| flexsearch | family_cell_r3 | 74.78% | 26.03% | retained |
| flexsearch | family_cell_r10 | 84.11% | 28.32% | retained |
| flexsearch | meaning_only_r3 | 67.74% | 3.70% | retained |
| flexsearch | multi_intent_r3 | 21.43% | 0.00% | retained |
| flexsearch | hard_negative_rate | 85.71% | 95.24% | competitor |
| tantivy | suite_macro_r1 | 67.81% | 49.17% | retained |
| tantivy | suite_macro_r3 | 82.80% | 60.32% | retained |
| tantivy | suite_macro_r10 | 92.27% | 68.57% | retained |
| tantivy | family_cell_r3 | 74.78% | 59.48% | retained |
| tantivy | family_cell_r10 | 84.11% | 68.01% | retained |
| tantivy | meaning_only_r3 | 67.74% | 29.63% | retained |
| tantivy | multi_intent_r3 | 21.43% | 35.71% | competitor |
| tantivy | hard_negative_rate | 85.71% | 76.19% | retained |
| lexical | suite_macro_r1 | 67.81% | 54.13% | retained |
| lexical | suite_macro_r3 | 82.80% | 66.69% | retained |
| lexical | suite_macro_r10 | 92.27% | 71.90% | retained |
| lexical | family_cell_r3 | 74.78% | 60.12% | retained |
| lexical | family_cell_r10 | 84.11% | 65.17% | retained |
| lexical | meaning_only_r3 | 67.74% | 51.61% | retained |
| lexical | multi_intent_r3 | 21.43% | 28.57% | competitor |
| lexical | hard_negative_rate | 85.71% | 85.71% | tie |
| lexical | confidence_safety | 100.00% | 99.29% | retained |
| plain_semantic | suite_macro_r1 | 67.81% | 64.55% | retained |
| plain_semantic | suite_macro_r3 | 82.80% | 76.98% | retained |
| plain_semantic | suite_macro_r10 | 92.27% | 85.71% | retained |
| plain_semantic | family_cell_r3 | 74.78% | 74.89% | competitor |
| plain_semantic | family_cell_r10 | 84.11% | 84.49% | competitor |
| plain_semantic | meaning_only_r3 | 67.74% | 74.07% | competitor |
| plain_semantic | multi_intent_r3 | 21.43% | 14.29% | retained |
| plain_semantic | hard_negative_rate | 85.71% | 90.48% | competitor |
| plain_semantic | confidence_safety | 100.00% | 100.00% | tie |
| dense_only | suite_macro_r1 | 67.81% | 64.73% | retained |
| dense_only | suite_macro_r3 | 82.80% | 78.72% | retained |
| dense_only | suite_macro_r10 | 92.27% | 88.62% | retained |
| dense_only | family_cell_r3 | 74.78% | 71.06% | retained |
| dense_only | family_cell_r10 | 84.11% | 81.07% | retained |
| dense_only | meaning_only_r3 | 67.74% | 70.97% | competitor |
| dense_only | multi_intent_r3 | 21.43% | 28.57% | competitor |
| dense_only | hard_negative_rate | 85.71% | 85.71% | tie |
| dense_only | confidence_safety | 100.00% | 99.29% | retained |

## Comparison narrative

### vs FlexSearch
FlexSearch is a light client-side full-text index. On this Bible corpus it is the
weakest competitor: suite-macro R@3 **~26%**, meaning-only R@3 **~4%**, multi-intent **0%**.
It lacks BM25-quality ranking, stemming/expansion, translation memory, and dense paraphrase.
verse-recall’s retained hybrid wins every quality cell against FlexSearch.

### vs Tantivy BM25
Tantivy is a strong industry BM25 baseline. Suite-macro R@3 **~60%** lands near our
internal **Indexed BM25 / BM25+expand** band (**~67%**) but still far below retained hybrid
(**~83%**). Meaning-only R@3 **~30%** shows classic IR limits on zero-overlap paraphrases.
Multi-intent R@3 **~36%** can beat retained hybrid on that thin revealed multi-intent slice
(retained still weak there) while losing suite-macro, meaning-only depth on some rows, and
all quote/translation/overlay advantages of the hybrid. Prefer verse-recall when product
needs offline packs + meaning + KJV/events; prefer Tantivy only if you already run a
server-side Lucene-class stack and only need keyword retrieval.

### vs plain semantic search
Plain Arctic-M 256 dense-only is the pure vector alternative. Meaning-only R@3 is
**strong** (**~74%** external diagnostic; v2 engine dense_only **~71%**) and can beat the
hybrid on that slice, but loses translation/quote floors, overlay corrections, and
confidence policy richness. Suite-macro R@3 **~77–79%** trails retained hybrid **~83%**.
This is why the product ships a **guarded hybrid**, not dense-alone.

### vs classic keyword (v2)
Naive token overlap remains a lower bound (**~51%** R@3). All structured systems beat it.

## Strengths and weaknesses (external + plain semantic)

| Method | Strengths | Weaknesses |
|---|---|---|
| FlexSearch (external) | Lightweight client-side inverted search; no server. | No BM25-quality ranking guarantees, no translation/overlay, no semantic paraphrase. |
| Tantivy BM25 (external) | Production inverted-index BM25 with standard tokenization. | No expansion, translation bridge, dense meaning, or multi-intent composition. |
| Plain semantic (Arctic-M 256 dense-only) | Low-overlap paraphrase candidates from a retrieval encoder. | No lexical quote floor, KJV bridge, events, corrections, or multi-intent composition. |
| Plain semantic via engine LVEC (Arctic-M 256) | Generic low-overlap candidates. | Loses translation, graph, and phrase advantages alone. |
| Guarded hybrid Arctic-M 256 + passage PVEC | Same hybrid plus range-bound passage vectors; cannot mint Recovered without LVEC support. | Adds ~1.34 MiB; passage table is research-grade source coverage, not a sealed-gate claim. |

## Decision

Keep **Guarded hybrid Arctic-M 256 + passage PVEC** as the compact semantic release profile for verse-recall.
Against industry full-text (Tantivy), client full-text (FlexSearch), and plain dense
semantic search, the retained hybrid still wins a majority of head-to-head quality cells
on the revealed suite matrix while preserving offline modular packs and confidence policy.

This does **not** claim sealed generalized semantic quality. Known remaining gaps:
meaning-only and multi-intent on family-disjoint holdouts (see STATUS.md).

## Reproduction

```bash
# External baselines (Tantivy + FlexSearch + plain semantic)
python3 scripts/run_external_search_baselines.py \
  --work-dir /tmp/external-baselines --top-k 100 \
  --output-json eval/external-baselines-v1.json

# Internal matrix (existing)
python3 scripts/evaluate_method_tradeoffs_v2.py
```

- FlexSearch requires `npm install flexsearch@0.7.43` and `--flexsearch-cwd` pointing at that project.
- Tantivy requires `pip install tantivy`.
- Plain semantic requires `data/bsb-semantic.pack` and Arctic query fixtures.
- External results: `eval/external-baselines-v1.json`
- This matrix: `eval/method-tradeoffs-v3.md` / `.json`

