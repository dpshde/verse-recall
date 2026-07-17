# Bible search red-team report

## Run identity

- Evaluator: v0.1.0 (`/Users/user/Developer/verse-recall/target/release/recall-eval`, 1164432 bytes, FNV-1a `8b819159e4dc2c0b`)
- Base pack: `/Users/user/Developer/verse-recall/data/bsb-full.pack` (6433152 bytes, FNV-1a `42bb9909d5a56a3a`)
- Overlay: `/Users/user/Developer/verse-recall/data/bsb-expand.pack` (5627520 bytes, FNV-1a `2daba354133287df`)
- Overlay: `/Users/user/Developer/verse-recall/data/recall-overlay.pack` (7428864 bytes, FNV-1a `4fbf2c94cc0cf810`)
- Overlay: `/Users/user/Developer/verse-recall/data/bsb-semantic.pack` (23874304 bytes, FNV-1a `f937771db44d9b79`)
- Prompts: `/Users/user/Developer/verse-recall/eval/overfit-diagnostic-v1.jsonl` (8659 bytes, FNV-1a `ab447f8a978f5c22`)
- Family registry: `/Users/user/Developer/verse-recall/eval/overfit-confirmation-v1.jsonl` (8589 bytes, FNV-1a `b82ac4a4b0740505`)
- Family registry: `/Users/user/Developer/verse-recall/eval/semantic-quality-v2.jsonl` (12015 bytes, FNV-1a `e5aee9f70cb1681a`)
- Family registry: `/Users/user/Developer/verse-recall/eval/semantic-mutation-frontier-v1.jsonl` (1594 bytes, FNV-1a `0cbc4899b882d616`)
- Family registry: `/Users/user/Developer/verse-recall/eval/tot-prompts.jsonl` (6346 bytes, FNV-1a `6f81b6179788229f`)
- Family registry: `/Users/user/Developer/verse-recall/eval/overlay-adversarial-v1.jsonl` (11023 bytes, FNV-1a `06e867bd96632786`)
- Family registry: `/Users/user/Developer/verse-recall/eval/redteam-prompts.jsonl` (15913 bytes, FNV-1a `0959a2e627648b1c`)
- Query embeddings: `/private/tmp/bge-overfit-diagnostic-v1-query.jsonl` (243168 bytes, FNV-1a `7e6bb4098244528b`)
- Embedding space ID: `8744273cd632ff40dc59a2c17bae871a5eebf8ed51a8e96b815c5c65c9672225`
- Parameters: depth=10, split=Some("holdout-overfit-diagnostic-v1"), baseline=true, release_gate=true, registry_count=6

Suite size: **30** prompts. Empty-gold safety cases are excluded from retrieval denominators.

Metrics: **Recall@1/3/10** and **MRR** over gold-bearing prompts; `gold_mode=all` requires every split/conflated gold; **confidence safety** requires every top-3 `Recovered` result to overlap gold (and forbids it for empty-gold prompts); named concepts may require all declared structured passages; hard negatives must rank below the completed gold; gold-bearing misquotes may require an explicit confidence label.

## Overall

| System | Retrieval n | R@1 | R@3 | R@10 | MRR | Confidence safety | Passage output | Hard negatives | Required labels |
|--------|------------:|----:|----:|-----:|----:|------------------:|---------------:|---------------:|----------------:|
| verse-recall | 28 | 42.9% | 60.7% | 64.3% | 0.513 | 100.0% (30/30) | 0.0% (0/0) | 0.0% (0/0) | 0.0% (0/1) |
| offline-keyword | 28 | 46.4% | 53.6% | 57.1% | 0.507 | 0.0% (0/0) | 0.0% (0/0) | 0.0% (0/0) | 0.0% (0/0) |

## By category

