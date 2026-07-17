# Bible search red-team report

## Run identity

- Evaluator: v0.1.0 (`/Users/user/Developer/verse-recall/target/release/recall-eval`, 982304 bytes, FNV-1a `561b78a0f9a9dee3`)
- Base pack: `/Users/user/Developer/verse-recall/data/bsb-full.pack` (4730432 bytes, FNV-1a `319bcf4110e62517`)
- Overlay: `/private/tmp/verse-recall-overlay-final.pack` (10366528 bytes, FNV-1a `0f7fc91b7b4e8ba2`)
- Prompts: `/Users/user/Developer/verse-recall/eval/redteam-prompts.jsonl` (15913 bytes, FNV-1a `0959a2e627648b1c`)
- Parameters: depth=10, split=None, baseline=false, release_gate=false, registry_count=0

Suite size: **66** prompts. Empty-gold safety cases are excluded from retrieval denominators.

Metrics: **Recall@1/3/10** and **MRR** over gold-bearing prompts; `gold_mode=all` requires every split/conflated gold; **confidence safety** requires every top-3 `Recovered` result to overlap gold (and forbids it for empty-gold prompts); named concepts may require all declared structured passages; hard negatives must rank below the completed gold; gold-bearing misquotes may require an explicit confidence label.

## Overall

| System | Retrieval n | R@1 | R@3 | R@10 | MRR | Confidence safety | Passage output | Hard negatives | Required labels |
|--------|------------:|----:|----:|-----:|----:|------------------:|---------------:|---------------:|----------------:|
| verse-recall | 63 | 73.0% | 79.4% | 82.5% | 0.762 | 100.0% (66/66) | 0.0% (0/0) | 0.0% (0/0) | 100.0% (2/2) |

## By category

| Category | System | n | R@1 | R@3 | R@10 | MRR |
|----------|--------|--:|----:|----:|-----:|----:|
| conflated | verse-recall | 3 | 0.0% | 0.0% | 33.3% | 0.037 |
| false-memory | verse-recall | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| keyword | verse-recall | 10 | 40.0% | 40.0% | 40.0% | 0.400 |
| meaning-only | verse-recall | 8 | 50.0% | 62.5% | 75.0% | 0.588 |
| named-concept | verse-recall | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| partial-phrase | verse-recall | 26 | 92.3% | 100.0% | 100.0% | 0.955 |
| topical | verse-recall | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| truncation | verse-recall | 7 | 85.7% | 100.0% | 100.0% | 0.905 |
| wrong-translation | verse-recall | 5 | 80.0% | 80.0% | 80.0% | 0.800 |

## By family (verse-recall)

