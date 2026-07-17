# Holdout contamination ledger

This ledger records split status without reproducing prompt text, gold
references, families, scores, or other holdout contents. A listed file is
permanently ineligible for first-shot or generalization claims.

| File | Status | Reason | Permitted use |
|---|---|---|---|
| `contaminated/overfit-holdout-v3-contaminated.jsonl` | quarantined | contents became available during development before a valid frozen-candidate reveal | regression only after an explicit split retirement; never holdout evidence |
| `contaminated/overfit-holdout-v4-contaminated.jsonl` | quarantined | contents became available during development before a valid frozen-candidate reveal | regression only after an explicit split retirement; never holdout evidence |
| `contaminated/overfit-holdout-v5-contaminated.jsonl` | quarantined | an output-selection/glob failure exposed the file before candidate freeze | regression only after an explicit split retirement; never holdout evidence |

## Rules

1. Do not inspect, embed, score, summarize, quote, rename back to `holdout`, or
   use any listed file to select a model, representation, adapter, fusion rule,
   threshold, alias, or source record.
2. Do not report a checksum or aggregate metric from a quarantined file as if
   it establishes non-leakage. Once contents were exposed, a hash cannot make
   the split blind again.
3. If a quarantined file is ever retained as regression, its role must be
   explicit and it must never be included in a claimed first-shot aggregate.
4. A successor holdout is not named or described here while sealed. Only its
   independent curator may report non-content metadata needed to establish
   existence and integrity before candidate freeze.
5. Any accidental display, log output, browser rendering, query embedding,
   score, or evaluator read immediately contaminates that successor. Rename it
   accordingly, add one row here without contents, and freeze a new disjoint
   batch.

This ledger supplements [AGENTS.md](../AGENTS.md) and the split discipline in
[`eval/README.md`](./README.md). It is an audit control, not an evaluation
artifact.
