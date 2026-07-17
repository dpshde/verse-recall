# Bible search red-team report

## Run identity

- Evaluator: v0.1.0 (`/Users/user/Developer/verse-recall/target/release/recall-eval`, 1164432 bytes, FNV-1a `8b819159e4dc2c0b`)
- Base pack: `/Users/user/Developer/verse-recall/data/bsb-full.pack` (6433152 bytes, FNV-1a `42bb9909d5a56a3a`)
- Overlay: `/Users/user/Developer/verse-recall/data/bsb-expand.pack` (5627520 bytes, FNV-1a `2daba354133287df`)
- Overlay: `/Users/user/Developer/verse-recall/data/recall-overlay.pack` (7428864 bytes, FNV-1a `4fbf2c94cc0cf810`)
- Prompts: `/Users/user/Developer/verse-recall/eval/tot-prompts.jsonl` (6346 bytes, FNV-1a `6f81b6179788229f`)
- Family registry: `/Users/user/Developer/verse-recall/eval/redteam-prompts.jsonl` (15913 bytes, FNV-1a `0959a2e627648b1c`)
- Parameters: depth=10, split=None, baseline=false, release_gate=true, registry_count=1

Suite size: **30** prompts. Empty-gold safety cases are excluded from retrieval denominators.

Metrics: **Recall@1/3/10** and **MRR** over gold-bearing prompts; `gold_mode=all` requires every split/conflated gold; **confidence safety** requires every top-3 `Recovered` result to overlap gold (and forbids it for empty-gold prompts); named concepts may require all declared structured passages; hard negatives must rank below the completed gold; gold-bearing misquotes may require an explicit confidence label.

## Overall

| System | Retrieval n | R@1 | R@3 | R@10 | MRR | Confidence safety | Passage output | Hard negatives | Required labels |
|--------|------------:|----:|----:|-----:|----:|------------------:|---------------:|---------------:|----------------:|
| verse-recall | 28 | 85.7% | 100.0% | 100.0% | 0.923 | 100.0% (30/30) | 0.0% (0/0) | 0.0% (0/0) | 100.0% (2/2) |

## By category

| Category | System | n | R@1 | R@3 | R@10 | MRR |
|----------|--------|--:|----:|----:|-----:|----:|
| conflated | verse-recall | 3 | 0.0% | 100.0% | 100.0% | 0.500 |
| false-memory | verse-recall | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| meaning-only | verse-recall | 6 | 83.3% | 100.0% | 100.0% | 0.889 |
| partial-phrase | verse-recall | 8 | 100.0% | 100.0% | 100.0% | 1.000 |
| situation | verse-recall | 5 | 100.0% | 100.0% | 100.0% | 1.000 |
| wrong-translation | verse-recall | 4 | 100.0% | 100.0% | 100.0% | 1.000 |

## By family (verse-recall)

| Family | n | R@1 | R@3 | R@10 | MRR |
|--------|--:|----:|----:|-----:|----:|
| no-gold:false-mem-02 | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| no-gold:false-mem-04 | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| passage:1-timothy-6-10 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:ephesians-6-11 | 4 | 75.0% | 100.0% | 100.0% | 0.833 |
| passage:ephesians-6-11-galatians-5-22 | 1 | 0.0% | 100.0% | 100.0% | 0.500 |
| passage:galatians-5-22 | 3 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:genesis-1-1 | 3 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:john-3-16 | 3 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:john-3-16-genesis-1-1 | 1 | 0.0% | 100.0% | 100.0% | 0.500 |
| passage:luke-15-4 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:luke-15-4-psalm-23-1 | 1 | 0.0% | 100.0% | 100.0% | 0.500 |
| passage:matthew-5-3 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:proverbs-16-18 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:proverbs-3-5 | 3 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:psalm-23-1 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:psalm-23-4 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |

## Per-prompt (verse-recall)