| Category | System | n | R@1 | R@3 | R@10 | MRR |
|----------|--------|--:|----:|----:|-----:|----:|
| contrast-paraphrase | verse-recall | 2 | 50.0% | 50.0% | 50.0% | 0.500 |
| contrast-paraphrase | offline-keyword | 2 | 0.0% | 0.0% | 0.0% | 0.000 |
| entity-event-unseen | verse-recall | 2 | 0.0% | 50.0% | 50.0% | 0.250 |
| entity-event-unseen | offline-keyword | 2 | 50.0% | 50.0% | 50.0% | 0.500 |
| event-paraphrase-unseen | verse-recall | 6 | 50.0% | 83.3% | 83.3% | 0.639 |
| event-paraphrase-unseen | offline-keyword | 6 | 66.7% | 83.3% | 83.3% | 0.750 |
| false-detail-unseen | verse-recall | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| false-detail-unseen | offline-keyword | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| false-tradition-unseen | verse-recall | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| false-tradition-unseen | offline-keyword | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| multi-intent-unseen | verse-recall | 2 | 0.0% | 0.0% | 50.0% | 0.100 |
| multi-intent-unseen | offline-keyword | 2 | 0.0% | 0.0% | 50.0% | 0.100 |
| rare-phrase | verse-recall | 5 | 80.0% | 100.0% | 100.0% | 0.900 |
| rare-phrase | offline-keyword | 5 | 80.0% | 100.0% | 100.0% | 0.900 |
| semantic-unseen | verse-recall | 7 | 14.3% | 28.6% | 28.6% | 0.190 |
| semantic-unseen | offline-keyword | 7 | 14.3% | 14.3% | 14.3% | 0.143 |
| translation-drift-novel | verse-recall | 3 | 100.0% | 100.0% | 100.0% | 1.000 |
| translation-drift-novel | offline-keyword | 3 | 100.0% | 100.0% | 100.0% | 1.000 |

## By family (verse-recall)

| Family | n | R@1 | R@3 | R@10 | MRR |
|--------|--:|----:|----:|-----:|----:|
| balaam-donkey | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| carmel-contest | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| daniel-lions-den | 1 | 0.0% | 100.0% | 100.0% | 0.333 |
| ephesians-3-20 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| exodus-22-21 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| first-corinthians-2-9 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| first-kings-19-12 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| hebrews-12-1-plus-second-timothy-4-7 | 1 | 0.0% | 0.0% | 100.0% | 0.200 |
| hebrews-13-2 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| james-1-5 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| james-2-26 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| james-3-5-6 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| jericho-collapse | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| jesus-calms-storm | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| john-8-32 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| jonah-great-fish | 1 | 0.0% | 100.0% | 100.0% | 0.500 |
| luke-14-28 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| magi-unknown-count | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| mary-magdalene-prostitute-tradition | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| matthew-22-14 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| philippi-prison-earthquake | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| proverbs-27-17 | 1 | 0.0% | 100.0% | 100.0% | 0.500 |
| psalm-119-105-plus-james-1-22 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| psalm-30-5 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| psalm-56-8 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| romans-1-20 | 1 | 0.0% | 100.0% | 100.0% | 0.333 |
| satan-rules-hell-tradition | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| second-corinthians-4-18 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| zacchaeus-tree | 1 | 0.0% | 100.0% | 100.0% | 0.500 |
| zechariah-4-6 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |

## Per-prompt (verse-recall)