| Family | n | R@1 | R@3 | R@10 | MRR |
|--------|--:|----:|----:|-----:|----:|
| no-gold:rt-falsemem-02 | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| no-gold:rt-falsemem-03 | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| no-gold:rt-falsemem-05 | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| passage:1-corinthians-13-1-john-3-16-john-15-13 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| passage:1-corinthians-13-13 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:1-corinthians-13-13-ephesians-6-11 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| passage:1-corinthians-13-4 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:1-corinthians-6-19 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:1-peter-5-7 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:2-chronicles-7-14 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:2-corinthians-12-9 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:colossians-3-13-1-john-1-9-ephesians-4-32 | 2 | 0.0% | 0.0% | 0.0% | 0.000 |
| passage:ephesians-2-8 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:ephesians-2-8-2-corinthians-12-9-romans-3-24 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| passage:exodus-20-1-17-deuteronomy-5-1-21-deuteronomy-4-13 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:galatians-5-22 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:hebrews-11-1 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:hebrews-11-1-hebrews-11-6-ephesians-2-8 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| passage:isaiah-11-6 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:isaiah-40-31 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:isaiah-41-10 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:isaiah-41-10-philippians-4-13-psalm-28-7 | 1 | 0.0% | 0.0% | 100.0% | 0.200 |
| passage:isaiah-41-10-psalm-23-4 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| passage:isaiah-53-5-jeremiah-17-14-psalm-103-3 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:isaiah-54-17 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:jeremiah-17-14-isaiah-53-5-psalm-103-3 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:jeremiah-29-11 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:john-13-34 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:john-14-2 | 2 | 50.0% | 100.0% | 100.0% | 0.750 |
| passage:john-14-26-acts-2-4-acts-19-2-galatians-5-22 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:john-14-27-philippians-4-7-romans-5-1 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| passage:john-14-6 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:joshua-1-9-deuteronomy-31-6-2-chronicles-32-7 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:joshua-24-15 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:matthew-11-28 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:matthew-5-4-psalm-34-18-2-corinthians-1-3 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| passage:matthew-6-33 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:nehemiah-8-10 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:nehemiah-8-10-philippians-4-4-john-15-11 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:philippians-4-13 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:philippians-4-6 | 2 | 0.0% | 100.0% | 100.0% | 0.333 |
| passage:philippians-4-6-1-peter-5-7-psalm-94-19 | 3 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:philippians-4-6-isaiah-41-10-philippians-4-7 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| passage:philippians-4-6-matthew-6-6-james-5-16 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| passage:proverbs-13-24 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:psalm-103-8-exodus-34-6 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:psalm-23-1-john-3-16 | 1 | 0.0% | 0.0% | 100.0% | 0.111 |
| passage:psalm-46-10 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:psalm-91-1-isaiah-41-10-psalm-59-1 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:romans-12-2 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| passage:romans-15-13-hebrews-11-1-romans-8-24 | 2 | 50.0% | 100.0% | 100.0% | 0.750 |
| passage:romans-8-28 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |

## Per-prompt (verse-recall)

