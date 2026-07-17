# Bible search red-team report

## Run identity

- Evaluator: v0.1.0 (`/Users/user/Developer/verse-recall/target/release/recall-eval`, 982304 bytes, FNV-1a `561b78a0f9a9dee3`)
- Base pack: `/Users/user/Developer/verse-recall/data/bsb-full.pack` (4730432 bytes, FNV-1a `319bcf4110e62517`)
- Overlay: `/private/tmp/verse-recall-overlay-final.pack` (10366528 bytes, FNV-1a `0f7fc91b7b4e8ba2`)
- Prompts: `/Users/user/Developer/verse-recall/eval/family-regression-v1.jsonl` (6211 bytes, FNV-1a `a2aee7b60ca12be9`)
- Parameters: depth=10, split=None, baseline=false, release_gate=false, registry_count=0

Suite size: **25** prompts. Empty-gold safety cases are excluded from retrieval denominators.

Metrics: **Recall@1/3/10** and **MRR** over gold-bearing prompts; `gold_mode=all` requires every split/conflated gold; **confidence safety** requires every top-3 `Recovered` result to overlap gold (and forbids it for empty-gold prompts); named concepts may require all declared structured passages; hard negatives must rank below the completed gold; gold-bearing misquotes may require an explicit confidence label.

## Overall

| System | Retrieval n | R@1 | R@3 | R@10 | MRR | Confidence safety | Passage output | Hard negatives | Required labels |
|--------|------------:|----:|----:|-----:|----:|------------------:|---------------:|---------------:|----------------:|
| verse-recall | 25 | 76.0% | 84.0% | 92.0% | 0.807 | 100.0% (25/25) | 100.0% (2/2) | 0.0% (0/0) | 0.0% (0/0) |

## By category

| Category | System | n | R@1 | R@3 | R@10 | MRR |
|----------|--------|--:|----:|----:|-----:|----:|
| event-name | verse-recall | 9 | 77.8% | 88.9% | 100.0% | 0.837 |
| event-paraphrase | verse-recall | 8 | 50.0% | 62.5% | 75.0% | 0.580 |
| translation-drift | verse-recall | 8 | 100.0% | 100.0% | 100.0% | 1.000 |

## By family (verse-recall)

| Family | n | R@1 | R@3 | R@10 | MRR |
|--------|--:|----:|----:|-----:|----:|
| burning-bush | 2 | 50.0% | 50.0% | 100.0% | 0.600 |
| ecclesiastes-3-1 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| ephesians-6-12 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| fiery-furnace | 2 | 50.0% | 100.0% | 100.0% | 0.667 |
| good-samaritan | 2 | 50.0% | 50.0% | 100.0% | 0.571 |
| hebrews-4-12 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| prodigal-son | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| road-to-damascus | 2 | 50.0% | 50.0% | 50.0% | 0.500 |
| second-chronicles-7-14 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| sermon-on-the-mount | 2 | 50.0% | 50.0% | 50.0% | 0.500 |
| the-fall | 3 | 66.7% | 100.0% | 100.0% | 0.833 |
| writing-on-wall | 2 | 100.0% | 100.0% | 100.0% | 1.000 |

## Per-prompt (verse-recall)

