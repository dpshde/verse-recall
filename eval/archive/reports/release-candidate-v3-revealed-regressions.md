# Release candidate v3 — revealed regression recheck

Date: 2026-07-16

These results were regenerated after removing the eight provenance-free
direct mappings, adding mandatory prompt/vector SHA-256 binding, and enabling
canonical whole-query plus clause embeddings. Every corpus below was already
revealed before this run. These are compatibility diagnostics, not holdout
evidence and not a generalized-quality claim.

All runs used the verified BSB, expansion, source/KJV overlay, and Arctic-M
256D packs recorded in `data/release-artifacts-v2.json`. Query records were
freshly encoded from the exact prompt UTF-8 and carry
`prompt_utf8_sha256`; a repeat encoding of the semantic suite was
byte-identical. The retained query adapter is `none`.

| Revealed suite | Queries with gold | R@1 | R@3 | R@10 | Confidence safety | Other applicable checks |
|---|---:|---:|---:|---:|---:|---|
| Semantic quality v2 | 40 | 72.5 | 82.5 | 87.5 | 43/43 | passages 2/2; hard negatives 14/16; labels 0/2 |
| Tip-of-the-tongue | 28 | 75.0 | 89.3 | 96.4 | 30/30 | labels 0/2 |
| Overlay adversarial | 29 | 72.4 | 89.7 | 100 | 35/35 | passages 8/12; hard negatives 5/5; labels 1/1 |
| Overfit diagnostic | 28 | 57.1 | 67.9 | 78.6 | 30/30 | labels 0/1 |
| Overfit confirmation | 28 | 82.1 | 92.9 | 100 | 30/30 | — |
| Family regression | 25 | 64.0 | 84.0 | 92.0 | 25/25 | passages 1/2 |
| Red-team prompts | 63 | 58.7 | 74.6 | 90.5 | 66/66 | labels 0/2 |
| Revealed v2 batch | 23 | 65.2 | 73.9 | 82.6 | 24/24 | labels 0/1 |

All 19 empty-gold prompts were confidence-safe. The reduced overlay/family
scores and passage failures are intentional visible regressions from retiring
the source-free query-to-passage table. The release was not repaired from
these misses. Independently sourced reviewed corrections are evaluated against
their own complete 42-surface corpus, where all 42 pass end to end.

Exact corpus, query-record, and generated report hashes are in the companion
JSON. The detailed generated reports were kept outside the repository because
they contain already-revealed per-prompt rankings and are not release inputs.

