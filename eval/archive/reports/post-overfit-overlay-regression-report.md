# Bible search red-team report

## Run identity

- Evaluator: v0.1.0 (`/Users/user/Developer/verse-recall/target/release/recall-eval`, 1164432 bytes, FNV-1a `32b9bb1c175def06`)
- Base pack: `/Users/user/Developer/verse-recall/data/bsb-full.pack` (6433152 bytes, FNV-1a `42bb9909d5a56a3a`)
- Overlay: `/Users/user/Developer/verse-recall/data/bsb-expand.pack` (5627520 bytes, FNV-1a `2daba354133287df`)
- Overlay: `/Users/user/Developer/verse-recall/data/recall-overlay.pack` (7428864 bytes, FNV-1a `4fbf2c94cc0cf810`)
- Overlay: `/Users/user/Developer/verse-recall/data/bsb-semantic.pack` (23874304 bytes, FNV-1a `f937771db44d9b79`)
- Prompts: `/Users/user/Developer/verse-recall/eval/overlay-adversarial-v1.jsonl` (11023 bytes, FNV-1a `06e867bd96632786`)
- Family registry: `/Users/user/Developer/verse-recall/eval/overfit-holdout-v2.jsonl` (6773 bytes, FNV-1a `8f681b787ff2ec62`)
- Query embeddings: `/private/tmp/bge-overlay-adversarial-query.jsonl` (283819 bytes, FNV-1a `a56f6b345a2d098a`)
- Embedding space ID: `8744273cd632ff40dc59a2c17bae871a5eebf8ed51a8e96b815c5c65c9672225`
- Parameters: depth=10, split=Some("regression-overlay-v1"), baseline=false, release_gate=true, registry_count=1

Suite size: **35** prompts. Empty-gold safety cases are excluded from retrieval denominators.

Metrics: **Recall@1/3/10** and **MRR** over gold-bearing prompts; `gold_mode=all` requires every split/conflated gold; **confidence safety** requires every top-3 `Recovered` result to overlap gold (and forbids it for empty-gold prompts); named concepts may require all declared structured passages; hard negatives must rank below the completed gold; gold-bearing misquotes may require an explicit confidence label.

## Overall

| System | Retrieval n | R@1 | R@3 | R@10 | MRR | Confidence safety | Passage output | Hard negatives | Required labels |
|--------|------------:|----:|----:|-----:|----:|------------------:|---------------:|---------------:|----------------:|
| verse-recall | 29 | 93.1% | 96.6% | 100.0% | 0.957 | 100.0% (35/35) | 100.0% (12/12) | 100.0% (5/5) | 100.0% (1/1) |

## By category

| Category | System | n | R@1 | R@3 | R@10 | MRR |
|----------|--------|--:|----:|----:|-----:|----:|
| ambiguity-safety | verse-recall | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| canonical-phrase | verse-recall | 5 | 100.0% | 100.0% | 100.0% | 1.000 |
| collision-control | verse-recall | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| concept-data-error | verse-recall | 3 | 100.0% | 100.0% | 100.0% | 1.000 |
| event-alias | verse-recall | 5 | 100.0% | 100.0% | 100.0% | 1.000 |
| event-name | verse-recall | 6 | 100.0% | 100.0% | 100.0% | 1.000 |
| kjv-drift | verse-recall | 5 | 100.0% | 100.0% | 100.0% | 1.000 |
| topic-positive-control | verse-recall | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| topic-source-audit | verse-recall | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| translation-hybrid | verse-recall | 3 | 33.3% | 66.7% | 100.0% | 0.583 |

## By family (verse-recall)

| Family | n | R@1 | R@3 | R@10 | MRR |
|--------|--:|----:|----:|-----:|----:|
| birth-of-job | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| david-goliath | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| death-of-moses | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| fall-of-jerusalem | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| feeding-five-thousand | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| first-corinthians-13-4 | 2 | 50.0% | 50.0% | 100.0% | 0.625 |
| first-missionary-journey | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| great-flood | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| hebrews-13-8 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| holy-spirit-comes | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| john-11-25 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| john-14-2 | 2 | 50.0% | 100.0% | 100.0% | 0.750 |
| lifetime-of-moses | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| mark-10-14 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| matthew-5-9 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| noahs-ark | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| pentecost | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| psalm-34-18 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| recurrent-kjv-came-to-pass | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| recurrent-kjv-thus-saith | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| romans-12-21 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| romans-8-1 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| saul-conversion | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| second-timothy-2-15 | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| tabernacle-built | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| ten-commandments-given | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| topic-noise-web-20 | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| topic-racism-slavery | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| topic-sensitive-animal-molesters | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| topic-sensitive-assisted-suicide | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| tower-of-babel | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| water-to-wine | 1 | 100.0% | 100.0% | 100.0% | 1.000 |

