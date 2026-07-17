# Bible search red-team report

## Run identity

- Evaluator: v0.1.0 (`/Users/user/Developer/verse-recall/target/release/recall-eval`, 1015408 bytes, FNV-1a `cfa36eda6a63f4a8`)
- Base pack: `/Users/user/Developer/verse-recall/data/bsb-full.pack` (4730432 bytes, FNV-1a `319bcf4110e62517`)
- Overlay: `/private/tmp/full-study-overlay.pack` (13441856 bytes, FNV-1a `58c479f1be8982bc`)
- Overlay: `/private/tmp/bge-semantic.pack` (23874304 bytes, FNV-1a `0f907acf9ea69b25`)
- Prompts: `/Users/user/Developer/verse-recall/eval/semantic-quality-v2.jsonl` (12015 bytes, FNV-1a `e5aee9f70cb1681a`)
- Parameters: depth=10, split=None, baseline=false, release_gate=false, registry_count=0

Suite size: **43** prompts. Empty-gold safety cases are excluded from retrieval denominators.

Metrics: **Recall@1/3/10** and **MRR** over gold-bearing prompts; `gold_mode=all` requires every split/conflated gold; **confidence safety** requires every top-3 `Recovered` result to overlap gold (and forbids it for empty-gold prompts); named concepts may require all declared structured passages; hard negatives must rank below the completed gold; gold-bearing misquotes may require an explicit confidence label.

## Overall

| System | Retrieval n | R@1 | R@3 | R@10 | MRR | Confidence safety | Passage output | Hard negatives | Required labels |
|--------|------------:|----:|----:|-----:|----:|------------------:|---------------:|---------------:|----------------:|
| verse-recall | 40 | 62.5% | 77.5% | 82.5% | 0.700 | 100.0% (43/43) | 100.0% (2/2) | 87.5% (14/16) | 100.0% (2/2) |

## By category

| Category | System | n | R@1 | R@3 | R@10 | MRR |
|----------|--------|--:|----:|----:|-----:|----:|
| conflation | verse-recall | 3 | 0.0% | 33.3% | 33.3% | 0.167 |
| false-memory | verse-recall | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| meaning-only | verse-recall | 12 | 16.7% | 50.0% | 66.7% | 0.333 |
| misquote | verse-recall | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| named-concept | verse-recall | 5 | 60.0% | 80.0% | 80.0% | 0.700 |
| near-quote | verse-recall | 10 | 100.0% | 100.0% | 100.0% | 1.000 |
| translation-drift | verse-recall | 8 | 100.0% | 100.0% | 100.0% | 1.000 |

## By family (verse-recall)

| Family | n | R@1 | R@3 | R@10 | MRR |
|--------|--:|----:|----:|-----:|----:|
| armor-of-god | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| body-temple | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| cleanliness-aphorism | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| exodus-34-6 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| first-corinthians-13-4 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| first-peter-5-7 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| first-timothy-6-10 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| fruit-of-spirit | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| genesis-1-1 | 1 | 0.0% | 100.0% | 100.0% | 0.500 |
| helps-themselves-aphorism | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| isaiah-11-6 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| isaiah-40-31 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| isaiah-41-10 | 1 | 0.0% | 100.0% | 100.0% | 0.333 |
| isaiah-54-17 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| jeremiah-29-11 | 2 | 50.0% | 100.0% | 100.0% | 0.750 |
| jeremiah-29-11-plus-romans-8-28 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| john-14-2 | 2 | 50.0% | 50.0% | 50.0% | 0.500 |
| john-3-16 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| joshua-1-9-plus-isaiah-41-10 | 1 | 0.0% | 100.0% | 100.0% | 0.500 |
| love-one-another | 1 | 0.0% | 100.0% | 100.0% | 0.500 |
| luke-15-4-7 | 1 | 0.0% | 100.0% | 100.0% | 0.333 |
| matthew-11-28 | 2 | 50.0% | 50.0% | 50.0% | 0.500 |
| nehemiah-8-10 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| no-more-than-handle | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| philippians-4-13 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| philippians-4-13-plus-first-corinthians-10-13 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| philippians-4-6 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| proverbs-3-5 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| psalm-23-1 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| psalm-23-4 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| psalm-23-5 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| psalm-46-10 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| romans-12-2 | 1 | 0.0% | 0.0% | 100.0% | 0.200 |
| romans-8-28 | 2 | 50.0% | 50.0% | 50.0% | 0.500 |
| second-corinthians-12-9 | 1 | 0.0% | 0.0% | 100.0% | 0.125 |
| second-timothy-2-15 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| ten-commandments | 1 | 0.0% | 0.0% | 0.0% | 0.000 |