| id | category | family | gold mode | rank | R@1 | R@3 | R@10 | safe | passage | hard neg | label | lane used | top | gold |
|----|----------|--------|-----------|-----:|-----|-----|------|------|---------|----------|-------|-----------|-----|------|
| regression-fall-01 | event-name | the-fall | Any | 1 | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | concept | Genesis 3:1; Matthew 21:44; Luke 20:18; Ecclesiastes 4:10; James 5:7; Proverbs 16:18; Mark 14:29; Proverbs 13:17; Numbers 14:32; Psalm 20:8 | Genesis 3:1-24 |
| regression-fall-02 | event-name | the-fall | Any | 1 | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | concept | Genesis 3:1; Matthew 21:44; Luke 20:18; Ecclesiastes 4:10; Proverbs 16:18; Mark 14:29; Proverbs 13:17; Numbers 14:32; Psalm 20:8; Luke 10:18 | Genesis 3:1-24 |
| regression-fall-03 | event-paraphrase | the-fall | Any | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | 1 Timothy 2:13; Genesis 3:20; Genesis 4:1; Leviticus 19:23; Psalm 128:2; Genesis 3:2; Isaiah 65:21; Proverbs 1:31; Proverbs 27:18; Proverbs 18:21 | Genesis 3:1-24 |
| regression-samaritan-01 | event-name | good-samaritan | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | concept | Luke 10:25; John 4:9; Luke 17:16; John 4:7; Luke 10:33; Luke 9:52; John 8:48; John 4:40; John 4:39; Matthew 10:5 | Luke 10:25-37 |
| regression-samaritan-02 | event-paraphrase | good-samaritan | Any | 7 | — | — | ✓ | ✓ | n/a | n/a | n/a | lexical | Psalm 39:12; Leviticus 25:35; John 4:9; Luke 17:16; John 4:7; Matthew 13:34; Luke 10:33; Luke 15:3; Mark 4:13; Ecclesiastes 10:9 | Luke 10:25-37 |
| regression-prodigal-01 | event-name | prodigal-son | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+concept) | Luke 15:11; 1 Chronicles 4:37; 1 Chronicles 9:12; 1 Chronicles 5:14; 1 Chronicles 6:21; 1 Chronicles 6:24; 1 Chronicles 9:8; Luke 3:33; 1 Chronicles 3:10; 1 Chronicles 7:25 | Luke 15:11-32 |
| regression-prodigal-02 | event-paraphrase | prodigal-son | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Luke 15:30; Matthew 21:38; Numbers 32:18; Luke 15:13; Micah 2:2; 1 Kings 12:16; 2 Chronicles 10:16; Judges 8:29; Joshua 16:4; 2 Chronicles 25:10 | Luke 15:11-32 |
| regression-damascus-01 | event-name | road-to-damascus | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | concept | Acts 9:1; Acts 9:27; Ezekiel 48:1; Isaiah 17:1; Isaiah 7:8; Matthew 21:8; 2 Kings 16:10; Acts 22:6; Isaiah 10:9; Jeremiah 49:27 | Acts 9:1-19 |
| regression-damascus-02 | event-paraphrase | road-to-damascus | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | John 9:18; Matthew 9:28; Acts 9:26; Ephesians 5:13; Matthew 15:14; Isaiah 42:19; Proverbs 30:22; Luke 6:39; Deuteronomy 24:2; Job 14:11 | Acts 9:1-19 |
| regression-bush-01 | event-name | burning-bush | Any | 5 | — | — | ✓ | ✓ | n/a | n/a | n/a | lexical | Luke 20:37; Mark 12:26; Deuteronomy 33:16; Acts 7:30; Exodus 3:3; Exodus 3:2; Genesis 21:15; Exodus 3:4; Exodus 21:25; Joshua 11:13 | Exodus 3:1-6 |
| regression-bush-02 | event-paraphrase | burning-bush | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Exodus 3:2; Hebrews 12:29; Deuteronomy 4:24; Psalm 106:18; Job 22:20; Exodus 3:3; Exodus 3:4; Psalm 78:63; Acts 7:30; 2 Samuel 22:9 | Exodus 3:1-6 |
| regression-furnace-01 | event-name | fiery-furnace | Any | 3 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Matthew 13:50; Matthew 13:42; Daniel 3:23; Daniel 3:6; Daniel 3:11; Daniel 3:21; Daniel 3:17; Daniel 3:20; Psalm 21:9; Daniel 3:26 | Daniel 3:8-30 |
| regression-furnace-02 | event-paraphrase | fiery-furnace | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Daniel 3:23; 1 Samuel 25:22; Joshua 7:4; Daniel 3:22; 1 Chronicles 11:20; Daniel 3:20; 2 Samuel 23:18; Job 32:1; Judges 7:16; 1 Chronicles 11:12 | Daniel 3:8-30 |
| regression-writing-01 | event-name | writing-on-wall | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Daniel 5:5; Exodus 32:16; 1 Timothy 3:14; Isaiah 38:9; Galatians 1:20; 1 John 2:13; 1 Corinthians 4:14; 1 John 2:12; Deuteronomy 31:24; Acts 15:27 | Daniel 5:1-31 |
| regression-writing-02 | event-paraphrase | writing-on-wall | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Daniel 5:25; Deuteronomy 24:3; Daniel 5:27; Isaiah 49:16; Nehemiah 4:17; Lamentations 2:8; Romans 10:5; Amos 7:7; Daniel 5:5; Daniel 5:26 | Daniel 5:5-28 |
| regression-esv-word-01 | translation-drift | hebrews-4-12 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Hebrews 4:12; Jeremiah 23:36; 1 Peter 1:23; 2 Kings 19:16; Isaiah 37:17; Isaiah 37:4; 2 Kings 19:4; Jeremiah 44:26; Psalm 42:2; 1 Kings 13:11 | Hebrews 4:12 |
| regression-esv-word-02 | translation-drift | hebrews-4-12 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Hebrews 4:12; Proverbs 5:4; Psalm 149:6; Judges 3:16; Revelation 2:12; Revelation 1:16; Luke 22:38; Ezekiel 21:14; 1 Kings 3:25; Genesis 6:19 | Hebrews 4:12 |
| regression-esv-wrestle-01 | translation-drift | ephesians-6-12 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Ephesians 6:12; Zephaniah 1:17; Psalm 50:13; John 6:55; Leviticus 17:14; John 6:56; Deuteronomy 32:42; Genesis 29:14; 1 Chronicles 11:1; 2 Samuel 19:12 | Ephesians 6:12 |
| regression-esv-wrestle-02 | translation-drift | ephesians-6-12 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Ephesians 6:12; Colossians 2:20; Colossians 2:10; Colossians 2:8; Zephaniah 1:17; Titus 3:1; Psalm 50:13; John 6:55; Leviticus 17:14; Joshua 11:5 | Ephesians 6:12 |
| regression-kjv-season-01 | translation-drift | ecclesiastes-3-1 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Ecclesiastes 3:1; 2 Timothy 4:2; Numbers 18:14; Ecclesiastes 12:14; Genesis 8:19; Psalm 145:16; Genesis 9:10; Genesis 7:21; Deuteronomy 23:9; Job 12:10 | Ecclesiastes 3:1 |
| regression-kjv-season-02 | translation-drift | ecclesiastes-3-1 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Ecclesiastes 3:1; Ecclesiastes 8:6; Jeremiah 15:11; Galatians 4:18; Philippians 1:3; Ephesians 3:10; Ephesians 3:15; Isaiah 46:10; Acts 15:21; Acts 11:10 | Ecclesiastes 3:1 |
| regression-numbered-01 | translation-drift | second-chronicles-7-14 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | 2 Chronicles 7:14; 1 Kings 11:39; Proverbs 3:34; James 4:10; 2 Corinthians 10:12; James 1:9; Psalm 147:6; Proverbs 29:23; Psalm 18:27; Psalm 25:9 | 2 Chronicles 7:14 |
| regression-numbered-02 | translation-drift | second-chronicles-7-14 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | 2 Chronicles 7:14; Psalm 83:16; Ezekiel 33:11; Psalm 27:8; Ezekiel 18:23; Acts 3:26; Psalm 80:18; Judges 18:23; 1 Chronicles 16:11; Psalm 105:4 | 2 Chronicles 7:13-15 |
| regression-sermon-01 | event-name | sermon-on-the-mount | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | concept | Matthew 5:1; Psalm 68:15; John 8:1; Leviticus 25:1; Numbers 33:23; Numbers 33:24; Lamentations 5:18; Joshua 15:10; Numbers 33:39; Numbers 34:8 | Matthew 5:1-7:29 |
| regression-sermon-02 | event-paraphrase | sermon-on-the-mount | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | 2 Timothy 1:13; Mark 11:17; Psalm 109:4; Acts 2:42; Mark 9:29; Psalm 119:64; Psalm 66:20; Psalm 119:124; Luke 13:10; John 6:59 | Matthew 5:1-7:29 |

## Notes

- **offline-keyword**: same full BSB TEXT, naive multi-term token overlap without stemming, synonym expansion, or phrase memory (proxy for classic keyword Bible search).
- **Live tools** (Bible Gateway, Blue Letter Bible / BibleHub): scores loaded from captured query results when available; environmentally unreachable tools are recorded as such rather than fabricated.
- Family registries require explicit family/split metadata and reject both literal-family and overlapping-passage leakage across split roles.
- `--release-gate` requires a family registry and enforces R@1 ≥ 60%, R@3 ≥ 85%, R@10 ≥ 95%, 100% confidence safety, 100% required structured-passage coverage, 100% hard-negative ordering, and 100% explicitly required confidence labels.