## Per-prompt (verse-recall)

| id | category | family | gold mode | rank | R@1 | R@3 | R@10 | safe | passage | hard neg | label | lane used | top | gold |
|----|----------|--------|-----------|-----:|-----|-----|------|------|---------|----------|-------|-----------|-----|------|
| oa-event-01 | event-name | tower-of-babel | Any | 1 | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | fuse(lexical+expand+concept) | Genesis 11:4; Psalm 48:12; Isaiah 2:15; Genesis 11:9; Psalm 61:3; Judges 9:51; Genesis 11:8; Proverbs 18:10; Psalm 46:7; Song Of Solomon 8:10 | Genesis 11:1-9 |
| oa-event-02 | event-name | water-to-wine | Any | 1 | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | fuse(lexical+expand+concept) | John 2:9; Isaiah 1:22; 1 Timothy 5:23; Isaiah 55:1; John 4:46; Joel 3:18; Ephesians 5:18; Proverbs 20:1; Proverbs 31:6; 1 Peter 5:8 | John 2:1-11 |
| oa-event-03 | event-name | first-missionary-journey | Any | 1 | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | fuse(lexical+expand+concept) | Acts 13:46; Ezra 7:9; Jonah 3:4; Genesis 12:9; Proverbs 7:19; Matthew 20:16; Proverbs 3:5; 2 Corinthians 10:14; Acts 16:11; Revelation 1:7 | Acts 12:24-14:28 |
| oa-event-04 | event-name | great-flood | Any | 1 | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | fuse(lexical+expand+concept) | Genesis 7:7; Daniel 11:10; Genesis 9:28; Genesis 9:11; 2 Peter 3:6; Job 27:20; Revelation 16:19; Genesis 7:18; Jeremiah 47:2; Genesis 7:11 | Genesis 7:6-8:19 |
| oa-event-05 | event-name | holy-spirit-comes | Any | 1 | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | fuse(lexical+expand+concept) | Acts 2:4; Acts 1:8; Luke 1:35; Matthew 12:32; Matthew 3:11; Luke 3:16; Hebrews 10:15; 1 John 4:1; John 14:2; Acts 8:16 | Acts 2:1-13 |
| oa-event-06 | event-name | ten-commandments-given | Any | 1 | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | fuse(lexical+expand+concept) | Exodus 20:6; Deuteronomy 10:4; Luke 19:13; Deuteronomy 4:13; Psalm 119:73; Matthew 25:28; Deuteronomy 26:19; Revelation 3:21; Leviticus 27:34; Galatians 5:13 | Exodus 19:1-20:17 |
| oa-alias-01 | event-alias | david-goliath | Any | 1 | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | concept | 1 Samuel 17:1; 1 Samuel 17:54; 1 Samuel 17:23; 1 Samuel 21:9; 1 Samuel 17:24; 1 Samuel 22:10; 1 Samuel 17:22; Mark 11:10; 2 Samuel 9:6; 1 Samuel 21:10 | 1 Samuel 17:1-58 |
| oa-alias-02 | event-alias | saul-conversion | Any | 1 | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | fuse(lexical+expand+concept) | Acts 9:1; Acts 15:3; 1 Samuel 15:31; 1 Samuel 15:34; 1 Samuel 18:20; 1 Samuel 15:35; 1 Samuel 18:22; 1 Chronicles 11:2; 1 Samuel 15:30; 1 Samuel 19:23 | Acts 9:1-19 |
| oa-alias-03 | event-alias | pentecost | Any | 1 | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | fuse(lexical+expand+concept) | Acts 2:1; 1 Corinthians 16:8; Acts 20:16; Luke 4:19; Mark 1:8; Genesis 49:18; Acts 2:2; Psalm 135:1; Psalm 118:24; Isaiah 12:4 | Acts 2:1-41 |
| oa-alias-04 | event-alias | noahs-ark | Any | 1 | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | fuse(lexical+expand+concept) | Genesis 6:8; Genesis 7:9; Genesis 8:6; Genesis 8:10; Genesis 7:15; 1 Peter 3:20; Genesis 7:14; Genesis 12:1; Exodus 18:23; Hebrews 11:7 | Genesis 6:8-8:19 |
| oa-alias-05 | event-alias | feeding-five-thousand | Any | 1 | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | fuse(expand+concept) | Matthew 14:13; John 6:10; Mark 6:44; 1 Kings 4:32; Matthew 14:21; Mark 8:19; Numbers 11:25; Luke 9:15; Ephesians 4:29; Luke 9:14 | Matthew 14:13-21 |
| oa-kjv-01 | kjv-drift | first-corinthians-13-4 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | 1 Corinthians 13:4; Acts 9:36; Leviticus 11:15; Deuteronomy 14:14; Leviticus 11:14; 2 Chronicles 10:7; Proverbs 19:17; Ephesians 4:32; Proverbs 3:16; John 15:7 | 1 Corinthians 13:4 |
| oa-kjv-02 | kjv-drift | second-timothy-2-15 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | 2 Timothy 2:15; Romans 14:18; 1 Thessalonians 2:4; Ecclesiastes 9:7; John 7:15; Romans 16:10; Romans 12:2; Hebrews 13:21; 1 Peter 1:7; Romans 15:14 | 2 Timothy 2:15 |
| oa-kjv-03 | kjv-drift | john-14-2 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | John 14:2; Isaiah 5:9; Hebrews 1:1; Luke 16:27; Mark 2:15; Numbers 20:15; Isaiah 32:18; Ecclesiastes 2:8; Hebrews 3:4; 2 Chronicles 6:7 | John 14:2 |
| oa-kjv-04 | kjv-drift | mark-10-14 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Mark 10:14; John 13:33; Luke 18:16; Matthew 19:14; 1 John 5:21; Mark 9:37; Psalm 127:3; Jeremiah 1:5; Proverbs 22:6; Mark 10:15 | Mark 10:14 |
| oa-kjv-05 | kjv-drift | romans-8-1 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Romans 8:1; Romans 15:17; Colossians 2:6; 2 Timothy 2:1; Romans 5:1; Acts 2:36; 1 Peter 4:16; Isaiah 40:28; Romans 8:2; Matthew 9:9 | Romans 8:1 |
| oa-hybrid-01 | translation-hybrid | first-corinthians-13-4 | Any | 4 | — | — | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Proverbs 19:17; Ephesians 4:32; 2 Corinthians 8:3; 1 Corinthians 13:4; Psalm 27:14; Acts 9:36; Luke 21:19; Romans 12:12; Hebrews 6:15; 2 Corinthians 9:7 | 1 Corinthians 13:4 |
| oa-hybrid-02 | translation-hybrid | second-timothy-2-15 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | 2 Timothy 2:15; 2 Samuel 22:26; Psalm 18:25; 1 Corinthians 11:19; 2 Samuel 22:27; Titus 2:7; Romans 12:2; 2 Corinthians 13:6; Hebrews 10:26; Psalm 119:18 | 2 Timothy 2:15 |
| oa-hybrid-03 | translation-hybrid | john-14-2 | Any | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Isaiah 5:9; John 14:2; Hebrews 1:1; Luke 16:27; Mark 2:15; Numbers 20:15; Isaiah 32:18; Ecclesiastes 2:8; Hebrews 3:4; Proverbs 24:3 | John 14:2 |
| oa-canonical-01 | canonical-phrase | matthew-5-9 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Matthew 5:9; Malachi 3:15; Genesis 25:11; James 3:18; Genesis 9:1; 1 Corinthians 1:9; Jeremiah 1:5; Matthew 5:10; Philippians 4:6; Romans 8:14 | Matthew 5:9 |
| oa-canonical-02 | canonical-phrase | john-11-25 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | John 11:25; John 5:29; John 6:48; Revelation 20:5; Hebrews 11:35; Job 9:21; Psalm 143:8; John 11:24; 1 Corinthians 6:19; Ecclesiastes 3:1 | John 11:25 |
| oa-canonical-03 | canonical-phrase | psalm-34-18 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Psalm 34:18; Psalm 147:3; Zephaniah 1:14; Isaiah 61:1; Psalm 119:151; Psalm 109:16; Luke 9:23; Psalm 34:19; Hebrews 4:16; Psalm 37:24 | Psalm 34:18 |
| oa-canonical-04 | canonical-phrase | romans-12-21 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Romans 12:21; Matthew 12:35; Proverbs 17:13; Psalm 38:20; Luke 6:45; Psalm 34:14; Mark 5:1; Matthew 5:44; Romans 13:1; Luke 6:27 | Romans 12:21 |
| oa-canonical-05 | canonical-phrase | hebrews-13-8 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Hebrews 13:8; Romans 16:27; Ephesians 3:21; Acts 11:17; 1 Samuel 20:27; Hebrews 13:21; 2 Corinthians 5:17; 1 Peter 4:16; Proverbs 27:1; Hebrews 13:9 | Hebrews 13:8 |
| oa-data-01 | concept-data-error | death-of-moses | Any | 1 | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | fuse(lexical+expand+concept) | Deuteronomy 34:5; Joshua 1:1; Deuteronomy 33:1; Numbers 15:36; Leviticus 16:1; Mark 7:10; Deuteronomy 34:6; Romans 5:14; Numbers 20:28; Numbers 17:12 | Deuteronomy 34:1-12 |
| oa-data-02 | concept-data-error | lifetime-of-moses | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+concept) | Deuteronomy 34:7; Psalm 49:18; Genesis 11:28; Psalm 30:5; 1 Kings 12:6; 1 Kings 11:12; Deuteronomy 34:6; Jeremiah 29:11; John 14:6; Acts 7:20 | Deuteronomy 34:7 |
| oa-data-03 | concept-data-error | tabernacle-built | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+concept) | Exodus 36:8; 1 Chronicles 6:32; Exodus 26:18; Nehemiah 3:2; 1 Samuel 14:35; 2 Chronicles 8:4; Matthew 16:18; Hebrews 9:2; Acts 7:45; Acts 7:44 | Exodus 36:8-40:33 |
| oa-safety-01 | ambiguity-safety | birth-of-job | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Job 40:1; 1 Corinthians 6:19; Job 3:2; Job 3:1; Job 6:1; Job 9:1; Job 12:1; Job 16:1; Job 19:1; Job 40:3 |  |
| oa-safety-02 | ambiguity-safety | recurrent-kjv-came-to-pass | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Lamentations 3:37; Isaiah 48:3; Acts 17:1; Luke 10:32; 1 Samuel 10:9; Acts 19:1; 1 Corinthians 1:18; Jeremiah 13:7; 2 Corinthians 9:7; Ezekiel 7:6 |  |
| oa-safety-03 | ambiguity-safety | recurrent-kjv-thus-saith | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Ezekiel 15:8; Jeremiah 42:9; Leviticus 19:25; Isaiah 56:8; Jeremiah 42:13; Jeremiah 7:3; Psalm 118:1; Deuteronomy 32:3; James 4:17; Psalm 103:13 |  |
| oa-topic-01 | topic-source-audit | topic-noise-web-20 | Any | — | — | — | — | ✓ | n/a | ✓ | n/a | n/a | Luke 20:30; Exodus 26:19; Exodus 26:18; Genesis 32:14; 2 Chronicles 2:10; Job 8:14; 1 Kings 5:11; Job 8:15; 1 Chronicles 12:30; Exodus 26:20 |  |
| oa-topic-02 | topic-source-audit | topic-sensitive-animal-molesters | Any | — | — | — | — | ✓ | n/a | ✓ | n/a | n/a | Philippians 3:2; 2 Peter 2:13; Leviticus 20:15; Leviticus 27:9; Genesis 30:42; Leviticus 18:23; Leviticus 24:18; Psalm 148:10; Leviticus 11:28; 2 Peter 2:12 |  |
| oa-topic-03 | topic-source-audit | topic-sensitive-assisted-suicide | Any | — | — | — | — | ✓ | n/a | ✓ | n/a | n/a | Ecclesiastes 8:7; Proverbs 14:12; 1 Timothy 5:5; Numbers 3:6; Numbers 1:5; Ezra 8:20; Isaiah 63:5; Ezra 8:36; 1 Timothy 5:16; Proverbs 16:25 |  |
| oa-topic-04 | topic-positive-control | topic-racism-slavery | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | dense | Galatians 3:28; Nehemiah 5:4; Nehemiah 5:5; Jeremiah 2:14; Hebrews 2:15; Romans 6:19; Exodus 20:2; Deuteronomy 5:6; Deuteronomy 6:12; Titus 2:8 | Galatians 3:28 |
| oa-control-01 | collision-control | fall-of-jerusalem | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | n/a | fuse(expand+concept) | 2 Kings 25:1; Zephaniah 3:16; Zechariah 14:17; Luke 21:24; Nehemiah 13:19; Jeremiah 19:7; Luke 21:25; Isaiah 3:9; Acts 6:7; Zechariah 14:12 | 2 Kings 25:1-21 |

## Notes

- **offline-keyword**: same full BSB TEXT, naive multi-term token overlap without stemming, synonym expansion, or phrase memory (proxy for classic keyword Bible search).
- **Live tools** (Bible Gateway, Blue Letter Bible / BibleHub): scores loaded from captured query results when available; environmentally unreachable tools are recorded as such rather than fabricated.
- Family registries require explicit family/split metadata and reject both literal-family and overlapping-passage leakage across split roles.
- `--release-gate` requires a family registry and enforces R@1 ≥ 60%, R@3 ≥ 85%, R@10 ≥ 95%, 100% confidence safety, 100% required structured-passage coverage, 100% hard-negative ordering, and 100% explicitly required confidence labels.
