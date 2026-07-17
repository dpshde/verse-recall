# Repository instructions

These instructions apply to every change in this repository. Search quality
must improve through mechanisms that generalize to future queries, not through
rules fitted to examples we have already seen.

## Core rule: do not hardcode benchmark answers

Never add production behavior that names, recognizes, rewrites, boosts, or
special-cases an evaluation prompt, prompt fragment, gold reference, family,
or failure discovered by red-teaming.

Prohibited examples include:

- query-to-verse dictionaries derived from failed evaluations;
- `if`/`match` branches for particular wording, people, events, references, or
  benchmark IDs;
- adding a failed prompt or a lightly edited version of it as a runtime alias;
- tuning weights, thresholds, lane order, or fixed result positions until one
  revealed suite turns green without independent evidence;
- copying gold references into expansion, concept, phrase, misconception, or
  topic data after seeing the evaluation result;
- shipping a small hand-selected sample while describing it as broad semantic,
  entity, event, translation, or misconception coverage;
- allowing test fixtures, demos, or evaluation files to become runtime data.

A solution is not generalized merely because the hardcoding lives in JSON,
SQLite, generated code, a prompt template, or a model prefix instead of Rust.
Judge it by where the information came from and whether it applies uniformly
to unseen queries.

## What to build instead

Prefer query-independent, corpus-wide mechanisms:

- better retrieval-trained encoders or multi-vector representations;
- comprehensive translation-aligned corpora with clear licensing;
- deterministic indexing, normalization, fuzzy matching, and calibrated
  rank-fusion algorithms;
- generic clause decomposition and result-set composition for multi-intent
  queries;
- complete entity/event/topic ingestion from identified source datasets;
- separately reviewed misconception evidence with provenance and explicit
  correction semantics;
- confidence calibration based on evidence type, selectivity, and agreement;
- build-time pipelines whose coverage and exclusions are measurable.

Curated knowledge is allowed only when it is maintained as a source dataset,
has provenance and a documented review policy, is not copied from a revealed
gold answer, and is ingested through the same generic path as every other
record. Product acceptance examples may be regression tests, but may not
justify code paths that exist only for those examples.

## Required evaluation discipline

Before selecting or tuning a search-quality candidate:

1. Freeze a family-disjoint `holdout-*` batch covering several query families.
2. Verify that no gold passage, overlapping passage, normalized query, or
   family appears in development or regression data.
3. Tune only on development and already revealed `regression-*` suites.
4. Reveal the holdout exactly once with all existing corpora supplied as family
   registries.
5. Save the immutable first-shot Markdown and JSON reports.
6. Immediately rename the revealed split to `regression-*`.
7. Do not patch its misses. Freeze another disjoint holdout before evaluating
   a subsequent candidate.

Treat a 100% result as a red-team trigger, not proof of completion. Add a new
disjoint batch with rarer passages, low-overlap paraphrases, translation drift,
entities, events, multi-intent memories, hard negatives, and false details.

Report per-family metrics, not only an aggregate. At minimum retain R@1, R@3,
R@10, MRR, confidence safety, hard-negative ordering, required correction
labels, structured-passage accuracy, artifact size, and latency. A large easy
slice must not conceal failure on semantic or multi-intent queries.

## Acceptance and stop rules

Do not claim generalized semantic quality until a sealed family-disjoint batch
meets all current release thresholds: R@3 >= 85%, R@10 >= 95%, 100% confidence
safety, 100% hard-negative ordering, and 100% required labels, without material
regression to exact quotes, translation recall, events, portability, latency,
or artifact size.

Revert or quarantine a candidate when it:

- improves a targeted suite but harms broader family-weighted evaluation;
- improves depth by making head ranking or safety materially worse;
- depends on a representation whose benefit disappears on a disjoint batch;
- cannot be explained as a uniform mechanism for arbitrary future queries.

When the evidence does not support a fix, leave the miss visible and document
the limitation. An honest unresolved result is preferable to a green benchmark
created by leakage.

## Change checklist

Every search-quality handoff must state:

- the general mechanism changed and why it should transfer;
- the data provenance and coverage, if data changed;
- which prompts and families were available during development;
- the untouched holdout used, or why no generalization claim is being made;
- regression, safety, size, and latency results;
- rejected experiments and remaining failures;
- confirmation that no revealed prompt or gold answer was added to production
  code or data.

The overfitting audit in `eval/overfit-audit-v1.md` is the governing example:
several plausible changes were reverted because they failed family-disjoint
evaluation. Preserve that standard.
