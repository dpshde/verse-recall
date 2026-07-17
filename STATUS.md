# Current status (finished baseline)

Date: 2026-07-16

This file is the short source of truth for what is **shipped**, what is
**optional research**, and what is **not claimed**. Longer history lives under
`eval/` and `ARTIFACTS.md`.

## Verdict

| Claim | Status |
|---|---|
| Portable offline engine (Rust / C / Swift / Kotlin / WASM) | **Shipped** |
| Lexical + expansion + KJV + source overlay + safety policy | **Shipped** |
| Compact semantic Lane C (Arctic-M 256D, one vector per verse) | **Shipped** as best compact baseline |
| Query-only clause planner + multi-intent composition | **Shipped** as a generic mechanism |
| Generalized semantic quality (sealed family-disjoint gate) | **Not resolved** |
| Prompt-specific / gold-derived runtime aliases | **Forbidden** (`AGENTS.md`) |

Release gates for a future sealed holdout remain R@3 ≥ 85%, R@10 ≥ 95%, 100%
confidence safety, 100% hard-negative ordering, and 100% required labels, with
no material quote / translation / event / size / portability regression.

## What you run in production

Four modular packs (26.22 MiB engine data):

| Pack | Role |
|---|---|
| `data/bsb-full.pack` | BSB text + BM25 |
| `data/bsb-expand.pack` | Weighted expansion graph |
| `data/recall-overlay.pack` | KJV, events, topics, graph, corrections |
| `data/bsb-semantic.pack` | Arctic-M 256D int8 verse vectors |

Validate with:

```bash
python3 scripts/verify_release_artifacts.py
cargo test --workspace
python3 -m unittest discover -s tests -q
```

Exact hashes, licensing, and platform matrix: [`ARTIFACTS.md`](./ARTIFACTS.md).

## Quality evidence (honest)

- **Last valid sealed holdout** (v2): R@1/3/10 **39.1 / 56.5 / 78.3** —
  failed the release gate. Immutable report:
  `eval/overfit-holdout-v2-first-shot-report.md`.
- **Method trade-off matrix v2** (revealed suites only; not a sealed gate):
  retained **Arctic-M 256 hybrid + optional passage PVEC** wins
  **51/60 (85%)** strict head-to-head cells vs traditional profiles and
  MiniLM. Suite-macro R@1/3/10 **67.8 / 82.8 / 92.3**, confidence safety
  **100%**. See `eval/method-tradeoffs-v2.md` (internal) and
  `eval/method-tradeoffs-v3.md` (adds Tantivy, FlexSearch, plain semantic).
- Historical encoder bake-off suite-macro R@3 **84.20%** remains the
  documentation reference for the ±2-point compatibility band; v2 measures
  current normalizer-v2 packs.
- Contaminated or post-tuning “holdouts” are quarantined in
  `eval/holdout-contamination-ledger.md` and must not support release language.

## Modular profiles (deliberate size/quality split)

| Profile | Packs | Approx. size | When to use |
|---|---|---:|---|
| Lexical | base + expand | 11.50 MiB | no encoder available |
| Recall | + overlay | 18.63 MiB | quotes, KJV, events offline |
| Semantic (verse LVEC) | + `bsb-semantic.pack` | 26.22 MiB | host encoder available |
| Semantic + passage | + optional `bsb-passage-semantic.pack` | ~27.56 MiB | matrix-retained depth; still not a sealed claim |

## Optional / research paths

| Mechanism | Why it exists | Release status |
|---|---|---|
| `PVEC` passage-dense pack | Range-bound passage vectors; cannot mint `Recovered` without LVEC support | Optional fifth pack; retained in trade-off matrix v2 |
| Source semantic document builder | Typed Study/graph documents for future multi-vector work | Build-time only |
| Enriched / window / Study-note dense tables | Bake-off candidates | Rejected for compact verse-only LVEC |
| Query adapters (`VRQADP01`) | Portable format for host-side adapters | Format supported; no adapter ships |
| Cross-encoder / extra encoder bake-offs | Candidate selection evidence | Documented under `eval/` |

## Policy

- Never hardcode benchmark answers: [`AGENTS.md`](./AGENTS.md).
- Design and trust hierarchy: [`DESIGN.md`](./DESIGN.md).
- Eval contract: [`eval/README.md`](./eval/README.md).
- Tooling map: [`scripts/README.md`](./scripts/README.md).

## Next legitimate work (when resuming quality)

1. Corpus-wide representation improvements (encoder, multi-vector, typed source
   documents) measured on **already revealed** suites.
2. Keep quote, translation, event, safety, size, and latency floors.
3. Freeze a **new** family-disjoint holdout only after a candidate is selected.
4. Reveal once; retire to regression; do not patch its misses.

Until that sealed pass lands, product language must stay: lexical/source
integrity is strong; **generalized meaning-only and multi-intent recall are
not proven resolved**.
