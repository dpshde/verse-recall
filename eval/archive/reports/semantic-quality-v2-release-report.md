# Bible search red-team report

## Run identity

- Evaluator: v0.1.0 (`/Users/user/Developer/verse-recall/target/release/recall-eval`, 1164432 bytes, FNV-1a `8b819159e4dc2c0b`)
- Base pack: `/Users/user/Developer/verse-recall/data/bsb-full.pack` (6433152 bytes, FNV-1a `42bb9909d5a56a3a`)
- Overlay: `/Users/user/Developer/verse-recall/data/bsb-expand.pack` (5627520 bytes, FNV-1a `2daba354133287df`)
- Overlay: `/Users/user/Developer/verse-recall/data/recall-overlay.pack` (7428864 bytes, FNV-1a `4fbf2c94cc0cf810`)
- Overlay: `/Users/user/Developer/verse-recall/data/bsb-semantic.pack` (23874304 bytes, FNV-1a `f937771db44d9b79`)
- Prompts: `/Users/user/Developer/verse-recall/eval/semantic-quality-v2.jsonl` (12015 bytes, FNV-1a `e5aee9f70cb1681a`)
- Family registry: `/Users/user/Developer/verse-recall/eval/overlay-adversarial-v1.jsonl` (11023 bytes, FNV-1a `06e867bd96632786`)
- Query embeddings: `/private/tmp/bge-semantic-v2-query.jsonl` (348730 bytes, FNV-1a `682a9bc0e0394093`)
- Embedding space ID: `8744273cd632ff40dc59a2c17bae871a5eebf8ed51a8e96b815c5c65c9672225`
- Parameters: depth=10, split=None, baseline=false, release_gate=true, registry_count=1

Suite size: **43** prompts. Empty-gold safety cases are excluded from retrieval denominators.

Metrics: **Recall@1/3/10** and **MRR** over gold-bearing prompts; `gold_mode=all` requires every split/conflated gold; **confidence safety** requires every top-3 `Recovered` result to overlap gold (and forbids it for empty-gold prompts); named concepts may require all declared structured passages; hard negatives must rank below the completed gold; gold-bearing misquotes may require an explicit confidence label.

## Overall

| System | Retrieval n | R@1 | R@3 | R@10 | MRR | Confidence safety | Passage output | Hard negatives | Required labels |
|--------|------------:|----:|----:|-----:|----:|------------------:|---------------:|---------------:|----------------:|
| verse-recall | 40 | 77.5% | 85.0% | 95.0% | 0.825 | 100.0% (43/43) | 100.0% (2/2) | 100.0% (16/16) | 100.0% (2/2) |

## By category

| Category | System | n | R@1 | R@3 | R@10 | MRR |
|----------|--------|--:|----:|----:|-----:|----:|
| conflation | verse-recall | 3 | 0.0% | 33.3% | 66.7% | 0.208 |
| false-memory | verse-recall | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| meaning-only | verse-recall | 12 | 50.0% | 66.7% | 91.7% | 0.615 |
| misquote | verse-recall | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| named-concept | verse-recall | 5 | 100.0% | 100.0% | 100.0% | 1.000 |
| near-quote | verse-recall | 10 | 100.0% | 100.0% | 100.0% | 1.000 |
| translation-drift | verse-recall | 8 | 100.0% | 100.0% | 100.0% | 1.000 |

## By family (verse-recall)

| Family | n | R@1 | R@3 | R@10 | MRR |
|--------|--:|----:|----:|-----:|----:|
| armor-of-god | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| body-temple | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| cleanliness-aphorism | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| exodus-34-6 | 1 | 0.0% | 0.0% | 100.0% | 0.200 |
| first-corinthians-13-4 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| first-peter-5-7 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| first-timothy-6-10 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| fruit-of-spirit | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| genesis-1-1 | 1 | 0.0% | 100.0% | 100.0% | 0.333 |
| helps-themselves-aphorism | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| isaiah-11-6 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| isaiah-40-31 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| isaiah-41-10 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| isaiah-54-17 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| jeremiah-29-11 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| jeremiah-29-11-plus-romans-8-28 | 1 | 0.0% | 100.0% | 100.0% | 0.500 |
| john-14-2 | 2 | 50.0% | 50.0% | 100.0% | 0.625 |
| john-3-16 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| joshua-1-9-plus-isaiah-41-10 | 1 | 0.0% | 0.0% | 100.0% | 0.125 |
| love-one-another | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| luke-15-4-7 | 1 | 0.0% | 100.0% | 100.0% | 0.500 |
| matthew-11-28 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
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
| romans-12-2 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| romans-8-28 | 2 | 50.0% | 50.0% | 100.0% | 0.550 |
| second-corinthians-12-9 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| second-timothy-2-15 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| ten-commandments | 1 | 100.0% | 100.0% | 100.0% | 1.000 |

