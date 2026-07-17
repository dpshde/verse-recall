# Bible search red-team report

## Run identity

- Evaluator: v0.1.0 (`/Users/user/Developer/verse-recall/target/release/recall-eval`, 1164432 bytes, FNV-1a `32b9bb1c175def06`)
- Base pack: `/Users/user/Developer/verse-recall/data/bsb-full.pack` (6433152 bytes, FNV-1a `42bb9909d5a56a3a`)
- Overlay: `/Users/user/Developer/verse-recall/data/bsb-expand.pack` (5627520 bytes, FNV-1a `2daba354133287df`)
- Overlay: `/Users/user/Developer/verse-recall/data/recall-overlay.pack` (7428864 bytes, FNV-1a `4fbf2c94cc0cf810`)
- Overlay: `/Users/user/Developer/verse-recall/data/bsb-semantic.pack` (23874304 bytes, FNV-1a `f937771db44d9b79`)
- Prompts: `/Users/user/Developer/verse-recall/eval/tot-prompts.jsonl` (6346 bytes, FNV-1a `6f81b6179788229f`)
- Family registry: `/Users/user/Developer/verse-recall/eval/overfit-holdout-v2.jsonl` (6773 bytes, FNV-1a `8f681b787ff2ec62`)
- Query embeddings: `/private/tmp/bge-tot-query.jsonl` (243321 bytes, FNV-1a `13c225e7c0c84033`)
- Embedding space ID: `8744273cd632ff40dc59a2c17bae871a5eebf8ed51a8e96b815c5c65c9672225`
- Parameters: depth=10, split=Some("regression-v1"), baseline=false, release_gate=true, registry_count=1

Suite size: **30** prompts. Empty-gold safety cases are excluded from retrieval denominators.

Metrics: **Recall@1/3/10** and **MRR** over gold-bearing prompts; `gold_mode=all` requires every split/conflated gold; **confidence safety** requires every top-3 `Recovered` result to overlap gold (and forbids it for empty-gold prompts); named concepts may require all declared structured passages; hard negatives must rank below the completed gold; gold-bearing misquotes may require an explicit confidence label.

## Overall

| System | Retrieval n | R@1 | R@3 | R@10 | MRR | Confidence safety | Passage output | Hard negatives | Required labels |
|--------|------------:|----:|----:|-----:|----:|------------------:|---------------:|---------------:|----------------:|
| verse-recall | 28 | 82.1% | 96.4% | 100.0% | 0.894 | 100.0% (30/30) | 0.0% (0/0) | 0.0% (0/0) | 100.0% (2/2) |

## By category

| Category | System | n | R@1 | R@3 | R@10 | MRR |
|----------|--------|--:|----:|----:|-----:|----:|
| conflated | verse-recall | 3 | 0.0% | 100.0% | 100.0% | 0.500 |
| false-memory | verse-recall | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| meaning-only | verse-recall | 6 | 66.7% | 83.3% | 100.0% | 0.756 |
| partial-phrase | verse-recall | 8 | 100.0% | 100.0% | 100.0% | 1.000 |
| situation | verse-recall | 5 | 100.0% | 100.0% | 100.0% | 1.000 |
| wrong-translation | verse-recall | 4 | 100.0% | 100.0% | 100.0% | 1.000 |

## By family (verse-recall)

| Family | n | R@1 | R@3 | R@10 | MRR |
|--------|--:|----:|----:|-----:|----:|
| no-gold:false-mem-02 | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| no-gold:false-mem-04 | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| passage:1-timothy-6-10 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:ephesians-6-11 | 4 | 75.0% | 75.0% | 100.0% | 0.800 |
| passage:ephesians-6-11-galatians-5-22 | 1 | 0.0% | 100.0% | 100.0% | 0.500 |
| passage:galatians-5-22 | 3 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:genesis-1-1 | 3 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:john-3-16 | 3 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:john-3-16-genesis-1-1 | 1 | 0.0% | 100.0% | 100.0% | 0.500 |
| passage:luke-15-4 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:luke-15-4-psalm-23-1 | 1 | 0.0% | 100.0% | 100.0% | 0.500 |
| passage:matthew-5-3 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:proverbs-16-18 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:proverbs-3-5 | 3 | 66.7% | 100.0% | 100.0% | 0.778 |
| passage:psalm-23-1 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:psalm-23-4 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |

