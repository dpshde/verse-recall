# Bible search red-team report

## Run identity

- Evaluator: v0.1.0 (`/Users/user/Developer/verse-recall/target/release/recall-eval`, 1015408 bytes, FNV-1a `cfa36eda6a63f4a8`)
- Base pack: `/Users/user/Developer/verse-recall/data/bsb-full.pack` (4730432 bytes, FNV-1a `319bcf4110e62517`)
- Overlay: `/private/tmp/full-study-overlay.pack` (13441856 bytes, FNV-1a `58c479f1be8982bc`)
- Overlay: `/private/tmp/bge-semantic.pack` (23874304 bytes, FNV-1a `0f907acf9ea69b25`)
- Prompts: `/Users/user/Developer/verse-recall/eval/semantic-mutation-frontier-v1.jsonl` (1594 bytes, FNV-1a `0cbc4899b882d616`)
- Parameters: depth=10, split=None, baseline=false, release_gate=false, registry_count=0

Suite size: **5** prompts. Empty-gold safety cases are excluded from retrieval denominators.

Metrics: **Recall@1/3/10** and **MRR** over gold-bearing prompts; `gold_mode=all` requires every split/conflated gold; **confidence safety** requires every top-3 `Recovered` result to overlap gold (and forbids it for empty-gold prompts); named concepts may require all declared structured passages; hard negatives must rank below the completed gold; gold-bearing misquotes may require an explicit confidence label.

## Overall

| System | Retrieval n | R@1 | R@3 | R@10 | MRR | Confidence safety | Passage output | Hard negatives | Required labels |
|--------|------------:|----:|----:|-----:|----:|------------------:|---------------:|---------------:|----------------:|
| verse-recall | 5 | 0.0% | 60.0% | 80.0% | 0.273 | 100.0% (5/5) | 0.0% (0/0) | 100.0% (3/3) | 0.0% (0/0) |

## By category

| Category | System | n | R@1 | R@3 | R@10 | MRR |
|----------|--------|--:|----:|----:|-----:|----:|
| semantic-mutation | verse-recall | 5 | 0.0% | 60.0% | 80.0% | 0.273 |

## By family (verse-recall)

| Family | n | R@1 | R@3 | R@10 | MRR |
|--------|--:|----:|----:|-----:|----:|
| ephesians-6-11-12 | 1 | 0.0% | 0.0% | 100.0% | 0.200 |
| genesis-1-1 | 1 | 0.0% | 100.0% | 100.0% | 0.500 |
| luke-15-4-7 | 1 | 0.0% | 100.0% | 100.0% | 0.333 |
| proverbs-3-5 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| second-corinthians-12-9 | 1 | 0.0% | 100.0% | 100.0% | 0.333 |

## Per-prompt (verse-recall)

| id | category | family | gold mode | rank | R@1 | R@3 | R@10 | safe | passage | hard neg | label | lane used | top | gold |
|----|----------|--------|-----------|-----:|-----|-----|------|------|---------|----------|-------|-----------|-----|------|
| smf-creation-01 | semantic-mutation | genesis-1-1 | Any | 2 | — | ✓ | ✓ | ✓ | n/a | ✓ | n/a | dense | Revelation 11:18; Genesis 1:1; Isaiah 42:5; Nehemiah 9:6; Isaiah 45:19; John 1:1; Genesis 35:11; Hebrews 1:10; Isaiah 45:18; Hebrews 1:11 | Genesis 1:1 |
| smf-trust-01 | semantic-mutation | proverbs-3-5 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Job 34:33; Deuteronomy 1:17; Romans 14:12; Romans 14:11; Romans 14:22; Romans 12:3; Ecclesiastes 3:18; Matthew 6:1; Job 34:32; Ezekiel 18:30 | Proverbs 3:5 |
| smf-armor-01 | semantic-mutation | ephesians-6-11-12 | Any | 5 | — | — | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+dense) | Psalm 140:1; Psalm 140:2; Ephesians 6:13; Psalm 140:4; Ephesians 6:12; John 17:15; Micah 2:1; Psalm 140:3; Jeremiah 17:18; 2 Samuel 22:29 | Ephesians 6:11-12 |
| smf-sheep-01 | semantic-mutation | luke-15-4-7 | Any | 3 | — | ✓ | ✓ | ✓ | n/a | ✓ | n/a | dense | Jeremiah 10:21; Ezekiel 34:6; Luke 15:5; Genesis 29:3; Ezekiel 34:7; Ezekiel 34:12; Ezekiel 34:5; John 10:14; John 10:12; Exodus 2:18 | Luke 15:4-7 |
| smf-weakness-01 | semantic-mutation | second-corinthians-12-9 | Any | 3 | — | ✓ | ✓ | ✓ | n/a | ✓ | n/a | dense | Psalm 18:32; 2 Corinthians 12:10; 2 Corinthians 12:9; Philippians 4:13; 2 Corinthians 12:8; 2 Samuel 22:34; Psalm 77:14; Psalm 18:33; Psalm 18:1; Lamentations 3:19 | 2 Corinthians 12:9 |

## Notes

- **offline-keyword**: same full BSB TEXT, naive multi-term token overlap without stemming, synonym expansion, or phrase memory (proxy for classic keyword Bible search).
- **Live tools** (Bible Gateway, Blue Letter Bible / BibleHub): scores loaded from captured query results when available; environmentally unreachable tools are recorded as such rather than fabricated.
- Family registries require explicit family/split metadata and reject both literal-family and overlapping-passage leakage across split roles.
- `--release-gate` requires a family registry and enforces R@1 ≥ 60%, R@3 ≥ 85%, R@10 ≥ 95%, 100% confidence safety, 100% required structured-passage coverage, 100% hard-negative ordering, and 100% explicitly required confidence labels.