| id | category | family | gold mode | rank | R@1 | R@3 | R@10 | safe | passage | hard neg | label | lane used | top | gold |
|----|----------|--------|-----------|-----:|-----|-----|------|------|---------|----------|-------|-----------|-----|------|
| partial-01 | partial-phrase | passage:luke-15-4 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+concept) | Luke 15:4; Matthew 10:6; Psalm 119:176; Matthew 15:24; Luke 15:6; Jeremiah 50:6; Song Of Solomon 6:6; Song Of Solomon 4:2; Matthew 18:12; Zechariah 11:16 | Luke 15:4 |
| partial-02 | partial-phrase | passage:john-3-16 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+dense+phrase) | John 3:16; 1 John 4:1; 1 John 5:5; 1 John 4:9; James 2:5; 1 John 2:15; 1 John 3:1; John 15:19; John 13:1; John 3:17 | John 3:16 |
| partial-03 | partial-phrase | passage:psalm-23-1 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+dense+phrase) | Psalm 23:1; Ezekiel 34:7; Ezekiel 34:9; Ezekiel 34:2; Zechariah 11:15; Jeremiah 23:1; Zechariah 13:7; Ezekiel 34:8; Leviticus 27:32; Hosea 4:16 | Psalm 23:1 |
| partial-04 | partial-phrase | passage:genesis-1-1 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+phrase) | Genesis 1:1; Romans 1:20; 2 Corinthians 5:17; Mark 10:6; Mark 13:19; Genesis 1:27; Psalm 51:10; 2 Peter 3:4; Romans 8:19; Colossians 1:15 | Genesis 1:1 |
| partial-05 | partial-phrase | passage:psalm-23-4 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+phrase) | Psalm 23:4; Job 38:17; Psalm 107:10; Psalm 107:14; Isaiah 9:2; Matthew 4:16; Luke 1:79; Joshua 11:17; Psalm 144:4; Psalm 102:11 | Psalm 23:4 |
| partial-06 | partial-phrase | passage:proverbs-3-5 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+dense+phrase) | Proverbs 3:5; Psalm 9:10; Psalm 112:7; Psalm 28:7; Jeremiah 17:5; Psalm 33:21; Psalm 13:5; Proverbs 31:11; Psalm 62:8; Psalm 20:7 | Proverbs 3:5 |
| partial-07 | partial-phrase | passage:galatians-5-22 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+concept) | Galatians 5:22; Matthew 7:17; Matthew 7:18; Luke 6:43; Matthew 12:33; Psalm 148:9; Ezekiel 47:12; Matthew 3:8; Genesis 1:11; Matthew 7:20 | Galatians 5:22 |
| partial-08 | partial-phrase | passage:ephesians-6-11 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+concept) | Ephesians 6:11; Ephesians 6:13; 1 Samuel 14:13; Judges 9:54; 2 Chronicles 26:14; 2 Samuel 18:15; 2 Samuel 23:37; 1 Chronicles 11:39; 1 Samuel 14:12; 1 John 5:18 | Ephesians 6:11 |
| meaning-01 | meaning-only | passage:luke-15-4 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Luke 15:4; Matthew 18:13; Matthew 18:12; Genesis 17:24; Genesis 17:1; Luke 15:7; Mark 7:17; Matthew 25:33; Luke 13:6; John 2:14 | Luke 15:4 |
| meaning-02 | meaning-only | passage:matthew-5-3 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Matthew 5:3; Psalm 132:15; Proverbs 22:9; Proverbs 14:21; Psalm 41:1; Luke 6:20; Psalm 32:2; Deuteronomy 15:4; Ruth 3:10; 1 Peter 4:14 | Matthew 5:3 |
| meaning-03 | meaning-only | passage:genesis-1-1 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Genesis 1:1; Acts 17:24; Ecclesiastes 3:11; Ephesians 1:22; Mark 10:6; Mark 10:28; Genesis 1:25; 1 Samuel 3:12; 1 Corinthians 15:27; Acts 4:24 | Genesis 1:1 |
| meaning-04 | meaning-only | passage:proverbs-3-5 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+dense) | Proverbs 3:5; Isaiah 30:12; Micah 7:5; Psalm 78:22; Ezekiel 16:32; Romans 10:19; Numbers 10:30; 1 Corinthians 11:22; 1 Corinthians 6:8; Romans 2:17 | Proverbs 3:5 |
| meaning-05 | meaning-only | passage:galatians-5-22 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+dense+phrase) | Galatians 5:22; Romans 1:20; Romans 15:13; Job 31:39; James 2:19; Matthew 13:8; Luke 8:15; 2 Corinthians 4:13; Psalm 119:66; John 20:29 | Galatians 5:22 |
| meaning-06 | meaning-only | passage:ephesians-6-11 | Any | 3 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | 1 Peter 5:8; James 4:7; Ephesians 6:11; 1 Peter 2:5; Job 9:4; 1 John 3:8; John 13:2; Romans 9:19; Romans 13:2; Luke 4:3 | Ephesians 6:11 |
| wrong-trans-01 | wrong-translation | passage:john-3-16 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+dense+phrase) | John 3:16; 1 John 3:1; John 13:34; Galatians 2:20; 1 John 5:5; Ephesians 5:25; 1 John 4:9; John 3:17; 1 John 2:15; Psalm 136:2 | John 3:16 |
| wrong-trans-02 | wrong-translation | passage:psalm-23-1 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+dense+phrase) | Psalm 23:1; Ezekiel 34:7; Ezekiel 34:9; Ezekiel 34:2; Zechariah 11:15; Jeremiah 23:1; Zechariah 13:7; Deuteronomy 12:20; Jeremiah 10:21; Ezekiel 34:8 | Psalm 23:1 |
| wrong-trans-03 | wrong-translation | passage:genesis-1-1 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+dense+phrase) | Genesis 1:1; Genesis 1:27; Revelation 21:1; Genesis 2:4; Isaiah 45:18; Mark 10:6; Deuteronomy 3:24; Acts 17:24; Isaiah 65:17; Colossians 1:16 | Genesis 1:1 |
| wrong-trans-04 | wrong-translation | passage:ephesians-6-11 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+phrase) | Ephesians 6:11; 1 Kings 10:24; Jeremiah 46:4; 1 Chronicles 10:10; 1 Samuel 17:38; 1 Kings 20:11; 1 Samuel 31:10; Jeremiah 51:3; Judges 20:37; Romans 13:12 | Ephesians 6:11 |
| conflated-01 | conflated | passage:luke-15-4-psalm-23-1 | All | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Luke 15:4; Psalm 23:1; Jeremiah 50:6; Matthew 10:6; John 10:11; John 10:14; Zechariah 11:16; Psalm 119:176; Matthew 15:24; Jeremiah 23:1 | Luke 15:4; Psalm 23:1 |
| conflated-02 | conflated | passage:john-3-16-genesis-1-1 | All | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+dense+phrase); fuse(lexical+expand+phrase) | John 3:16; Genesis 1:1; Romans 1:20; Genesis 2:4; 1 John 2:15; John 6:33; Psalm 136:26; Romans 8:39; Nehemiah 1:5; Isaiah 45:18 | John 3:16; Genesis 1:1 |
| conflated-03 | conflated | passage:ephesians-6-11-galatians-5-22 | All | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase); fuse(lexical+expand+dense+phrase) | Galatians 5:22; Ephesians 6:11; Romans 8:14; 1 John 4:1; Judges 9:54; Ephesians 6:17; John 3:16; Romans 8:16; Genesis 1:11; Amos 8:1 | Ephesians 6:11; Galatians 5:22 |
| situation-01 | situation | passage:john-3-16 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+dense+phrase) | John 3:16; Luke 1:31; 1 John 3:1; 1 John 5:5; John 13:34; Ephesians 5:25; 1 John 4:9; Job 34:13; Proverbs 8:31; John 3:17 | John 3:16 |
| situation-02 | situation | passage:psalm-23-4 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Psalm 23:4; John 8:12; Proverbs 14:26; Job 29:3; Leviticus 26:6; Proverbs 3:25; Ecclesiastes 12:5; Isaiah 50:10; 1 John 2:11; Proverbs 2:13 | Psalm 23:4 |
| situation-03 | situation | passage:ephesians-6-11 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Ephesians 6:11; Luke 11:22; 1 Samuel 17:38; 1 Chronicles 10:10; Jeremiah 46:4; 1 Peter 2:5; Romans 13:12; 1 Samuel 31:10; 1 Kings 20:11; Jeremiah 51:3 | Ephesians 6:11 |
| situation-04 | situation | passage:proverbs-3-5 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Proverbs 3:5; 2 Peter 1:20; Proverbs 20:24; Acts 20:34; Luke 22:71; 2 Peter 3:3; Jeremiah 3:15; John 16:30; 2 Peter 1:3; Ephesians 4:28 | Proverbs 3:5 |
| situation-05 | situation | passage:galatians-5-22 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+dense+phrase) | Galatians 5:22; Titus 3:5; Romans 8:26; Hebrews 10:15; 2 Timothy 1:14; Acts 11:15; Romans 5:5; Acts 15:28; Psalm 143:4; Acts 15:8 | Galatians 5:22 |
| false-mem-01 | false-memory | passage:1-timothy-6-10 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | fuse(lexical+alternate+phrase) | 1 Timothy 6:10; Romans 12:21; Proverbs 12:12; 2 Kings 12:4; Romans 11:18; Job 5:3; Deuteronomy 14:25; Psalm 37:35; Ecclesiastes 5:10; Luke 22:5 | 1 Timothy 6:10 |
| false-mem-02 | false-memory | no-gold:false-mem-02 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | 1 Timothy 6:6; 1 Timothy 4:7; 2 Peter 1:7; 2 Timothy 3:5; 2 Peter 1:6; 1 Timothy 6:11; 1 Timothy 6:5; 1 Timothy 2:2; 1 Timothy 4:8; Nehemiah 3:2 |  |
| false-mem-03 | false-memory | passage:proverbs-16-18 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | fuse(lexical+expand+phrase) | Proverbs 16:18; Psalm 97:3; Ezekiel 32:12; Leviticus 26:7; Deuteronomy 1:30; Deuteronomy 31:8; Isaiah 52:12; Isaiah 54:15; Isaiah 10:34; Revelation 4:10 | Proverbs 16:18 |
| false-mem-04 | false-memory | no-gold:false-mem-04 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Romans 13:2; Daniel 11:39; Isaiah 41:10; 1 Corinthians 12:28; Titus 3:8; Proverbs 23:20; Hebrews 2:18; Galatians 5:12; Psalm 146:5; Psalm 109:26 |  |

## Notes

- **offline-keyword**: same full BSB TEXT, naive multi-term token overlap without stemming, synonym expansion, or phrase memory (proxy for classic keyword Bible search).
- **Live tools** (Bible Gateway, Blue Letter Bible / BibleHub): scores loaded from captured query results when available; environmentally unreachable tools are recorded as such rather than fabricated.
- Family registries require explicit family/split metadata and reject both literal-family and overlapping-passage leakage across split roles.
- `--release-gate` requires a family registry and enforces R@1 ≥ 60%, R@3 ≥ 85%, R@10 ≥ 95%, 100% confidence safety, 100% required structured-passage coverage, 100% hard-negative ordering, and 100% explicitly required confidence labels.