## Per-prompt (verse-recall)

| id | category | family | gold mode | rank | R@1 | R@3 | R@10 | safe | passage | hard neg | label | lane used | top | gold |
|----|----------|--------|-----------|-----:|-----|-----|------|------|---------|----------|-------|-----------|-----|------|
| partial-01 | partial-phrase | passage:luke-15-4 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+concept) | Luke 15:4; Matthew 10:6; Psalm 119:176; Matthew 15:24; Luke 15:6; Jeremiah 50:6; Luke 15:5; Jeremiah 50:7; Matthew 18:12; Matthew 18:13 | Luke 15:4 |
| partial-02 | partial-phrase | passage:john-3-16 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+phrase) | John 3:16; 1 John 4:9; James 2:5; 1 John 2:15; 1 John 3:1; John 15:19; John 14:2; Psalm 126:4; Proverbs 17:22; Psalm 47:7 | John 3:16 |
| partial-03 | partial-phrase | passage:psalm-23-1 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+phrase) | Psalm 23:1; Ezekiel 34:7; Ezekiel 34:9; Ezekiel 34:2; Zechariah 11:15; Jeremiah 23:1; Psalm 23:2; Psalm 99:5; Psalm 118:1; John 4:24 | Psalm 23:1 |
| partial-04 | partial-phrase | passage:genesis-1-1 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+phrase) | Genesis 1:1; Mark 10:6; Mark 13:19; Genesis 1:27; Psalm 51:10; 2 Peter 3:4; Isaiah 60:1; Genesis 1:2; Jeremiah 1:5; Mark 11:24 | Genesis 1:1 |
| partial-05 | partial-phrase | passage:psalm-23-4 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+phrase) | Psalm 23:4; Job 38:17; Psalm 107:10; Psalm 107:14; Isaiah 9:2; Matthew 4:16; Isaiah 22:2; Matthew 11:21; Isaiah 22:6; Jeremiah 7:32 | Psalm 23:4 |
| partial-06 | partial-phrase | passage:proverbs-3-5 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Proverbs 3:5; Psalm 112:7; Psalm 28:7; Jeremiah 17:5; Psalm 33:21; Psalm 13:5; 2 Corinthians 1:3; Proverbs 3:6; John 14:6; Psalm 9:10 | Proverbs 3:5 |
| partial-07 | partial-phrase | passage:galatians-5-22 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+concept) | Galatians 5:22; Matthew 7:17; Matthew 7:18; Luke 6:43; Matthew 12:33; Psalm 148:9; Jeremiah 17:8; Romans 8:8; Galatians 5:23; John 15:5 | Galatians 5:22 |
| partial-08 | partial-phrase | passage:ephesians-6-11 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+concept) | Ephesians 6:11; Ephesians 6:13; 1 Samuel 14:13; Judges 9:54; 2 Chronicles 26:14; 2 Samuel 18:15; Psalm 91:1; Ephesians 6:12; 1 John 5:18; Isaiah 54:17 | Ephesians 6:11 |
| meaning-01 | meaning-only | passage:luke-15-4 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Luke 15:4; Matthew 18:13; Matthew 18:12; Genesis 17:24; Genesis 17:1; Luke 15:7; Numbers 11:25; Genesis 1:29; Luke 15:5; Ecclesiastes 11:6 | Luke 15:4 |
| meaning-02 | meaning-only | passage:matthew-5-3 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Matthew 5:3; Psalm 132:15; Proverbs 22:9; Proverbs 14:21; Psalm 41:1; Luke 6:20; Matthew 5:4; Romans 15:13; John 14:2; Proverbs 31:30 | Matthew 5:3 |
| meaning-03 | meaning-only | passage:genesis-1-1 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Genesis 1:1; Ecclesiastes 3:11; Ephesians 1:22; Mark 10:6; Acts 17:24; Mark 10:28; Exodus 20:1; Romans 1:20; John 1:2; 2 Corinthians 8:11 | Genesis 1:1 |
| meaning-04 | meaning-only | passage:proverbs-3-5 | Any | 3 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | dense | Proverbs 3:6; 1 Corinthians 4:2; Proverbs 3:5; Isaiah 30:12; Micah 7:5; Psalm 78:22; Ezekiel 16:32; Romans 10:19; Matthew 7:13; Job 11:6 | Proverbs 3:5 |
| meaning-05 | meaning-only | passage:galatians-5-22 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Galatians 5:22; Job 31:39; James 2:19; Matthew 13:8; Luke 8:15; 2 Corinthians 4:13; John 20:29; Psalm 23:6; Galatians 5:23; Genesis 1:29 | Galatians 5:22 |
| meaning-06 | meaning-only | passage:ephesians-6-11 | Any | 5 | — | — | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Ephesians 6:12; 1 Peter 5:8; Ecclesiastes 12:7; James 4:7; Ephesians 6:11; 1 Peter 2:5; Job 9:4; 1 John 3:8; John 13:2; John 4:24 | Ephesians 6:11 |
| wrong-trans-01 | wrong-translation | passage:john-3-16 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+phrase) | John 3:16; 1 John 4:9; John 3:17; 1 John 2:15; 1 John 3:1; Psalm 136:2; Revelation 3:21; Psalm 18:50; Psalm 37:25; Proverbs 22:6 | John 3:16 |
| wrong-trans-02 | wrong-translation | passage:psalm-23-1 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+phrase) | Psalm 23:1; Ezekiel 34:7; Ezekiel 34:9; Ezekiel 34:2; Zechariah 11:15; Jeremiah 23:1; Psalm 99:5; Psalm 23:2; Psalm 118:1; John 4:24 | Psalm 23:1 |
| wrong-trans-03 | wrong-translation | passage:genesis-1-1 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+phrase) | Genesis 1:1; Genesis 2:4; Isaiah 45:18; Mark 10:6; Deuteronomy 3:24; Isaiah 65:17; Isaiah 60:1; Genesis 1:2; 1 Peter 1:16; 2 Corinthians 5:17 | Genesis 1:1 |
| wrong-trans-04 | wrong-translation | passage:ephesians-6-11 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+phrase) | Ephesians 6:11; 1 Kings 10:24; Jeremiah 46:4; 1 Chronicles 10:10; 1 Samuel 17:38; 1 Kings 20:11; Psalm 91:1; Ephesians 6:12; 1 John 5:18; John 14:27 | Ephesians 6:11 |
| conflated-01 | conflated | passage:luke-15-4-psalm-23-1 | All | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Luke 15:4; Psalm 23:1; Jeremiah 50:6; Matthew 10:6; John 10:11; John 10:14; Jeremiah 50:7; John 1:29; Psalm 99:5; Ezekiel 34:7 | Luke 15:4; Psalm 23:1 |
| conflated-02 | conflated | passage:john-3-16-genesis-1-1 | All | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Genesis 1:1; John 3:16; Genesis 2:4; 1 John 2:15; John 6:33; Psalm 136:26; Ecclesiastes 3:11; Psalm 33:7; Psalm 37:4; Psalm 119:103 | John 3:16; Genesis 1:1 |
| conflated-03 | conflated | passage:ephesians-6-11-galatians-5-22 | All | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Galatians 5:22; Ephesians 6:11; Romans 8:16; Genesis 1:11; Amos 8:1; 1 John 4:2; Job 33:5; 1 John 4:1; Ezekiel 18:4; Ephesians 5:10 | Ephesians 6:11; Galatians 5:22 |
| situation-01 | situation | passage:john-3-16 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | John 3:16; Luke 1:31; 1 John 4:9; Job 34:13; Proverbs 8:31; John 3:17; Joel 1:1; Matthew 5:48; Hebrews 1:9; Proverbs 22:6 | John 3:16 |
| situation-02 | situation | passage:psalm-23-4 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Psalm 23:4; Job 29:3; Leviticus 26:6; Proverbs 3:25; Ecclesiastes 12:5; Isaiah 50:10; Isaiah 54:17; Jeremiah 13:16; Psalm 31:10; Matthew 6:25 | Psalm 23:4 |
| situation-03 | situation | passage:ephesians-6-11 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Ephesians 6:11; Luke 11:22; 1 Samuel 17:38; 1 Chronicles 10:10; Jeremiah 46:4; 1 Peter 2:5; Ecclesiastes 12:7; Ephesians 6:13; Isaiah 35:4; John 4:24 | Ephesians 6:11 |
| situation-04 | situation | passage:proverbs-3-5 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Proverbs 3:5; 2 Peter 1:20; Proverbs 20:24; Acts 20:34; Luke 22:71; 2 Peter 3:3; Proverbs 22:18; Romans 2:8; Proverbs 22:17; James 1:5 | Proverbs 3:5 |
| situation-05 | situation | passage:galatians-5-22 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Galatians 5:22; Hebrews 10:15; 2 Timothy 1:14; Acts 11:15; Romans 5:5; Acts 15:28; Genesis 1:29; Haggai 2:20; Ecclesiastes 11:6; Ezekiel 17:10 | Galatians 5:22 |
| false-mem-01 | false-memory | passage:1-timothy-6-10 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | fuse(lexical+alternate+phrase) | 1 Timothy 6:10; Proverbs 12:12; 2 Kings 12:4; Romans 11:18; Job 5:3; Deuteronomy 14:25; Romans 12:21; Luke 10:19; 1 Timothy 6:9; 1 Thessalonians 5:22 | 1 Timothy 6:10 |
| false-mem-02 | false-memory | no-gold:false-mem-02 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Leviticus 10:10; Proverbs 3:6; Luke 11:40; 1 Timothy 6:6; 1 Timothy 4:7; 2 Peter 1:7; 2 Timothy 3:5; 2 Peter 1:6; 1 Timothy 6:11; Revelation 21:1 |  |
| false-mem-03 | false-memory | passage:proverbs-16-18 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | fuse(lexical+expand+phrase) | Proverbs 16:18; Psalm 97:3; Ezekiel 32:12; Leviticus 26:7; Deuteronomy 1:30; Deuteronomy 31:8; Titus 2:11; Proverbs 16:19; Micah 6:8; Proverbs 18:12 | Proverbs 16:18 |
| false-mem-04 | false-memory | no-gold:false-mem-04 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Romans 13:2; Daniel 11:39; 1 Corinthians 12:28; Titus 3:8; Proverbs 23:20; Hebrews 2:18; Job 21:19; Philippians 2:3; 2 Corinthians 9:7; 1 Timothy 5:8 |  |

## Notes

- **offline-keyword**: same full BSB TEXT, naive multi-term token overlap without stemming, synonym expansion, or phrase memory (proxy for classic keyword Bible search).
- **Live tools** (Bible Gateway, Blue Letter Bible / BibleHub): scores loaded from captured query results when available; environmentally unreachable tools are recorded as such rather than fabricated.
- Family registries require explicit family/split metadata and reject both literal-family and overlapping-passage leakage across split roles.
- `--release-gate` requires a family registry and enforces R@1 ≥ 60%, R@3 ≥ 85%, R@10 ≥ 95%, 100% confidence safety, 100% required structured-passage coverage, 100% hard-negative ordering, and 100% explicitly required confidence labels.
