# Bible search red-team report

## Run identity

- Evaluator: v0.1.0 (`/Users/user/Developer/verse-recall/target/release/recall-eval`, 1164432 bytes, FNV-1a `8b819159e4dc2c0b`)
- Base pack: `/Users/user/Developer/verse-recall/data/bsb-full.pack` (6433152 bytes, FNV-1a `42bb9909d5a56a3a`)
- Overlay: `/Users/user/Developer/verse-recall/data/bsb-expand.pack` (5627520 bytes, FNV-1a `2daba354133287df`)
- Overlay: `/Users/user/Developer/verse-recall/data/recall-overlay.pack` (7428864 bytes, FNV-1a `4fbf2c94cc0cf810`)
- Overlay: `/Users/user/Developer/verse-recall/data/bsb-semantic.pack` (23874304 bytes, FNV-1a `f937771db44d9b79`)
- Prompts: `/Users/user/Developer/verse-recall/eval/semantic-mutation-frontier-v1.jsonl` (1594 bytes, FNV-1a `0cbc4899b882d616`)
- Family registry: `/Users/user/Developer/verse-recall/eval/overlay-adversarial-v1.jsonl` (11023 bytes, FNV-1a `06e867bd96632786`)
- Query embeddings: `/private/tmp/bge-frontier-query.jsonl` (40524 bytes, FNV-1a `1276c540fc3aa194`)
- Embedding space ID: `8744273cd632ff40dc59a2c17bae871a5eebf8ed51a8e96b815c5c65c9672225`
- Parameters: depth=10, split=None, baseline=false, release_gate=false, registry_count=1

Suite size: **5** prompts. Empty-gold safety cases are excluded from retrieval denominators.

Metrics: **Recall@1/3/10** and **MRR** over gold-bearing prompts; `gold_mode=all` requires every split/conflated gold; **confidence safety** requires every top-3 `Recovered` result to overlap gold (and forbids it for empty-gold prompts); named concepts may require all declared structured passages; hard negatives must rank below the completed gold; gold-bearing misquotes may require an explicit confidence label.

## Overall

| System | Retrieval n | R@1 | R@3 | R@10 | MRR | Confidence safety | Passage output | Hard negatives | Required labels |
|--------|------------:|----:|----:|-----:|----:|------------------:|---------------:|---------------:|----------------:|
| verse-recall | 5 | 40.0% | 100.0% | 100.0% | 0.667 | 100.0% (5/5) | 0.0% (0/0) | 100.0% (3/3) | 0.0% (0/0) |

## By category

| Category | System | n | R@1 | R@3 | R@10 | MRR |
|----------|--------|--:|----:|----:|-----:|----:|
| semantic-mutation | verse-recall | 5 | 40.0% | 100.0% | 100.0% | 0.667 |

## By family (verse-recall)

| Family | n | R@1 | R@3 | R@10 | MRR |
|--------|--:|----:|----:|-----:|----:|
| ephesians-6-11-12 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| genesis-1-1 | 1 | 0.0% | 100.0% | 100.0% | 0.333 |
| luke-15-4-7 | 1 | 0.0% | 100.0% | 100.0% | 0.500 |
| proverbs-3-5 | 1 | 0.0% | 100.0% | 100.0% | 0.500 |
| second-corinthians-12-9 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |

## Per-prompt (verse-recall)

| id | category | family | gold mode | rank | R@1 | R@3 | R@10 | safe | passage | hard neg | label | lane used | top | gold |
|----|----------|--------|-----------|-----:|-----|-----|------|------|---------|----------|-------|-----------|-----|------|
| smf-creation-01 | semantic-mutation | genesis-1-1 | Any | 3 | — | ✓ | ✓ | ✓ | n/a | ✓ | n/a | dense | Isaiah 40:28; Revelation 11:18; Genesis 1:1; Exodus 28:13; Isaiah 44:10; Hebrews 11:3; Hebrews 1:2; Revelation 1:8; Psalm 119:73; John 20:29 | Genesis 1:1 |
| smf-trust-01 | semantic-mutation | proverbs-3-5 | Any | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | dense | Job 34:33; Proverbs 3:5; Deuteronomy 1:17; Galatians 3:18; Romans 9:16; Acts 4:19; Daniel 3:28; 2 Kings 18:24; Isaiah 36:9; Isaiah 41:13 | Proverbs 3:5 |
| smf-armor-01 | semantic-mutation | ephesians-6-11-12 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | dense | Ephesians 6:10; Psalm 140:1; Isaiah 41:10; Matthew 5:39; 1 John 5:19; Judges 3:12; Micah 2:1; Ephesians 6:12; Nahum 2:5; Psalm 140:2 | Ephesians 6:11-12 |
| smf-sheep-01 | semantic-mutation | luke-15-4-7 | Any | 2 | — | ✓ | ✓ | ✓ | n/a | ✓ | n/a | dense | Matthew 7:6; Luke 15:5; Jeremiah 10:21; Leviticus 27:32; John 10:12; Jeremiah 25:35; Ezekiel 34:8; 1 Samuel 30:20; Jeremiah 23:4; Joel 2:25 | Luke 15:4-7 |
| smf-weakness-01 | semantic-mutation | second-corinthians-12-9 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | n/a | dense | 2 Corinthians 12:9; Psalm 18:32; 2 Timothy 1:7; Acts 8:10; Daniel 10:8; Psalm 88:4; Job 26:2; Psalm 82:1; Judges 6:14; John 14:2 | 2 Corinthians 12:9 |

## Notes

- **offline-keyword**: same full BSB TEXT, naive multi-term token overlap without stemming, synonym expansion, or phrase memory (proxy for classic keyword Bible search).
- **Live tools** (Bible Gateway, Blue Letter Bible / BibleHub): scores loaded from captured query results when available; environmentally unreachable tools are recorded as such rather than fabricated.
- Family registries require explicit family/split metadata and reject both literal-family and overlapping-passage leakage across split roles.
- `--release-gate` requires a family registry and enforces R@1 ≥ 60%, R@3 ≥ 85%, R@10 ≥ 95%, 100% confidence safety, 100% required structured-passage coverage, 100% hard-negative ordering, and 100% explicitly required confidence labels.