## Per-prompt (verse-recall)

| id | category | family | gold mode | rank | R@1 | R@3 | R@10 | safe | passage | hard neg | label | lane used | top | gold |
|----|----------|--------|-----------|-----:|-----|-----|------|------|---------|----------|-------|-----------|-----|------|
| sq2-near-01 | near-quote | nehemiah-8-10 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Nehemiah 8:10; 1 Chronicles 16:27; Psalm 81:1; Psalm 18:1; Psalm 29:1; Psalm 29:11; 1 Chronicles 16:11; Psalm 105:4; 1 Chronicles 16:28; Psalm 96:7 | Nehemiah 8:10 |
| sq2-near-02 | near-quote | first-corinthians-13-4 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | 1 Corinthians 13:4; Psalm 86:5; Psalm 109:21; Psalm 27:14; Genesis 37:4; Genesis 27:9; 1 Timothy 6:10; Luke 21:19; Hebrews 6:15; Romans 12:12 | 1 Corinthians 13:4 |
| sq2-near-03 | near-quote | psalm-23-1 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | n/a | fuse(lexical+alternate+phrase) | Psalm 23:1; Ezekiel 34:7; Ezekiel 34:9; Ezekiel 34:2; Zechariah 11:15; Jeremiah 23:1; Zechariah 13:7; Ezekiel 34:8; Leviticus 27:32; Hosea 4:16 | Psalm 23:1 |
| sq2-near-04 | near-quote | john-3-16 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+phrase) | John 3:16; 1 John 4:9; 1 John 2:15; John 3:17; James 2:5; John 12:25; 1 John 4:16; John 15:19; 1 John 3:1; John 13:1 | John 3:16 |
| sq2-near-05 | near-quote | philippians-4-13 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | n/a | fuse(lexical+alternate) | Philippians 4:13; 1 Corinthians 8:6; Hebrews 13:21; Hebrews 9:11; 1 Peter 4:11; Colossians 1:20; John 1:17; 2 Corinthians 3:4; 2 Corinthians 1:5; Romans 5:1 | Philippians 4:13 |
| sq2-near-06 | near-quote | psalm-46-10 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | n/a | fuse(lexical+alternate) | Psalm 46:10; Exodus 9:30; John 7:28; Romans 3:7; Jeremiah 1:6; Galatians 1:10; Deuteronomy 31:27; Ezekiel 39:22; Exodus 29:46; Ezekiel 28:9 | Psalm 46:10 |
| sq2-near-07 | near-quote | isaiah-40-31 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Isaiah 40:31; Isaiah 33:2; Lamentations 3:25; Isaiah 41:1; Psalm 27:14; Lamentations 5:21; Habakkuk 3:19; Psalm 130:6; Psalm 88:4; Psalm 130:5 | Isaiah 40:31 |
| sq2-near-08 | near-quote | jeremiah-29-11 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Jeremiah 29:11; Proverbs 23:18; Job 6:11; Jeremiah 31:17; Proverbs 24:14; Psalm 20:4; Psalm 119:49; Matthew 28:12; 1 Chronicles 28:11; Romans 5:5 | Jeremiah 29:11 |
| sq2-near-09 | near-quote | romans-8-28 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Romans 8:28; Proverbs 12:14; Isaiah 41:23; 2 Corinthians 9:8; Colossians 1:17; Matthew 12:35; 1 Corinthians 12:6; Psalm 133:1; 1 Thessalonians 5:21; 1 Timothy 6:18 | Romans 8:28 |
| sq2-near-10 | near-quote | matthew-11-28 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Matthew 11:28; Isaiah 1:14; Isaiah 43:23; Isaiah 43:24; Proverbs 30:1; Isaiah 43:22; Micah 6:3; Job 4:5; 2 Samuel 15:33; Psalm 38:4 | Matthew 11:28 |
| sq2-drift-01 | translation-drift | philippians-4-13 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Philippians 4:13; 1 Corinthians 8:6; Philippians 1:26; Galatians 6:14; Judges 11:37; Romans 15:18; 2 Corinthians 13:3; Hebrews 13:21; Colossians 1:20; Hebrews 9:11 | Philippians 4:13 |
| sq2-drift-02 | translation-drift | john-3-16 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+dense+phrase) | John 3:16; John 1:12; Acts 7:5; John 1:13; Hebrews 11:17; Hebrews 5:9; John 3:17; Leviticus 7:36; Genesis 22:2; Acts 13:34 | John 3:16 |
| sq2-drift-03 | translation-drift | psalm-23-4 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+phrase) | Psalm 23:4; Isaiah 9:2; Romans 6:4; Job 38:17; Psalm 107:10; Psalm 107:14; Nehemiah 2:15; 1 Corinthians 15:21; Romans 5:12; Matthew 4:16 | Psalm 23:4 |
| sq2-drift-04 | translation-drift | proverbs-3-5 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Proverbs 3:5; Proverbs 20:24; 2 Peter 1:20; Jeremiah 3:15; 2 Peter 3:3; John 13:25; 2 Kings 7:2; Psalm 109:24; Job 8:15; 2 Peter 3:16 | Proverbs 3:5 |
| sq2-drift-05 | translation-drift | psalm-23-5 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Psalm 23:5; Genesis 40:11; Ezekiel 23:33; Luke 22:20; Ezekiel 23:32; 1 Corinthians 10:21; 1 Corinthians 11:25; Psalm 75:8; Isaiah 51:17; Psalm 116:13 | Psalm 23:5 |
| sq2-drift-06 | translation-drift | isaiah-54-17 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Isaiah 54:17; Psalm 83:5; 2 Kings 18:7; Jeremiah 22:7; Luke 12:15; Acts 20:3; Ezekiel 23:24; Numbers 12:8; Isaiah 30:1; 2 Corinthians 10:4 | Isaiah 54:17 |
| sq2-drift-07 | translation-drift | second-timothy-2-15 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | 2 Timothy 2:15; Romans 14:18; Ecclesiastes 9:7; 1 Thessalonians 2:4; John 7:15; 1 Corinthians 11:19; Romans 16:10; 2 Corinthians 10:18; Ecclesiastes 12:12; Ezra 7:10 | 2 Timothy 2:15 |
| sq2-drift-08 | translation-drift | john-14-2 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | John 14:2; Isaiah 5:9; Hebrews 1:1; Luke 16:27; Numbers 20:15; Mark 2:15; 1 Chronicles 7:22; Habakkuk 2:10; 1 Corinthians 4:15; Genesis 17:4 | John 14:2 |
| sq2-meaning-01 | meaning-only | jeremiah-29-11 | Any | 2 | — | ✓ | ✓ | ✓ | n/a | ✓ | n/a | fuse(lexical+dense) | Job 6:8; Jeremiah 29:11; Romans 15:14; Jeremiah 29:12; Job 22:27; 1 Chronicles 17:25; Job 6:9; Psalm 138:8; 1 Corinthians 4:17; Lamentations 3:22 | Jeremiah 29:11 |
| sq2-meaning-02 | meaning-only | isaiah-41-10 | Any | 3 | — | ✓ | ✓ | ✓ | n/a | ✓ | n/a | fuse(lexical+dense) | Psalm 56:11; Job 23:16; Isaiah 41:10; Deuteronomy 31:8; Psalm 56:10; Job 23:15; Deuteronomy 31:6; Psalm 118:6; Jeremiah 42:12; Acts 27:24 | Isaiah 41:10 |
| sq2-meaning-03 | meaning-only | philippians-4-6 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | n/a | fuse(lexical+dense) | Philippians 4:6; Philippians 4:7; 1 Thessalonians 5:17; 1 Thessalonians 5:18; Romans 15:32; Romans 14:22; Isaiah 41:10; Matthew 6:33; Deuteronomy 31:8; Philippians 4:9 | Philippians 4:6 |
| sq2-meaning-04 | meaning-only | romans-8-28 | Any | — | — | — | — | ✓ | n/a | ✓ | n/a | n/a | 1 Corinthians 12:26; Proverbs 14:13; Ecclesiastes 8:6; James 3:2; 1 Corinthians 13:8; Romans 12:18; 1 Thessalonians 5:21; Proverbs 16:5; Proverbs 2:10; 1 Corinthians 13:7 | Romans 8:28 |
| sq2-meaning-05 | meaning-only | first-peter-5-7 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | n/a | fuse(lexical+dense) | 1 Peter 5:7; Romans 12:1; 1 Samuel 12:24; Romans 12:2; Romans 15:14; Hebrews 13:21; Deuteronomy 15:10; 1 Thessalonians 3:8; Deuteronomy 13:4; Isaiah 41:10 | 1 Peter 5:7 |
| sq2-meaning-06 | meaning-only | matthew-11-28 | Any | — | — | — | — | ✓ | n/a | ✓ | n/a | n/a | Romans 15:5; Mark 14:40; Luke 22:45; Mark 14:41; Mark 14:38; John 16:33; John 17:1; Matthew 26:41; 1 Thessalonians 4:14; Obadiah 1:15 | Matthew 11:28 |
| sq2-meaning-07 | meaning-only | second-corinthians-12-9 | Any | 8 | — | — | ✓ | ✓ | n/a | FAIL | n/a | dense | Psalm 18:32; 2 Corinthians 12:10; 2 Samuel 22:34; Philippians 4:13; 2 Corinthians 12:8; Psalm 18:33; Psalm 77:14; 2 Corinthians 12:9; Psalm 18:1; Lamentations 3:19 | 2 Corinthians 12:9 |
| sq2-meaning-08 | meaning-only | romans-12-2 | Any | 5 | — | — | ✓ | ✓ | n/a | ✓ | n/a | dense | Galatians 5:26; Colossians 3:9; Colossians 3:8; Colossians 3:2; Romans 12:2; Titus 2:7; Romans 12:3; James 1:22; Romans 14:13; 1 Corinthians 14:20 | Romans 12:2 |
| sq2-meaning-09 | meaning-only | exodus-34-6 | Any | — | — | — | — | ✓ | n/a | FAIL | n/a | n/a | Numbers 14:18; Nahum 1:3; Nahum 1:4; Job 24:23; Jeremiah 30:24; Hebrews 6:11; Jeremiah 23:20; Psalm 145:8; Psalm 103:9; Joel 2:13 | Exodus 34:6 |
| sq2-meaning-10 | meaning-only | john-14-2 | Any | — | — | — | — | ✓ | n/a | ✓ | n/a | n/a | Psalm 135:2; Acts 1:14; 2 Kings 4:10; Ephesians 2:18; Psalm 115:3; Hebrews 10:20; Psalm 11:4; John 20:10; Ezekiel 42:13; Ezekiel 42:14 | John 14:2 |
| sq2-meaning-11 | meaning-only | luke-15-4-7 | Any | 3 | — | ✓ | ✓ | ✓ | n/a | ✓ | n/a | dense | Jeremiah 10:21; Ezekiel 34:6; Luke 15:5; Genesis 29:3; Ezekiel 34:7; Ezekiel 34:12; Ezekiel 34:5; John 10:14; John 10:12; Exodus 2:18 | Luke 15:4-7 |
| sq2-meaning-12 | meaning-only | genesis-1-1 | Any | 2 | — | ✓ | ✓ | ✓ | n/a | ✓ | n/a | dense | Revelation 11:18; Genesis 1:1; Isaiah 42:5; Hebrews 1:11; Revelation 11:17; Isaiah 57:15; Isaiah 45:9; Hebrews 1:10; Isaiah 45:19; Isaiah 45:8 | Genesis 1:1 |
| sq2-concept-01 | named-concept | fruit-of-spirit | Any | 1 | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | fuse(lexical+alternate+concept) | Galatians 5:22; James 3:18; Ephesians 5:9; Matthew 7:17; Matthew 7:18; Luke 6:43; Matthew 12:33; Matthew 3:8; Ezekiel 47:12; Psalm 148:9 | Galatians 5:22-23 |
| sq2-concept-02 | named-concept | armor-of-god | Any | 1 | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | fuse(lexical+expand+dense+concept) | Ephesians 6:11; Ephesians 6:13; Ephesians 6:12; Psalm 46:7; Ephesians 6:14; Psalm 46:11; Psalm 94:1; Exodus 28:37; Ezekiel 38:4; Psalm 68:19 | Ephesians 6:10-18 |
| sq2-concept-03 | named-concept | ten-commandments | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Deuteronomy 4:13; Romans 13:9; Exodus 34:28; Deuteronomy 1:1; Mark 10:19; Matthew 22:39; Proverbs 7:2; Leviticus 27:34; Deuteronomy 11:27; Deuteronomy 11:21 | Exodus 20:1-17; Deuteronomy 5:6-21 |
| sq2-concept-04 | named-concept | love-one-another | Any | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+dense) | John 15:17; John 13:34; John 15:18; 1 John 4:12; 1 John 3:11; John 15:13; John 15:12; 1 Peter 4:9; 1 Peter 5:14; Romans 12:16 | John 13:34-35; 1 John 4:7-12 |
| sq2-concept-05 | named-concept | body-temple | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | n/a | lexical | 1 Corinthians 6:19; John 2:21; 1 Samuel 31:10; 2 Kings 10:25; 1 Corinthians 15:44; 1 Corinthians 15:40; 1 Corinthians 3:17; Jeremiah 7:4; Luke 11:34; 1 Corinthians 15:38 | 1 Corinthians 6:19-20 |
| sq2-integrity-01 | false-memory | no-more-than-handle | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Deuteronomy 15:6; Deuteronomy 30:5; Joshua 23:9; Psalm 3:2; 2 Corinthians 10:14; 1 Corinthians 10:13; Deuteronomy 1:10; Deuteronomy 7:22; Hebrews 6:11; Deuteronomy 15:7 |  |
| sq2-integrity-02 | false-memory | helps-themselves-aphorism | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Job 21:19; 2 Corinthians 9:7; Acts 20:35; Genesis 2:18; 2 Corinthians 1:4; 2 Corinthians 9:8; Psalm 46:1; Deuteronomy 15:7; Hebrews 2:17; James 5:11 |  |
| sq2-integrity-03 | false-memory | cleanliness-aphorism | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Leviticus 10:10; Luke 11:40; John 13:10; 1 Timothy 6:6; Luke 11:42; 1 Timothy 6:7; 2 Samuel 22:25; Psalm 51:2; Leviticus 17:16; Luke 11:41 |  |
| sq2-integrity-04 | misquote | first-timothy-6-10 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | fuse(lexical+alternate+phrase) | 1 Timothy 6:10; Proverbs 12:12; Ecclesiastes 5:10; 2 Kings 12:4; Romans 11:18; Deuteronomy 14:25; Job 5:3; Psalm 37:35; Luke 22:5; Job 18:16 | 1 Timothy 6:10 |
| sq2-integrity-05 | misquote | isaiah-11-6 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | fuse(lexical+phrase) | Isaiah 11:6; Isaiah 11:7; Psalm 57:4; Jeremiah 5:6; Ezekiel 19:2; Psalm 3:5; Psalm 36:12; Job 11:19; Psalm 104:22; Genesis 49:9 | Isaiah 11:6 |
| sq2-conflation-01 | conflation | philippians-4-13-plus-first-corinthians-10-13 | All | — | — | — | — | ✓ | n/a | n/a | n/a | fuse(lexical+dense) | Philippians 4:13; Psalm 18:32; Philippians 4:12; Philippians 4:14; Psalm 54:4; 2 Corinthians 12:9; Psalm 40:17; Psalm 56:10; 2 Samuel 22:33; Psalm 49:15 | Philippians 4:13; 1 Corinthians 10:13 |
| sq2-conflation-02 | conflation | jeremiah-29-11-plus-romans-8-28 | All | — | — | — | — | ✓ | n/a | n/a | n/a | fuse(lexical+dense) | Jeremiah 29:11; Proverbs 16:3; Jeremiah 29:12; Ephesians 1:10; Philemon 1:7; Philemon 1:6; Philippians 2:3; 1 Corinthians 10:33; Psalm 85:13; Psalm 20:4 | Jeremiah 29:11; Romans 8:28 |
| sq2-conflation-03 | conflation | joshua-1-9-plus-isaiah-41-10 | All | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+dense) | Isaiah 41:10; Joshua 1:9; Isaiah 41:11; Deuteronomy 31:6; Psalm 71:8; Daniel 10:19; Joshua 1:8; Isaiah 35:4; Proverbs 3:26; Psalm 31:24 | Joshua 1:9; Isaiah 41:10 |

## Notes

- **offline-keyword**: same full BSB TEXT, naive multi-term token overlap without stemming, synonym expansion, or phrase memory (proxy for classic keyword Bible search).
- **Live tools** (Bible Gateway, Blue Letter Bible / BibleHub): scores loaded from captured query results when available; environmentally unreachable tools are recorded as such rather than fabricated.
- Family registries require explicit family/split metadata and reject both literal-family and overlapping-passage leakage across split roles.
- `--release-gate` requires a family registry and enforces R@1 ≥ 60%, R@3 ≥ 85%, R@10 ≥ 95%, 100% confidence safety, 100% required structured-passage coverage, 100% hard-negative ordering, and 100% explicitly required confidence labels.
