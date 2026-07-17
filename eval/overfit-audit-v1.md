# Overfitting audit v1

This audit treats a perfect score as a trigger for a new family-disjoint test,
not as proof that semantic search is complete. No prompt or gold reference from
the new batches was added to a runtime phrase, concept, expansion, or ranking
rule.

## Blind reveals

| Batch / candidate | n (gold) | R@1 | R@3 | R@10 | Safety | Release gate |
|---|---:|---:|---:|---:|---:|---|
| Diagnostic v1, pre-change production engine | 28 | 42.9% | 60.7% | 64.3% | 100% | Fail |
| Confirmation v1, generic all-lane RRF candidate | 28 | 71.4% | 85.7% | 89.3% | 100% | Fail (R@10) |
| Holdout v2, final retained engine | 23 | 39.1% | 56.5% | 78.3% | 100% | Fail |
| Holdout v2, offline keyword baseline | 23 | 43.5% | 52.2% | 56.5% | n/a | n/a |

The final holdout is the strongest evidence because it was frozen after the
first two batches were revealed and before the final candidate was rebuilt. Its
full immutable result is in
[`overfit-holdout-v2-first-shot-report.md`](./overfit-holdout-v2-first-shot-report.md).

## What the holdout exposed

| Query family | Final R@1 | Final R@3 | Final R@10 | Finding |
|---|---:|---:|---:|---|
| Rare phrase | 100% | 100% | 100% | Canonical partial-phrase recall generalizes on this slice. |
| Translation drift | 66.7% | 66.7% | 100% | Hidden KJV improves depth, but dense mode can still lower the exact alternate phrase at the head. |
| Meaning-only | 12.5% | 37.5% | 62.5% | The current BGE-small verse/window representation is not a sufficient generalized semantic retriever. |
| Event descriptions | 50% | 100% | 100% | Descriptive event recall is broad when the query retains several narrative entities/actions. |
| Entity + event | 66.7% | 66.7% | 100% | Coverage exists, but top-three ranking is uneven. |
| Multi-intent | 0% | 0% | 50% | A single whole-query embedding and lexical split do not reliably return every remembered passage. |
| Previously unseen false detail | 0% | 0% | 0% | Safety stayed intact, but correction labels require a reviewed misconception evidence source. |

## Rejected experiments

1. **All-lane RRF in semantic mode.** It removed the fixed semantic/lexical
   interleave, but regressed the revealed 43-query semantic suite from
   77.5/85/95 to 75/82.5/87.5 and reduced hard-negative ordering to 93.8%.
   It was reverted.
2. **Comprehensive enriched dense documents.** A deterministic 31,086-document
   corpus combined BSB, hidden KJV, Study Bible paragraph context, and trusted
   event/entity/place labels. It raised diagnostic R@10 only from 64.3% to
   67.9% while lowering R@3 in one fusion experiment. The builder and tests are
   retained for controlled encoder comparisons; the generated table is not a
   release artifact.
3. **BGE retrieval instruction prefix.** Using the model-card retrieval prefix
   changed the embedding-space identity but did not change the diagnostic
   result. It was not shipped.
4. **Multi-view rank summation.** Rewarding occurrence in both dense tables
   suppressed strong candidates unique to one valid representation and
   regressed the semantic suite. Best-view rank fusion was retained.

These reversions are intentional evidence: a broadly described mechanism is
not a generalized improvement unless it survives family-disjoint evaluation.

## Regression and integrity floors after the audit

The retained engine still clears all declared gates on the already-revealed
regression suites:

| Regression suite | R@1 | R@3 | R@10 | Confidence | Hard negatives | Gate |
|---|---:|---:|---:|---:|---:|---|
| Semantic quality v2 | 77.5% | 85.0% | 95.0% | 100% | 100% | Pass |
| Tip-of-the-tongue | 82.1% | 96.4% | 100% | 100% | n/a | Pass |
| Overlay adversarial | 93.1% | 96.6% | 100% | 100% | 100% | Pass |

These are compatibility floors, not generalization evidence. Reproducible
reports are `post-overfit-*-regression-report.md` in this directory.

## Correct course

The unresolved problem is representation/candidate generation, not a missing
list of Bible aliases. The next release candidate should therefore:

1. compare multiple retrieval-trained encoders and late-interaction or
   multi-vector representations on the now-revealed regression pools;
2. keep near-quote, translation, event, safety, hard-negative, latency, and
   artifact-size floors while optimizing equal-weight family recall;
3. accept clause-level embeddings from the host for generic multi-intent
   decomposition instead of verse-specific rewrites;
4. add a separately reviewed misconception corpus with provenance and explicit
   correction labels, never inferred from topical similarity;
5. freeze another family-disjoint batch before selecting any candidate and
   reveal it once.

Until a new sealed batch reaches R@3 >= 85%, R@10 >= 95%, 100% confidence and
hard-negative safety, and 100% required labels without regression, semantic
quality is **not resolved**.
