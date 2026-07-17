# Bible search red-team report

## Run identity

- Evaluator: v0.1.0 (`/Users/user/Developer/verse-recall/target/release/recall-eval`, 982304 bytes, FNV-1a `561b78a0f9a9dee3`)
- Base pack: `/Users/user/Developer/verse-recall/data/bsb-full.pack` (4730432 bytes, FNV-1a `319bcf4110e62517`)
- Overlay: `/private/tmp/verse-recall-overlay-final.pack` (10366528 bytes, FNV-1a `0f7fc91b7b4e8ba2`)
- Prompts: `/Users/user/Developer/verse-recall/eval/semantic-mutation-frontier-v1.jsonl` (1594 bytes, FNV-1a `0cbc4899b882d616`)
- Parameters: depth=10, split=None, baseline=false, release_gate=false, registry_count=0

Suite size: **5** prompts. Empty-gold safety cases are excluded from retrieval denominators.

Metrics: **Recall@1/3/10** and **MRR** over gold-bearing prompts; `gold_mode=all` requires every split/conflated gold; **confidence safety** requires every top-3 `Recovered` result to overlap gold (and forbids it for empty-gold prompts); named concepts may require all declared structured passages; hard negatives must rank below the completed gold; gold-bearing misquotes may require an explicit confidence label.

## Overall

| System | Retrieval n | R@1 | R@3 | R@10 | MRR | Confidence safety | Passage output | Hard negatives | Required labels |
|--------|------------:|----:|----:|-----:|----:|------------------:|---------------:|---------------:|----------------:|
| verse-recall | 5 | 0.0% | 0.0% | 20.0% | 0.022 | 100.0% (5/5) | 0.0% (0/0) | 66.7% (2/3) | 0.0% (0/0) |

## By category

| Category | System | n | R@1 | R@3 | R@10 | MRR |
|----------|--------|--:|----:|----:|-----:|----:|
| semantic-mutation | verse-recall | 5 | 0.0% | 0.0% | 20.0% | 0.022 |

## By family (verse-recall)

| Family | n | R@1 | R@3 | R@10 | MRR |
|--------|--:|----:|----:|-----:|----:|
| ephesians-6-11-12 | 1 | 0.0% | 0.0% | 100.0% | 0.111 |
| genesis-1-1 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| luke-15-4-7 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| proverbs-3-5 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| second-corinthians-12-9 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |

## Per-prompt (verse-recall)

| id | category | family | gold mode | rank | R@1 | R@3 | R@10 | safe | passage | hard neg | label | lane used | top | gold |
|----|----------|--------|-----------|-----:|-----|-----|------|------|---------|----------|-------|-----------|-----|------|
| smf-creation-01 | semantic-mutation | genesis-1-1 | Any | — | — | — | — | ✓ | n/a | FAIL | n/a | n/a | 2 Corinthians 6:18; Revelation 1:8; Isaiah 44:13; Jeremiah 28:13; Isaiah 28:10; Exodus 28:13; Zechariah 1:16; Isaiah 28:13; Isaiah 44:10; Isaiah 45:18 | Genesis 1:1 |
| smf-trust-01 | semantic-mutation | proverbs-3-5 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Galatians 3:18; Romans 9:16; Ezekiel 5:8; Daniel 3:28; Ezekiel 13:3; 2 Timothy 3:4; Amos 9:15; Ezekiel 36:7; Proverbs 9:4; Proverbs 9:16 | Proverbs 3:5 |
| smf-armor-01 | semantic-mutation | ephesians-6-11-12 | Any | 9 | — | — | ✓ | ✓ | n/a | n/a | n/a | lexical | Matthew 5:39; 1 John 5:19; Judges 3:12; Micah 2:1; Nahum 2:5; 2 Chronicles 13:7; Jeremiah 23:10; James 5:6; Ephesians 6:12; Zechariah 11:15 | Ephesians 6:11-12 |
| smf-sheep-01 | semantic-mutation | luke-15-4-7 | Any | — | — | — | — | ✓ | n/a | ✓ | n/a | n/a | Leviticus 27:32; John 10:12; Ezekiel 34:8; Jeremiah 23:4; Jeremiah 25:35; Ezekiel 34:2; Luke 2:8; 1 Samuel 30:20; Genesis 30:36; Exodus 2:19 | Luke 15:4-7 |
| smf-weakness-01 | semantic-mutation | second-corinthians-12-9 | Any | — | — | — | — | ✓ | n/a | ✓ | n/a | n/a | Acts 8:10; Daniel 10:8; Psalm 88:4; Job 26:2; Psalm 82:1; Matthew 22:38; Judges 6:14; Matthew 23:11; Proverbs 16:10; Matthew 22:36 | 2 Corinthians 12:9 |

## Notes

- **offline-keyword**: same full BSB TEXT, naive multi-term token overlap without stemming, synonym expansion, or phrase memory (proxy for classic keyword Bible search).
- **Live tools** (Bible Gateway, Blue Letter Bible / BibleHub): scores loaded from captured query results when available; environmentally unreachable tools are recorded as such rather than fabricated.
- Family registries require explicit family/split metadata and reject both literal-family and overlapping-passage leakage across split roles.
- `--release-gate` requires a family registry and enforces R@1 ≥ 60%, R@3 ≥ 85%, R@10 ≥ 95%, 100% confidence safety, 100% required structured-passage coverage, 100% hard-negative ordering, and 100% explicitly required confidence labels.
