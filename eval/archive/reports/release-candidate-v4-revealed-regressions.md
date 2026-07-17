# Release candidate v4 — revealed regression evidence

Date: 2026-07-16

This is the final compatibility measurement before the next sealed reveal. It
uses the normalizer-v2 base/expansion packs, the rebuilt source overlay, the
unchanged Arctic-M 256D verse table, and the generic bounded semantic-tail
schedule. Every prompt in this table had already been revealed. These numbers
are a compatibility floor, not a generalization claim.

| Revealed suite | Gold queries | R@1 | R@3 | R@10 | R@100 |
|---|---:|---:|---:|---:|---:|
| Confirmation | 28 | 82.1 | 92.9 | 100 | 100 |
| Diagnostic | 28 | 60.7 | 71.4 | 82.1 | 96.4 |
| Family regression | 25 | 60.0 | 84.0 | 92.0 | 92.0 |
| Overlay adversarial | 29 | 75.9 | 89.7 | 96.6 | 100 |
| Red team | 63 | 57.1 | 74.6 | 90.5 | 95.2 |
| Semantic quality | 40 | 70.0 | 80.0 | 90.0 | 97.5 |
| Tip of the tongue | 28 | 75.0 | 85.7 | 96.4 | 100 |
| Revealed v2 batch | 23 | 65.2 | 73.9 | 82.6 | 95.7 |
| **Pooled** | **264** | **67.0** | **80.7** | **91.3** | **97.0** |

Confidence safety is 283/283 and empty-gold safety is 19/19. The visible
failures are not repaired from gold answers: applicable hard-negative ordering
is 19/21, structured-passage accuracy is 12/16, and required labels in old
prompt suites are 1/9. Separately, the independently source-reviewed v2
correction corpus passes 94/94 complete surfaces end to end, including rank,
confidence, label, provenance, and declared passages.

The overlay now derives 396 globally collision-checked signatures from the
complete 402-event BSB inventory. They cover 195 of 247 direct-safe event
records; 155 events are quarantined and ambiguous signatures are suppressed.
This adds no prompt-authored alias and all event evidence remains `Related`.

The product improves broad retrieval depth without pretending that revealed
R@3 is sufficient. The governing decision is the one-shot family-disjoint v6
report under the non-bypassable release profile. A failed v6 is retired as a
regression and is not patched.
