# Evaluation

Product status: [`../STATUS.md`](../STATUS.md).  
Policy: [`../AGENTS.md`](../AGENTS.md) (no eval hardcoding).

## Layout

```text
eval/
  *.jsonl                 Active regression / revealed suite prompts
  prompt.schema.json      Prompt schema
  semantic-release-profile-v1.json
  ci-compatibility-*      CI query fixture (revealed only)
  overfit-audit-v1.md     Governing overfitting narrative
  generalization-resolution-v3.md
  method-tradeoffs-v2.*   Internal pack/profile ablation matrix
  method-tradeoffs-v3.*   v2 + Tantivy / FlexSearch / plain semantic
  external-baselines-v1.* Live-results baselines for external engines
  holdout-contamination-ledger.md
  *-first-shot-report.*   Immutable sealed/regression first shots
  contaminated/           Quarantined invalid “holdouts”
  archive/reports/        Historical bake-offs and old reports
```

## Active suites (revealed / regression)

| File | Role |
|---|---|
| `tot-prompts.jsonl` | Tip-of-the-tongue regression |
| `semantic-quality-v2.jsonl` | Semantic product regression |
| `overlay-adversarial-v1.jsonl` | Overlay / safety regression |
| `family-regression-v1.jsonl` | Family regression |
| `family-regression-v6.jsonl` | Harder family regression (retired holdout) |
| `redteam-prompts.jsonl` | Broad red-team |
| `overfit-diagnostic-v1.jsonl` | Overfit diagnostic (revealed) |
| `overfit-confirmation-v1.jsonl` | Overfit confirmation (revealed) |
| `overfit-holdout-v2.jsonl` | Retired sealed holdout → regression |
| `semantic-mutation-frontier-v1.jsonl` | Small mutation frontier |
| `sample-verses.jsonl` | Tiny sample |

## Governing narrative (read first)

| Doc | Why |
|---|---|
| `overfit-audit-v1.md` | Why green suites ≠ generalization |
| `generalization-resolution-v3.md` | Current quality story |
| `method-tradeoffs-v2.md` | Internal pack/profile ablation matrix |
| `method-tradeoffs-v3.md` | Same suites + Tantivy, FlexSearch, plain semantic |
| `external-baselines-v1.json` | Raw external baseline aggregates |
| `holdout-contamination-ledger.md` | What must never support claims |
| `resolution-evidence-v2.md` | Corrected status trail |

## Family-held-out workflow

1. Tune only on `dev` and already-retired `regression-*` families.
2. Keep a sealed `holdout-*` batch closed while changing ranking or packs.
3. Reveal once with full family registries; save immutable first-shot reports.
4. Retire with `scripts/retire_holdout.py` → `regression-*`.
5. Never patch misses into runtime aliases or gold-derived rules.

Release-gate shape (hosts must supply dense query vectors):

```bash
cargo run -p recall-eval --release -- \
  --pack data/bsb-full.pack \
  --overlay data/bsb-expand.pack \
  --overlay data/recall-overlay.pack \
  --overlay data/bsb-semantic.pack \
  --prompts path/to/holdout.jsonl \
  --split holdout-N \
  --family-registry eval/semantic-quality-v2.jsonl \
  --query-embeddings path/to/holdout-query.jsonl \
  --embedding-space-id 36db5d816aeac4c6f6631619d587d7caea5f829fcddfff51450d60823e58d744 \
  --release-profile eval/semantic-release-profile-v1.json \
  --release-gate
```

Compatibility gate (revealed suites only) is a floor, not a release claim.
See CI and `eval/ci-compatibility-arctic-m256-query.manifest.json`.

## Archive and contaminated

- **`contaminated/`** — explicitly invalid for generalization claims.
- **`archive/reports/`** — old bake-offs and superseded reports kept for audit.

Do not promote archive scores as current product evidence without re-running
on the present packs and embedding-space contract.