## Per-prompt (verse-recall)

| id | category | family | gold mode | rank | R@1 | R@3 | R@10 | safe | passage | hard neg | label | lane used | top | gold |
|----|----------|--------|-----------|-----:|-----|-----|------|------|---------|----------|-------|-----------|-----|------|
| sq2-near-01 | near-quote | nehemiah-8-10 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Nehemiah 8:10; 1 Chronicles 16:27; Psalm 81:1; Psalm 18:1; Psalm 29:1; Psalm 29:11; Psalm 28:7; Isaiah 40:31; Psalm 118:14; Psalm 150:6 | Nehemiah 8:10 |
| sq2-near-02 | near-quote | first-corinthians-13-4 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | 1 Corinthians 13:4; Psalm 86:5; Psalm 109:21; Psalm 27:14; Genesis 37:4; Genesis 27:9; Psalm 119:103; Romans 15:7; 1 Corinthians 13:5; 2 Corinthians 9:7 | 1 Corinthians 13:4 |
| sq2-near-03 | near-quote | psalm-23-1 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | n/a | fuse(lexical+alternate+phrase) | Psalm 23:1; Ezekiel 34:7; Ezekiel 34:9; Ezekiel 34:2; Zechariah 11:15; Jeremiah 23:1; Psalm 23:2; Psalm 99:5; Psalm 118:1; John 4:24 | Psalm 23:1 |
| sq2-near-04 | near-quote | john-3-16 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+phrase) | John 3:16; 1 John 4:9; James 2:5; 1 John 2:15; 1 John 3:1; John 15:19; John 14:2; Psalm 126:4; Proverbs 17:22; Psalm 47:7 | John 3:16 |
| sq2-near-05 | near-quote | philippians-4-13 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | n/a | fuse(lexical+alternate) | Philippians 4:13; 1 Corinthians 8:6; Hebrews 13:21; Hebrews 9:11; 1 Peter 4:11; Colossians 1:20; Ephesians 4:29; Philippians 4:14; Hebrews 4:12; Genesis 1:27 | Philippians 4:13 |
| sq2-near-06 | near-quote | psalm-46-10 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | n/a | fuse(lexical+alternate) | Psalm 46:10; John 7:28; Exodus 9:30; Romans 3:7; Jeremiah 1:6; Deuteronomy 31:27; John 20:29; Psalm 25:6; Mark 11:24; Psalm 83:1 | Psalm 46:10 |
| sq2-near-07 | near-quote | isaiah-40-31 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Isaiah 40:31; Isaiah 33:2; Isaiah 41:1; Lamentations 3:25; Psalm 27:14; Lamentations 5:21; Psalm 27:13; James 4:8; Psalm 84:7; Matthew 6:33 | Isaiah 40:31 |
| sq2-near-08 | near-quote | jeremiah-29-11 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Jeremiah 29:11; Proverbs 23:18; Job 6:11; Jeremiah 31:17; Proverbs 24:14; Psalm 20:4; John 15:7; Proverbs 23:19; Psalm 37:4; Jeremiah 29:12 | Jeremiah 29:11 |
| sq2-near-09 | near-quote | romans-8-28 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Romans 8:28; Proverbs 12:14; Isaiah 41:23; 2 Corinthians 9:8; Colossians 1:17; Psalm 133:1; Matthew 6:19; 1 Corinthians 12:6; Acts 16:16; 1 Corinthians 12:7 | Romans 8:28 |
| sq2-near-10 | near-quote | matthew-11-28 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Matthew 11:28; Isaiah 1:14; Isaiah 43:23; Isaiah 43:24; Proverbs 30:1; Micah 6:3; Psalm 127:2; Matthew 11:29; Psalm 69:4; Psalm 55:2 | Matthew 11:28 |
| sq2-drift-01 | translation-drift | philippians-4-13 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Philippians 4:13; 1 Corinthians 8:6; Philippians 1:26; Romans 15:18; Judges 11:37; Galatians 6:14; Genesis 1:27; Philippians 4:14; 2 Timothy 3:16; Ephesians 4:29 | Philippians 4:13 |
| sq2-drift-02 | translation-drift | john-3-16 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | John 3:16; 1 Chronicles 25:5; Genesis 30:23; Job 38:28; 1 John 5:11; 1 John 5:9; John 1:13; Acts 7:5; Revelation 3:21; Psalm 37:25 | John 3:16 |
| sq2-drift-03 | translation-drift | psalm-23-4 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+phrase) | Psalm 23:4; Isaiah 9:2; Romans 6:4; Psalm 107:14; Job 38:17; Psalm 107:10; Psalm 91:11; Psalm 23:5; John 14:31; Psalm 23:3 | Psalm 23:4 |
| sq2-drift-04 | translation-drift | proverbs-3-5 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Proverbs 3:5; Proverbs 20:24; 2 Peter 1:20; Jeremiah 3:15; 2 Peter 3:3; John 13:25; 2 Corinthians 10:12; James 1:23; Proverbs 16:23; Proverbs 8:33 | Proverbs 3:5 |
| sq2-drift-05 | translation-drift | psalm-23-5 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Psalm 23:5; Genesis 40:11; Ezekiel 23:33; Luke 22:20; Ezekiel 23:32; 1 Corinthians 10:21; Ezekiel 23:31; Proverbs 5:15; Lamentations 4:21; Jeremiah 25:16 | Psalm 23:5 |
| sq2-drift-06 | translation-drift | isaiah-54-17 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Isaiah 54:17; Psalm 83:5; 2 Kings 18:7; Jeremiah 22:7; Luke 12:15; Acts 20:3; Psalm 108:12; Isaiah 54:16; Exodus 20:13; Ezekiel 21:13 | Isaiah 54:17 |
| sq2-drift-07 | translation-drift | second-timothy-2-15 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | 2 Timothy 2:15; Romans 14:18; 1 Thessalonians 2:4; Ecclesiastes 9:7; John 7:15; Romans 16:10; Romans 12:2; Hebrews 13:21; 1 Peter 1:7; Romans 15:14 | 2 Timothy 2:15 |
| sq2-drift-08 | translation-drift | john-14-2 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | John 14:2; Isaiah 5:9; Hebrews 1:1; Luke 16:27; Mark 2:15; Numbers 20:15; Isaiah 32:18; Ecclesiastes 2:8; Hebrews 3:4; 2 Chronicles 6:7 | John 14:2 |
| sq2-meaning-01 | meaning-only | jeremiah-29-11 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | n/a | fuse(lexical+expand+phrase) | Jeremiah 29:11; Hebrews 11:20; Romans 8:38; 1 Corinthians 3:22; 1 Timothy 6:19; Acts 27:43; Job 6:8; John 20:29; John 14:2; Romans 15:14 | Jeremiah 29:11 |
| sq2-meaning-02 | meaning-only | isaiah-41-10 | Any | — | — | — | — | ✓ | n/a | ✓ | n/a | n/a | John 16:32; Job 7:19; John 8:29; John 8:16; Job 7:16; Mark 14:6; Psalm 56:11; John 20:29; Job 23:16; Mark 11:24 | Isaiah 41:10 |
| sq2-meaning-03 | meaning-only | philippians-4-6 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | n/a | dense | Philippians 4:6; 1 Thessalonians 5:17; Philippians 4:7; Psalm 61:1; Acts 24:19; Philemon 1:4; Matthew 6:34; Psalm 84:8; Psalm 66:19; Hebrews 11:6 | Philippians 4:6 |
| sq2-meaning-04 | meaning-only | romans-8-28 | Any | 10 | — | — | ✓ | ✓ | n/a | ✓ | n/a | dense | Psalm 23:6; 1 Corinthians 12:26; Proverbs 14:13; Matthew 7:17; Matthew 7:18; Titus 2:7; Luke 6:43; Jeremiah 24:3; Matthew 12:33; Romans 8:28 | Romans 8:28 |
| sq2-meaning-05 | meaning-only | first-peter-5-7 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | n/a | fuse(lexical+expand) | 1 Peter 5:7; 1 Chronicles 5:20; Genesis 39:23; Matthew 10:19; Matthew 6:34; Jeremiah 30:17; John 13:34; Isaiah 41:10; Romans 12:1; 1 Samuel 12:24 | 1 Peter 5:7 |
| sq2-meaning-06 | meaning-only | matthew-11-28 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | n/a | dense | Matthew 11:28; Mark 14:40; Romans 15:5; Luke 22:45; Judges 8:5; Exodus 33:14; 2 Chronicles 20:30; Hebrews 4:8; Galatians 6:9; Psalm 4:8 | Matthew 11:28 |
| sq2-meaning-07 | meaning-only | second-corinthians-12-9 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | n/a | dense | 2 Corinthians 12:9; Psalm 18:32; 2 Timothy 1:7; Acts 8:10; Daniel 10:8; Psalm 88:4; Job 26:2; Psalm 82:1; Judges 6:14; 2 Samuel 22:34 | 2 Corinthians 12:9 |
| sq2-meaning-08 | meaning-only | romans-12-2 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | n/a | dense | Romans 12:2; Galatians 5:26; Colossians 3:9; 1 Samuel 15:29; Luke 5:22; 1 Corinthians 14:20; Job 9:27; Numbers 23:20; Malachi 3:6; Colossians 3:8 | Romans 12:2 |
| sq2-meaning-09 | meaning-only | exodus-34-6 | Any | 5 | — | — | ✓ | ✓ | n/a | ✓ | n/a | fuse(lexical+expand) | 1 Thessalonians 3:12; Ephesians 4:26; John 3:16; Psalm 86:15; Exodus 34:6; Psalm 103:8; Numbers 14:18; Proverbs 17:22; Nahum 1:4; 1 Timothy 4:8 | Exodus 34:6 |
| sq2-meaning-10 | meaning-only | john-14-2 | Any | 4 | — | — | ✓ | ✓ | n/a | ✓ | n/a | fuse(lexical+expand) | Psalm 135:2; Isaiah 32:18; Acts 1:14; John 14:2; Psalm 68:12; Acts 20:8; Psalm 119:84; Hebrews 1:1; Genesis 24:23; Hebrews 3:4 | John 14:2 |
| sq2-meaning-11 | meaning-only | luke-15-4-7 | Any | 2 | — | ✓ | ✓ | ✓ | n/a | ✓ | n/a | dense | Matthew 7:6; Luke 15:5; Jeremiah 10:21; Leviticus 27:32; John 10:12; Jeremiah 25:35; Ezekiel 34:8; 1 Samuel 30:20; Jeremiah 23:4; Joel 2:25 | Luke 15:4-7 |
| sq2-meaning-12 | meaning-only | genesis-1-1 | Any | 3 | — | ✓ | ✓ | ✓ | n/a | ✓ | n/a | dense | Isaiah 40:28; Revelation 11:18; Genesis 1:1; Exodus 28:13; Isaiah 44:10; Hebrews 11:3; Hebrews 1:2; Revelation 1:8; Psalm 119:73; John 20:29 | Genesis 1:1 |
| sq2-concept-01 | named-concept | fruit-of-spirit | Any | 1 | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | fuse(lexical+alternate+concept) | Galatians 5:22; Matthew 7:17; Matthew 7:18; Luke 6:43; Matthew 12:33; Psalm 148:9; Jeremiah 17:8; Romans 8:8; Galatians 5:23; John 15:5 | Galatians 5:22-23 |
| sq2-concept-02 | named-concept | armor-of-god | Any | 1 | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | fuse(lexical+expand+concept) | Ephesians 6:11; Ephesians 6:13; 1 Samuel 14:13; Judges 9:54; 2 Chronicles 26:14; 2 Samuel 18:15; Psalm 91:1; Ephesians 6:12; 1 John 5:18; Isaiah 54:17 | Ephesians 6:10-18 |
| sq2-concept-03 | named-concept | ten-commandments | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+concept) | Exodus 20:6; Deuteronomy 4:13; Exodus 34:28; Deuteronomy 10:4; 1 Kings 7:43; Exodus 27:12; Romans 13:9; Deuteronomy 1:1; Matthew 22:39; Mark 10:19 | Exodus 20:1-17; Deuteronomy 5:6-21 |
| sq2-concept-04 | named-concept | love-one-another | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | dense | John 13:34; John 15:17; John 15:18; John 15:12; 1 Peter 4:8; 1 John 4:11; 1 John 4:7; Galatians 1:10; Matthew 5:16; 1 John 4:12 | John 13:34-35; 1 John 4:7-12 |
| sq2-concept-05 | named-concept | body-temple | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | n/a | fuse(lexical+expand) | 1 Corinthians 6:19; John 2:21; 1 Samuel 31:10; 2 Kings 10:25; 1 Corinthians 15:44; 1 Corinthians 15:40; 1 Corinthians 6:20; Psalm 48:9; 1 Corinthians 3:17; Song Of Solomon 7:5 | 1 Corinthians 6:19-20 |
| sq2-integrity-01 | false-memory | no-more-than-handle | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Deuteronomy 15:6; Philippians 4:19; Deuteronomy 30:5; Colossians 2:21; Romans 11:35; Ezekiel 27:29; Exodus 18:18; Psalm 84:8; Acts 11:17; James 1:17 |  |
| sq2-integrity-02 | false-memory | helps-themselves-aphorism | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Romans 13:2; Daniel 11:39; 1 Corinthians 12:28; Titus 3:8; Proverbs 23:20; Hebrews 2:18; Job 21:19; Philippians 2:3; 2 Corinthians 9:7; 1 Timothy 5:8 |  |
| sq2-integrity-03 | false-memory | cleanliness-aphorism | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Leviticus 10:10; Proverbs 3:6; Luke 11:40; 1 Timothy 6:6; 1 Timothy 4:7; 2 Peter 1:7; 2 Timothy 3:5; 2 Peter 1:6; 1 Timothy 6:11; Revelation 21:1 |  |
| sq2-integrity-04 | misquote | first-timothy-6-10 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | fuse(lexical+alternate+phrase) | 1 Timothy 6:10; Proverbs 12:12; 2 Kings 12:4; Romans 11:18; Job 5:3; Deuteronomy 14:25; Romans 12:21; Luke 10:19; 1 Timothy 6:9; 1 Thessalonians 5:22 | 1 Timothy 6:10 |
| sq2-integrity-05 | misquote | isaiah-11-6 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | fuse(lexical+expand+phrase) | Isaiah 11:6; Isaiah 11:7; Psalm 57:4; Ezekiel 19:2; Jeremiah 5:6; Genesis 49:9; Proverbs 12:10; Psalm 23:1; Isaiah 65:24; Proverbs 30:30 | Isaiah 11:6 |
| sq2-conflation-01 | conflation | philippians-4-13-plus-first-corinthians-10-13 | All | — | — | — | — | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Philippians 4:13; Job 33:4; Genesis 30:31; Matthew 16:23; Acts 5:32; Colossians 1:25; Psalm 134:2; Genesis 1:27; Philippians 4:12; Psalm 18:32 | Philippians 4:13; 1 Corinthians 10:13 |
| sq2-conflation-02 | conflation | jeremiah-29-11-plus-romans-8-28 | All | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Romans 8:28; Jeremiah 29:11; Ephesians 1:10; Deuteronomy 1:23; Proverbs 12:14; 3 John 1:2; Matthew 6:19; Jeremiah 29:12; Acts 16:16; Proverbs 16:3 | Jeremiah 29:11; Romans 8:28 |
| sq2-conflation-03 | conflation | joshua-1-9-plus-isaiah-41-10 | All | 8 | — | — | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand); dense | 2 Chronicles 32:7; Psalm 56:3; Joshua 10:25; Joshua 1:9; 1 Chronicles 22:13; Deuteronomy 31:6; Proverbs 3:26; Isaiah 41:10; Psalm 112:7; Isaiah 41:11 | Joshua 1:9; Isaiah 41:10 |

## Notes

- **offline-keyword**: same full BSB TEXT, naive multi-term token overlap without stemming, synonym expansion, or phrase memory (proxy for classic keyword Bible search).
- **Live tools** (Bible Gateway, Blue Letter Bible / BibleHub): scores loaded from captured query results when available; environmentally unreachable tools are recorded as such rather than fabricated.
- Family registries require explicit family/split metadata and reject both literal-family and overlapping-passage leakage across split roles.
- `--release-gate` requires a family registry and enforces R@1 ≥ 60%, R@3 ≥ 85%, R@10 ≥ 95%, 100% confidence safety, 100% required structured-passage coverage, 100% hard-negative ordering, and 100% explicitly required confidence labels.