| id | category | family | gold mode | rank | R@1 | R@3 | R@10 | safe | passage | hard neg | label | lane used | top | gold |
|----|----------|--------|-----------|-----:|-----|-----|------|------|---------|----------|-------|-----------|-----|------|
| rt-phrase-01 | partial-phrase | passage:nehemiah-8-10 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Nehemiah 8:10; 1 Chronicles 16:27; Psalm 81:1; Psalm 18:1; Psalm 29:1; Psalm 29:11; 1 Chronicles 16:11; Psalm 105:4; 1 Chronicles 16:28; Psalm 96:7 | Nehemiah 8:10 |
| rt-phrase-02 | partial-phrase | passage:john-13-34 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | John 13:34; John 15:12; 1 Peter 4:8; 1 John 4:11; John 15:17; 1 John 4:7; 1 John 4:12; John 13:35; 1 John 3:11; 1 Thessalonians 4:9 | John 13:34 |
| rt-phrase-03 | partial-phrase | passage:philippians-4-13 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Philippians 4:13; 1 Corinthians 8:6; Hebrews 13:21; Hebrews 9:11; 1 Peter 4:11; Colossians 1:20; John 1:17; 2 Corinthians 3:4; 2 Corinthians 1:5; Romans 5:1 | Philippians 4:13 |
| rt-phrase-04 | partial-phrase | passage:jeremiah-29-11 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Jeremiah 29:11; Job 42:2; Micah 4:12; Isaiah 29:15; Isaiah 5:19; Proverbs 20:18; Proverbs 16:9; John 12:10; Proverbs 15:22; Proverbs 16:3 | Jeremiah 29:11 |
| rt-phrase-05 | partial-phrase | passage:romans-8-28 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Romans 8:28; Proverbs 12:14; Isaiah 41:23; 2 Corinthians 9:8; Colossians 1:17; Matthew 12:35; 1 Corinthians 12:6; Psalm 133:1; 1 Thessalonians 5:21; 1 Timothy 6:18 | Romans 8:28 |
| rt-phrase-06 | partial-phrase | passage:philippians-4-6 | Any | 3 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Job 20:2; Daniel 2:3; Philippians 4:6; Philippians 2:28; Isaiah 35:4; 1 Corinthians 10:19; Luke 2:48; Proverbs 30:30; Ezekiel 4:16; Judges 13:14 | Philippians 4:6 |
| rt-phrase-07 | partial-phrase | passage:joshua-1-9-deuteronomy-31-6-2-chronicles-32-7 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | 2 Chronicles 32:7; Psalm 31:24; Psalm 27:14; Joshua 1:6; 2 Samuel 2:7; Joshua 10:25; Joshua 1:9; Joshua 1:18; 1 Chronicles 22:13; Deuteronomy 31:6 | Joshua 1:9; Deuteronomy 31:6; 2 Chronicles 32:7 |
| rt-phrase-08 | partial-phrase | passage:psalm-46-10 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Psalm 46:10; Exodus 9:30; John 7:28; Romans 3:7; Jeremiah 1:6; Galatians 1:10; Deuteronomy 31:27; Ezekiel 39:22; Exodus 29:46; Ezekiel 28:9 | Psalm 46:10 |
| rt-phrase-09 | partial-phrase | passage:matthew-11-28 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Matthew 11:28; Isaiah 43:22; Micah 6:3; Job 4:5; Isaiah 1:14; Isaiah 43:23; Isaiah 43:24; Isaiah 47:13; Isaiah 50:4; John 6:37 | Matthew 11:28 |
| rt-phrase-10 | partial-phrase | passage:1-peter-5-7 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | 1 Peter 5:7; Isaiah 44:10; Matthew 27:35; Psalm 94:19; Jonah 1:15; Proverbs 12:25; Mark 15:24; 2 Samuel 18:17; Ezekiel 12:18; John 8:7 | 1 Peter 5:7 |
| rt-phrase-11 | partial-phrase | passage:2-corinthians-12-9 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | 2 Corinthians 12:9; 2 Corinthians 2:6; Isaiah 40:16; John 1:16; Romans 11:6; Leviticus 25:21; Hebrews 13:25; James 4:6; 1 Corinthians 15:10; 1 Corinthians 16:23 | 2 Corinthians 12:9 |
| rt-phrase-12 | partial-phrase | passage:matthew-6-33 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Matthew 6:33; Luke 12:31; Daniel 9:1; Hosea 2:7; Matthew 21:31; 2 Chronicles 36:22; Ezra 1:1; 1 Chronicles 16:11; Psalm 27:8; Psalm 105:4 | Matthew 6:33 |
| rt-phrase-13 | partial-phrase | passage:romans-12-2 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Romans 12:2; Ephesians 2:2; 1 Peter 1:14; Exodus 25:9; Exodus 25:40; Exodus 26:30; Philippians 3:10; John 1:10; Philippians 3:17; 1 John 4:5 | Romans 12:2 |
| rt-phrase-14 | partial-phrase | passage:isaiah-54-17 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Isaiah 54:17; Psalm 83:5; Jeremiah 22:7; Luke 12:15; Acts 20:3; Ezekiel 23:24; Numbers 12:8; Isaiah 30:1; 2 Corinthians 10:4; 2 Chronicles 25:27 | Isaiah 54:17 |
| rt-phrase-15 | partial-phrase | passage:isaiah-40-31 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Isaiah 40:31; Lamentations 3:25; Psalm 125:5; Psalm 27:14; Psalm 130:6; Ezekiel 11:21; Psalm 130:5; Psalm 71:10; Revelation 6:10; Habakkuk 3:19 | Isaiah 40:31 |
| rt-phrase-16 | partial-phrase | passage:john-14-6 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | John 14:6; Numbers 22:30; Psalm 119:30; Jeremiah 26:3; Acts 26:25; John 6:48; 2 Peter 2:2; Romans 9:1; Psalm 86:11; John 14:4 | John 14:6 |
| rt-phrase-17 | partial-phrase | passage:joshua-24-15 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Joshua 24:15; Song Of Solomon 2:4; Jeremiah 5:11; 1 Chronicles 17:12; Psalm 119:54; Ezekiel 3:24; Genesis 39:15; Psalm 69:9; John 2:17; 1 Samuel 2:30 | Joshua 24:15 |
| rt-phrase-18 | partial-phrase | passage:1-corinthians-6-19 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | 1 Corinthians 6:19; John 2:21; 1 Samuel 31:10; 2 Kings 10:25; 1 Corinthians 15:44; 1 Corinthians 15:40; 1 Corinthians 3:17; Jeremiah 7:4; Luke 11:34; 1 Corinthians 15:38 | 1 Corinthians 6:19 |
| rt-phrase-19 | partial-phrase | passage:john-14-2 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | John 14:2; Genesis 24:23; Acts 20:8; Hebrews 1:1; 1 Chronicles 9:26; Luke 22:11; Luke 16:27; Numbers 20:15; Mark 14:14; 1 Chronicles 28:12 | John 14:2 |
| rt-phrase-20 | partial-phrase | passage:psalm-103-8-exodus-34-6 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Psalm 103:8; Psalm 145:8; Psalm 86:15; Exodus 34:6; Joel 2:13; Numbers 14:18; Jonah 4:2; James 1:19; Proverbs 15:18; Nehemiah 9:17 | Psalm 103:8; Exodus 34:6 |
| rt-phrase-21 | partial-phrase | passage:2-chronicles-7-14 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | 2 Chronicles 7:14; Psalm 116:4; Lamentations 3:55; Psalm 147:4; Acts 2:21; Romans 10:13; Deuteronomy 25:10; Psalm 116:13; Psalm 99:6; Psalm 116:17 | 2 Chronicles 7:14 |
| rt-phrase-22 | partial-phrase | passage:1-corinthians-13-13 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | 1 Corinthians 13:13; 1 Thessalonians 1:3; Colossians 1:5; 1 Thessalonians 5:8; Galatians 5:5; Hebrews 11:1; Psalm 33:22; Psalm 147:11; Titus 2:2; Philemon 1:5 | 1 Corinthians 13:13 |
| rt-phrase-23 | partial-phrase | passage:isaiah-41-10 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Isaiah 41:10; Psalm 56:3; Job 23:15; Genesis 50:19; Hebrews 12:21; Psalm 119:63; Job 9:35; Jeremiah 42:11; Jeremiah 1:8; Leviticus 25:17 | Isaiah 41:10 |
| rt-phrase-24 | partial-phrase | passage:1-corinthians-13-4 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | 1 Corinthians 13:4; Psalm 86:5; Psalm 109:21; Psalm 27:14; Genesis 37:4; Genesis 27:9; 1 Timothy 6:10; Luke 21:19; Hebrews 6:15; Romans 12:12 | 1 Corinthians 13:4 |
| rt-phrase-25 | named-concept | passage:exodus-20-1-17-deuteronomy-5-1-21-deuteronomy-4-13 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Deuteronomy 4:13; Exodus 34:28; Deuteronomy 10:4; 1 Kings 7:43; Exodus 27:12; Luke 19:13; Romans 7:11; 1 John 5:3; 2 Kings 20:10; Luke 19:25 | Exodus 20:1-17; Deuteronomy 5:1-21; Deuteronomy 4:13 |
| rt-phrase-26 | topical | passage:john-14-26-acts-2-4-acts-19-2-galatians-5-22 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Acts 19:2; Jude 1:20; Luke 1:67; Acts 8:15; Acts 13:52; Hebrews 3:7; Hebrews 10:15; Luke 4:1; Acts 2:4; Mark 1:8 | John 14:26; Acts 2:4; Acts 19:2; Galatians 5:22 |
| rt-phrase-27 | partial-phrase | passage:galatians-5-22 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate+concept) | Galatians 5:22; James 3:18; Ephesians 5:9; Matthew 7:17; Matthew 7:18; Luke 6:43; Matthew 12:33; Matthew 3:8; Ezekiel 47:12; Psalm 148:9 | Galatians 5:22 |
| rt-phrase-28 | partial-phrase | passage:john-14-2 | Any | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Judges 6:15; John 14:2; Luke 2:49; Luke 16:27; Psalm 45:10; 1 Samuel 22:16; Genesis 28:21; Genesis 38:11; Genesis 31:14; 1 Chronicles 28:4 | John 14:2 |
| rt-trunc-01 | truncation | passage:nehemiah-8-10 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Nehemiah 8:10; Psalm 86:4; Psalm 126:3; Philippians 2:29; Isaiah 35:10; Isaiah 51:11; Psalm 47:5; Psalm 32:11; Psalm 98:6; Psalm 95:1 | Nehemiah 8:10 |
| rt-trunc-02 | truncation | passage:jeremiah-29-11 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Jeremiah 29:11; 3 John 1:2; Jeremiah 29:7; Proverbs 20:18; Psalm 122:6; Proverbs 16:9; John 12:10; Psalm 118:25; Proverbs 11:25; Proverbs 15:22 | Jeremiah 29:11 |
| rt-trunc-03 | truncation | passage:philippians-4-6 | Any | 3 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Job 20:2; Daniel 2:3; Philippians 4:6; Philippians 2:28; Isaiah 35:4; Luke 2:48; Ezekiel 4:16 | Philippians 4:6 |
| rt-trunc-04 | truncation | passage:psalm-46-10 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Psalm 46:10; Job 9:28; Exodus 9:30; Luke 22:60; John 7:28; Genesis 46:30; Ecclesiastes 8:12; Deuteronomy 31:27; John 14:9; Genesis 43:7 | Psalm 46:10 |
| rt-trunc-05 | truncation | passage:isaiah-54-17 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Isaiah 54:17; 2 Corinthians 10:4; Ezekiel 1:5; 1 Thessalonians 5:22; Genesis 49:5; 2 Samuel 1:27; Psalm 7:13; Job 20:24; Ecclesiastes 9:18; 1 Timothy 2:13 | Isaiah 54:17 |
| rt-trunc-06 | truncation | passage:1-peter-5-7 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | 1 Peter 5:7; Jeremiah 7:15; Proverbs 18:18; Lamentations 3:31; Psalm 2:3; Deuteronomy 18:11; Psalm 50:17; Isaiah 44:10; Lamentations 3:53; Psalm 22:18 | 1 Peter 5:7 |
| rt-trunc-07 | truncation | passage:matthew-11-28 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Matthew 11:28; John 6:37; Genesis 26:27; Psalm 38:22; 2 Timothy 4:9; Song Of Solomon 2:10; John 1:30; John 5:40; John 8:42; Jeremiah 29:12 | Matthew 11:28 |
| rt-kjv-01 | wrong-translation | passage:philippians-4-13 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Philippians 4:13; 1 Corinthians 8:6; Philippians 1:26; Galatians 6:14; Judges 11:37; Romans 15:18; 2 Corinthians 13:3; Hebrews 13:21; Colossians 1:20; Hebrews 9:11 | Philippians 4:13 |
| rt-kjv-02 | wrong-translation | passage:hebrews-11-1 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Hebrews 11:1; Hebrews 11:7; Romans 14:2; Psalm 105:21; 1 Timothy 6:11; Galatians 3:12; Job 6:20; Philemon 1:6; Job 30:26; Galatians 3:2 | Hebrews 11:1 |
| rt-kjv-03 | wrong-translation | passage:ephesians-2-8 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Ephesians 2:8; 1 Timothy 2:15; Acts 15:11; Romans 5:2; Romans 1:5; Luke 7:50; John 1:17; 1 Timothy 6:21; Ephesians 2:5; Galatians 3:26 | Ephesians 2:8 |
| rt-kjv-04 | wrong-translation | passage:isaiah-41-10-psalm-23-4 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | 1 John 4:18; Psalm 34:9; Luke 12:5; Jeremiah 42:11; Psalm 102:15; Isaiah 8:12; 1 Peter 3:14; Psalm 56:3; Psalm 2:11; Psalm 64:4 | Isaiah 41:10; Psalm 23:4 |
| rt-kjv-05 | wrong-translation | passage:romans-8-28 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Romans 8:28; Isaiah 41:23; Proverbs 12:14; 1 Corinthians 5:6; Matthew 7:11; Nehemiah 4:11; 2 Corinthians 9:8; Colossians 1:17; Ecclesiastes 11:5; Matthew 12:35 | Romans 8:28 |
| rt-meaning-01 | meaning-only | passage:philippians-4-6-1-peter-5-7-psalm-94-19 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Psalm 94:19; 1 Peter 5:7; Proverbs 12:25; Ezekiel 12:18; Luke 21:26; Jeremiah 49:23; Ezekiel 12:19 | Philippians 4:6; 1 Peter 5:7; Psalm 94:19 |
| rt-meaning-02 | meaning-only | passage:jeremiah-17-14-isaiah-53-5-psalm-103-3 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Jeremiah 17:14; Luke 6:18; Matthew 8:7; Jeremiah 33:6; Leviticus 13:18; 2 Chronicles 30:20; Psalm 147:3; Matthew 19:2; Luke 9:11; Proverbs 13:17 | Jeremiah 17:14; Isaiah 53:5; Psalm 103:3 |
| rt-meaning-03 | meaning-only | passage:colossians-3-13-1-john-1-9-ephesians-4-32 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Colossians 1:14; Psalm 130:4; Luke 1:77; Daniel 9:9; Mark 1:4; Matthew 26:28; Luke 3:3; Luke 24:47; John 20:23; Acts 10:43 | Colossians 3:13; 1 John 1:9; Ephesians 4:32 |
| rt-meaning-04 | meaning-only | passage:matthew-5-4-psalm-34-18-2-corinthians-1-3 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Luke 18:23; Philippians 4:11; Isaiah 51:12; Isaiah 40:1; Matthew 2:18; Ecclesiastes 7:3; 1 Samuel 21:15; 2 Corinthians 1:7; Revelation 3:17; 1 Peter 1:16 | Matthew 5:4; Psalm 34:18; 2 Corinthians 1:3 |
| rt-meaning-05 | meaning-only | passage:psalm-91-1-isaiah-41-10-psalm-59-1 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Psalm 59:1; Psalm 69:29; Psalm 140:1; Psalm 32:7; Psalm 91:14; Psalm 20:1; 1 John 5:18; John 17:12; 1 Samuel 25:21; Deuteronomy 23:14 | Psalm 91:1; Isaiah 41:10; Psalm 59:1 |
| rt-meaning-06 | meaning-only | passage:isaiah-41-10-philippians-4-13-psalm-28-7 | Any | 5 | — | — | ✓ | ✓ | n/a | n/a | n/a | lexical | Psalm 84:7; Psalm 18:1; Job 41:22; Proverbs 8:14; Philippians 4:13; 1 Samuel 2:31; Genesis 31:6; Numbers 23:22; 1 Chronicles 16:11; Job 12:13 | Isaiah 41:10; Philippians 4:13; Psalm 28:7 |
| rt-meaning-07 | meaning-only | passage:romans-15-13-hebrews-11-1-romans-8-24 | Any | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | 1 Timothy 5:5; Romans 8:24; Job 17:15; Proverbs 11:7; Luke 19:34; Romans 5:4; 1 Corinthians 12:21; 2 Corinthians 8:14; Hebrews 5:12; Romans 12:13 | Romans 15:13; Hebrews 11:1; Romans 8:24 |
| rt-meaning-08 | meaning-only | passage:philippians-4-6-1-peter-5-7-psalm-94-19 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Psalm 94:19; 1 Peter 5:7; Proverbs 12:25; Ezekiel 12:18; Luke 21:26; Jeremiah 49:23; Mark 13:37; 1 Corinthians 9:8; Matthew 9:5; Luke 5:23 | Philippians 4:6; 1 Peter 5:7; Psalm 94:19 |
| rt-keyword-01 | keyword | passage:1-corinthians-13-1-john-3-16-john-15-13 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Luke 6:32; John 15:9; 1 John 4:19; John 13:34; 1 Samuel 20:17; Ephesians 5:28; Proverbs 12:1; 1 John 4:8; 1 John 4:7; John 14:21 | 1 Corinthians 13:1; John 3:16; John 15:13 |
| rt-keyword-02 | keyword | passage:hebrews-11-1-hebrews-11-6-ephesians-2-8 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Galatians 3:9; James 2:18; Romans 3:3; 1 Timothy 1:19; Galatians 3:23; James 2:22; Ephesians 4:5; Romans 3:30; Hebrews 11:4; James 2:14 | Hebrews 11:1; Hebrews 11:6; Ephesians 2:8 |
| rt-keyword-03 | keyword | passage:romans-15-13-hebrews-11-1-romans-8-24 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Romans 8:24; Job 17:15; Proverbs 11:7; Romans 5:4; Lamentations 3:21; Matthew 12:21; Romans 4:18; Job 5:16; Romans 12:12; 2 Corinthians 3:12 | Romans 15:13; Hebrews 11:1; Romans 8:24 |
| rt-keyword-04 | keyword | passage:nehemiah-8-10-philippians-4-4-john-15-11 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | John 15:11; Jeremiah 48:33; Acts 8:8; 1 Thessalonians 2:20; Jeremiah 31:13; Isaiah 35:10; Isaiah 51:11; Psalm 33:3; Psalm 126:5; Isaiah 12:3 | Nehemiah 8:10; Philippians 4:4; John 15:11 |
| rt-keyword-05 | keyword | passage:john-14-27-philippians-4-7-romans-5-1 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Jeremiah 6:14; Jeremiah 8:11; Ephesians 2:17; Isaiah 57:19; Luke 10:6; Matthew 10:34; Isaiah 27:5; Ezekiel 13:10; Matthew 10:13; Jude 1:2 | John 14:27; Philippians 4:7; Romans 5:1 |
| rt-keyword-06 | keyword | passage:ephesians-2-8-2-corinthians-12-9-romans-3-24 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | John 1:16; Romans 11:6; Hebrews 13:25; James 4:6; 1 Corinthians 15:10; 1 Corinthians 16:23; Zechariah 4:7; Hebrews 4:16; Proverbs 3:34; 1 Thessalonians 5:28 | Ephesians 2:8; 2 Corinthians 12:9; Romans 3:24 |
| rt-keyword-07 | keyword | passage:philippians-4-6-1-peter-5-7-psalm-94-19 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Psalm 94:19; 1 Peter 5:7; Proverbs 12:25; Ezekiel 12:18; Luke 21:26; Jeremiah 49:23; Ezekiel 12:19 | Philippians 4:6; 1 Peter 5:7; Psalm 94:19 |
| rt-keyword-08 | keyword | passage:isaiah-53-5-jeremiah-17-14-psalm-103-3 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | lexical | Jeremiah 17:14; Luke 6:18; Matthew 8:7; Jeremiah 33:6; Leviticus 13:18; 2 Chronicles 30:20; Psalm 147:3; Matthew 19:2; Luke 9:11; Proverbs 13:17 | Isaiah 53:5; Jeremiah 17:14; Psalm 103:3 |
| rt-keyword-09 | keyword | passage:colossians-3-13-1-john-1-9-ephesians-4-32 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Colossians 1:14; Psalm 130:4; Luke 1:77; Daniel 9:9; Mark 1:4; Matthew 26:28; Luke 3:3; Luke 24:47; John 20:23; Acts 10:43 | Colossians 3:13; 1 John 1:9; Ephesians 4:32 |
| rt-keyword-10 | keyword | passage:philippians-4-6-matthew-6-6-james-5-16 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Psalm 102:17; Psalm 61:1; Colossians 4:2; 2 Corinthians 1:11; Psalm 72:20; Philemon 1:4; Habakkuk 3:1; Mark 9:29; Romans 12:12; 2 Chronicles 6:19 | Philippians 4:6; Matthew 6:6; James 5:16 |
| rt-falsemem-01 | false-memory | passage:proverbs-13-24 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | fuse(lexical+phrase) | Proverbs 13:24; Proverbs 29:15; Proverbs 23:13; Proverbs 22:15; Romans 11:21; 1 Corinthians 13:11; Ezekiel 40:7; Psalm 119:162; Ezekiel 40:5; Judges 5:30 | Proverbs 13:24 |
| rt-falsemem-02 | false-memory | no-gold:rt-falsemem-02 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Luke 10:32; Numbers 11:23; Psalm 139:6; Matthew 24:35; Mark 13:31; Luke 21:33; Exodus 1:9; Isaiah 28:20; Isaiah 59:1; Romans 3:29 |  |
| rt-falsemem-03 | false-memory | no-gold:rt-falsemem-03 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | 1 Corinthians 12:6; Deuteronomy 32:4; Revelation 15:3; 1 Corinthians 2:7; 2 Chronicles 27:6; Psalm 64:6; 2 Samuel 22:22; Job 27:11; Psalm 18:21; Job 21:14 |  |
| rt-falsemem-04 | false-memory | passage:isaiah-11-6 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | fuse(lexical+phrase) | Isaiah 11:6; Ezekiel 19:2; Genesis 49:9; 1 Samuel 17:34; John 10:18; Joshua 2:8; John 13:37; Jeremiah 51:40; John 15:13; 1 John 3:16 | Isaiah 11:6 |
| rt-falsemem-05 | false-memory | no-gold:rt-falsemem-05 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Luke 6:32; Proverbs 12:1; Psalm 119:163; Psalm 36:2; Ecclesiastes 3:8; Matthew 5:43; Proverbs 8:36; Romans 5:8; Proverbs 13:24; Proverbs 9:8 |  |
| rt-conflated-01 | conflated | passage:psalm-23-1-john-3-16 | All | 9 | — | — | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand); fuse(lexical+expand+phrase) | John 3:16; John 21:16; 1 John 2:15; John 12:25; John 15:19; John 13:1; 1 John 4:9; John 17:24; Psalm 23:1; John 17:23 | Psalm 23:1; John 3:16 |
| rt-conflated-02 | conflated | passage:1-corinthians-13-13-ephesians-6-11 | All | — | — | — | — | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | 1 Corinthians 13:13; 1 Thessalonians 1:3; Colossians 1:5; 1 Thessalonians 5:8; Galatians 5:5; Hebrews 11:1; 1 Peter 1:21; Psalm 33:22; Ephesians 6:23; Psalm 147:11 | 1 Corinthians 13:13; Ephesians 6:11 |
| rt-conflated-03 | conflated | passage:philippians-4-6-isaiah-41-10-philippians-4-7 | All | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Psalm 29:11; Psalm 94:19; 1 Peter 5:7; Acts 9:31; Proverbs 12:25; Ezekiel 12:18; Luke 21:26; Psalm 84:7; Jeremiah 49:23; Psalm 18:1 | Philippians 4:6; Isaiah 41:10; Philippians 4:7 |

## Notes

- **offline-keyword**: same full BSB TEXT, naive multi-term token overlap without stemming, synonym expansion, or phrase memory (proxy for classic keyword Bible search).
- **Live tools** (Bible Gateway, Blue Letter Bible / BibleHub): scores loaded from captured query results when available; environmentally unreachable tools are recorded as such rather than fabricated.
- Family registries require explicit family/split metadata and reject both literal-family and overlapping-passage leakage across split roles.
- `--release-gate` requires a family registry and enforces R@1 ≥ 60%, R@3 ≥ 85%, R@10 ≥ 95%, 100% confidence safety, 100% required structured-passage coverage, 100% hard-negative ordering, and 100% explicitly required confidence labels.