| id | category | family | gold mode | rank | R@1 | R@3 | R@10 | safe | passage | hard neg | label | lane used | top | gold |
|----|----------|--------|-----------|-----:|-----|-----|------|------|---------|----------|-------|-----------|-----|------|
| od1-rare-01 | rare-phrase | proverbs-27-17 | Any | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | dense | Leviticus 19:28; Proverbs 27:17; Proverbs 27:18; 1 Samuel 13:21; Psalm 52:2; Psalm 64:3; Jeremiah 15:12; Daniel 2:33; 1 Corinthians 11:6; Job 28:3 | Proverbs 27:17 |
| od1-drift-01 | translation-drift-novel | first-kings-19-12 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | 1 Kings 19:12; Revelation 19:5; Ezekiel 1:25; Job 4:16; Revelation 10:8; Daniel 4:31; Leviticus 19:31; Revelation 13:9; Deuteronomy 18:10; Job 32:15 | 1 Kings 19:12 |
| od1-drift-02 | translation-drift-novel | psalm-30-5 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Psalm 30:5; Job 38:7; James 4:9; Psalm 55:17; Psalm 92:2; Psalm 6:6; John 21:12; Psalm 90:7; Acts 16:25; Psalm 4:8 | Psalm 30:5 |
| od1-drift-03 | translation-drift-novel | hebrews-13-2 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Hebrews 13:2; Judges 16:25; 1 Timothy 5:19; Acts 28:7; 1 Timothy 5:10; Judges 16:27; Matthew 5:43; 1 Corinthians 13:4; 2 Peter 2:12; 1 Peter 1:12 | Hebrews 13:2 |
| od1-quote-01 | rare-phrase | matthew-22-14 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Matthew 22:14; Matthew 15:34; Acts 17:12; Numbers 13:18; 1 Samuel 16:8; 1 Corinthians 8:5; Philippians 2:3; Numbers 11:25; Matthew 22:15; Matthew 7:12 | Matthew 22:14 |
| od1-quote-02 | rare-phrase | james-2-26 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | James 2:26; Hebrews 6:1; James 2:18; James 2:20; Romans 14:1; James 2:22; Matthew 6:7; Hebrews 11:6; James 3:1; Colossians 3:23 | James 2:26 |
| od1-quote-03 | rare-phrase | john-8-32 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | John 8:32; Genesis 49:21; Romans 6:18; Psalm 105:20; Psalm 119:30; Psalm 22:5; Matthew 6:6; Luke 8:17; John 8:33; John 17:17 | John 8:32 |
| od1-contrast-01 | contrast-paraphrase | zechariah-4-6 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Philippians 4:13; Romans 8:14; Hebrews 2:4; Romans 5:5; Romans 1:4; Hebrews 9:14; Job 37:23; 1 John 4:1; Psalm 30:1; Ezekiel 18:4 | Zechariah 4:6 |
| od1-quote-04 | rare-phrase | first-corinthians-2-9 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | 1 Corinthians 2:9; Isaiah 64:4; Psalm 94:9; Matthew 5:38; Ecclesiastes 1:8; Job 28:7; Psalm 119:18; Proverbs 20:12; 1 Corinthians 2:10; Deuteronomy 29:4 | 1 Corinthians 2:9 |
| od1-sem-01 | semantic-unseen | psalm-56-8 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Psalm 30:5; Psalm 57:2; Psalm 3:5; 2 Corinthians 10:5; Revelation 7:17; Deuteronomy 28:61; 1 Kings 22:36; Isaiah 25:8; Psalm 61:1; Revelation 21:4 | Psalm 56:8 |
| od1-sem-02 | semantic-unseen | james-3-5-6 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Psalm 5:9; Ephesians 4:29; Isaiah 29:5; Luke 12:49; Isaiah 5:14; Psalm 19:3; Psalm 32:9; Psalm 34:13; Proverbs 16:27; James 5:12 | James 3:5-6 |
| od1-sem-03 | semantic-unseen | luke-14-28 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Romans 1:20; James 1:4; Matthew 6:33; Luke 14:30; 2 Chronicles 31:7; Deuteronomy 31:24; Proverbs 8:23; 2 Corinthians 8:11; Mark 14:19; Matthew 5:48 | Luke 14:28 |
| od1-sem-04 | semantic-unseen | ephesians-3-20 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Job 37:23; Jeremiah 1:5; Job 36:27; Hebrews 11:11; Acts 19:39; Acts 15:28; Judges 13:7; 2 Corinthians 4:17; Romans 15:18; Psalm 127:3 | Ephesians 3:20 |
| od1-sem-05 | contrast-paraphrase | second-corinthians-4-18 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | dense | 2 Corinthians 4:18; 2 Corinthians 5:1; Habakkuk 2:3; Isaiah 43:18; Proverbs 4:1; Isaiah 28:23; Job 33:31; Proverbs 4:20; Proverbs 5:1; 2 Corinthians 4:17 | 2 Corinthians 4:18 |
| od1-sem-06 | semantic-unseen | exodus-22-21 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Exodus 22:22; John 3:16; Ezekiel 47:22; 2 Samuel 4:3; Deuteronomy 23:7; Numbers 20:15; Isaiah 14:1; Acts 7:6; Job 18:19; Romans 12:3 | Exodus 22:21 |
| od1-sem-07 | semantic-unseen | james-1-5 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | James 1:5; Matthew 10:8; John 11:22; Job 11:6; Matthew 7:11; 1 Corinthians 1:21; Proverbs 1:7; James 1:6; Isaiah 41:10; Matthew 25:35 | James 1:5 |
| od1-sem-08 | semantic-unseen | romans-1-20 | Any | 3 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Genesis 1:1; John 3:16; Romans 1:20; Colossians 1:15; Titus 1:3; Colossians 1:16; Mark 11:24; Romans 1:21; 2 Corinthians 5:17; Hebrews 4:13 | Romans 1:20 |
| od1-event-01 | event-paraphrase-unseen | jericho-collapse | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | dense | Joshua 6:19; Psalm 27:14; Joshua 8:22; Joshua 6:20; Hebrews 11:30; Joshua 6:16; Joshua 6:4; Joshua 6:9; Joshua 6:5; Proverbs 20:12 | Joshua 6:1-27 |
| od1-event-02 | event-paraphrase-unseen | carmel-contest | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | dense | 1 Kings 18:39; John 6:12; 1 Kings 18:40; 1 Kings 18:22; 2 Kings 10:19; 1 Kings 18:25; Luke 9:8; Mark 6:15; Jeremiah 29:11; 1 Kings 18:2 | 1 Kings 18:20-40 |
| od1-event-03 | event-paraphrase-unseen | daniel-lions-den | Any | 3 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | dense | 1 Kings 20:36; John 3:16; Daniel 6:22; Psalm 17:10; Psalm 58:6; Hebrews 11:33; 1 Samuel 1:6; James 2:24; 1 Kings 13:24; Matthew 6:26 | Daniel 6:10-24 |
| od1-event-04 | event-paraphrase-unseen | jonah-great-fish | Any | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | dense | Genesis 2:2; Jonah 2:1; Jonah 1:17; 2 Corinthians 11:25; Matthew 12:40; 1 Chronicles 12:39; Ezra 10:8; Revelation 21:5; Jonah 1:16; 1 Corinthians 6:19 | Jonah 1:17-2:10 |
| od1-event-05 | event-paraphrase-unseen | jesus-calms-storm | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Job 38:11; 1 Corinthians 14:40; Mark 14:32; Mark 6:45; Matthew 14:22; John 6:43; Psalm 107:29; Mark 6:49; Mark 6:48; Luke 8:23 | Mark 4:35-41 |
| od1-entity-01 | entity-event-unseen | zacchaeus-tree | Any | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | dense | Psalm 95:4; Luke 19:3; 1 Samuel 2:2; Luke 19:4; Luke 15:1; Luke 5:27; Matthew 9:10; Luke 5:30; Mark 2:16; Psalm 18:2 | Luke 19:1-10 |
| od1-entity-02 | entity-event-unseen | balaam-donkey | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Ephesians 4:29; 2 Peter 2:16; Genesis 1:27; 1 Kings 13:13; 1 Kings 13:23; Hosea 12:10; Ezekiel 23:20; Job 26:4; 1 Kings 13:27; Proverbs 22:1 | Numbers 22:21-35 |
| od1-event-06 | event-paraphrase-unseen | philippi-prison-earthquake | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Acts 16:26; Acts 16:25; Acts 16:40; Acts 16:29; Acts 17:14; Acts 16:32; Romans 6:23; Revelation 3:8; Acts 16:24; Revelation 3:20 | Acts 16:16-40 |
| od1-blend-01 | multi-intent-unseen | psalm-119-105-plus-james-1-22 | All | — | — | — | — | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Psalm 119:105; Deuteronomy 13:9; Job 24:13; Psalm 119:101; Psalm 119:130; 1 Kings 12:24; Proverbs 15:1; Psalm 119:106; Isaiah 28:23; Psalm 119:28 | Psalm 119:105; James 1:22 |
| od1-blend-02 | multi-intent-unseen | hebrews-12-1-plus-second-timothy-4-7 | All | 5 | — | — | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | John 14:6; 1 Corinthians 9:24; Matthew 5:48; 2 Timothy 4:7; Hebrews 12:1; Jeremiah 44:27; Exodus 23:27; Zechariah 9:6; Matthew 24:44; 1 Thessalonians 5:22 | Hebrews 12:1; 2 Timothy 4:7 |
| od1-misremember-01 | false-detail-unseen | magi-unknown-count | Any | — | — | — | — | ✓ | n/a | n/a | FAIL | n/a | Proverbs 23:25; 1 John 4:1; Psalm 14:2; Acts 11:3; Job 34:2; Job 15:18; Luke 18:15; Joshua 7:4; Romans 16:27; Isaiah 40:28 | Matthew 2:1-12 |
| od1-nogold-01 | false-tradition-unseen | mary-magdalene-prostitute-tradition | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Ezekiel 16:36; Ephesians 4:29; Mark 15:47; Matthew 27:61; Matthew 27:56; Luke 24:10; John 19:25; Matthew 28:1; Hebrews 4:12; Ezekiel 16:35 |  |
| od1-nogold-02 | false-tradition-unseen | satan-rules-hell-tradition | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Job 2:4; 1 Peter 5:8; Job 2:6; Matthew 12:26; Mark 3:23; Job 1:9; Matthew 23:33; Luke 10:18; 1 Timothy 5:15; James 2:19 |  |

## Notes

- **offline-keyword**: same full BSB TEXT, naive multi-term token overlap without stemming, synonym expansion, or phrase memory (proxy for classic keyword Bible search).
- **Live tools** (Bible Gateway, Blue Letter Bible / BibleHub): scores loaded from captured query results when available; environmentally unreachable tools are recorded as such rather than fabricated.
- Family registries require explicit family/split metadata and reject both literal-family and overlapping-passage leakage across split roles.
- `--release-gate` requires a family registry and enforces R@1 ≥ 60%, R@3 ≥ 85%, R@10 ≥ 95%, 100% confidence safety, 100% required structured-passage coverage, 100% hard-negative ordering, and 100% explicitly required confidence labels.
