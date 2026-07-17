# Bible search red-team report

## Run identity

- Evaluator: v0.1.0 (`/Users/user/Developer/verse-recall/target/release/recall-eval`, 1164432 bytes, FNV-1a `a93280ef84f375cb`)
- Base pack: `/Users/user/Developer/verse-recall/data/bsb-full.pack` (6433152 bytes, FNV-1a `42bb9909d5a56a3a`)
- Overlay: `/Users/user/Developer/verse-recall/data/bsb-expand.pack` (5627520 bytes, FNV-1a `2daba354133287df`)
- Overlay: `/Users/user/Developer/verse-recall/data/recall-overlay.pack` (7428864 bytes, FNV-1a `4fbf2c94cc0cf810`)
- Overlay: `/Users/user/Developer/verse-recall/data/bsb-semantic.pack` (23874304 bytes, FNV-1a `f937771db44d9b79`)
- Prompts: `/Users/user/Developer/verse-recall/eval/overfit-holdout-v2.jsonl` (6701 bytes, FNV-1a `00ed8428fd1880fa`)
- Family registry: `/Users/user/Developer/verse-recall/eval/overfit-diagnostic-v1.jsonl` (8659 bytes, FNV-1a `ab447f8a978f5c22`)
- Family registry: `/Users/user/Developer/verse-recall/eval/overfit-confirmation-v1.jsonl` (8589 bytes, FNV-1a `b82ac4a4b0740505`)
- Family registry: `/Users/user/Developer/verse-recall/eval/semantic-quality-v2.jsonl` (12015 bytes, FNV-1a `e5aee9f70cb1681a`)
- Family registry: `/Users/user/Developer/verse-recall/eval/semantic-mutation-frontier-v1.jsonl` (1594 bytes, FNV-1a `0cbc4899b882d616`)
- Family registry: `/Users/user/Developer/verse-recall/eval/tot-prompts.jsonl` (6346 bytes, FNV-1a `6f81b6179788229f`)
- Family registry: `/Users/user/Developer/verse-recall/eval/overlay-adversarial-v1.jsonl` (11023 bytes, FNV-1a `06e867bd96632786`)
- Family registry: `/Users/user/Developer/verse-recall/eval/redteam-prompts.jsonl` (15913 bytes, FNV-1a `0959a2e627648b1c`)
- Query embeddings: `/private/tmp/bge-overfit-holdout-v2-query.jsonl` (194521 bytes, FNV-1a `d776aa39417a6e5e`)
- Embedding space ID: `8744273cd632ff40dc59a2c17bae871a5eebf8ed51a8e96b815c5c65c9672225`
- Parameters: depth=10, split=Some("holdout-overfit-v2"), baseline=true, release_gate=true, registry_count=7

Suite size: **24** prompts. Empty-gold safety cases are excluded from retrieval denominators.

Metrics: **Recall@1/3/10** and **MRR** over gold-bearing prompts; `gold_mode=all` requires every split/conflated gold; **confidence safety** requires every top-3 `Recovered` result to overlap gold (and forbids it for empty-gold prompts); named concepts may require all declared structured passages; hard negatives must rank below the completed gold; gold-bearing misquotes may require an explicit confidence label.

## Overall

| System | Retrieval n | R@1 | R@3 | R@10 | MRR | Confidence safety | Passage output | Hard negatives | Required labels |
|--------|------------:|----:|----:|-----:|----:|------------------:|---------------:|---------------:|----------------:|
| verse-recall | 23 | 39.1% | 56.5% | 78.3% | 0.511 | 100.0% (24/24) | 0.0% (0/0) | 0.0% (0/0) | 0.0% (0/1) |
| offline-keyword | 23 | 43.5% | 52.2% | 56.5% | 0.489 | 0.0% (0/0) | 0.0% (0/0) | 0.0% (0/0) | 0.0% (0/0) |

## By category

| Category | System | n | R@1 | R@3 | R@10 | MRR |
|----------|--------|--:|----:|----:|-----:|----:|
| entity-event-unseen | verse-recall | 3 | 66.7% | 66.7% | 100.0% | 0.750 |
| entity-event-unseen | offline-keyword | 3 | 66.7% | 100.0% | 100.0% | 0.833 |
| event-paraphrase-unseen | verse-recall | 4 | 50.0% | 100.0% | 100.0% | 0.708 |
| event-paraphrase-unseen | offline-keyword | 4 | 75.0% | 75.0% | 75.0% | 0.750 |
| false-detail-unseen | verse-recall | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| false-detail-unseen | offline-keyword | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| false-tradition-unseen | verse-recall | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| false-tradition-unseen | offline-keyword | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| multi-intent-unseen | verse-recall | 2 | 0.0% | 0.0% | 50.0% | 0.100 |
| multi-intent-unseen | offline-keyword | 2 | 0.0% | 0.0% | 0.0% | 0.000 |
| rare-phrase | verse-recall | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| rare-phrase | offline-keyword | 2 | 100.0% | 100.0% | 100.0% | 1.000 |
| semantic-unseen | verse-recall | 8 | 12.5% | 37.5% | 62.5% | 0.278 |
| semantic-unseen | offline-keyword | 8 | 12.5% | 12.5% | 25.0% | 0.156 |
| translation-drift-novel | verse-recall | 3 | 66.7% | 66.7% | 100.0% | 0.750 |
| translation-drift-novel | offline-keyword | 3 | 66.7% | 100.0% | 100.0% | 0.833 |

## By family (verse-recall)

| Family | n | R@1 | R@3 | R@10 | MRR |
|--------|--:|----:|----:|-----:|----:|
| david-spares-saul-cave | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| deborah-barak-sisera | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| delilah-did-not-cut-hair | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| ecclesiastes-7-9 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| elisha-floating-axe-head | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| first-corinthians-12-26 | 1 | 0.0% | 100.0% | 100.0% | 0.500 |
| first-peter-4-9 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| galatians-6-1 | 1 | 0.0% | 0.0% | 100.0% | 0.250 |
| hosea-6-6 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| james-4-14 | 1 | 0.0% | 100.0% | 100.0% | 0.333 |
| jesus-stable-tradition | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| luke-10-7 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| lydia-conversion | 1 | 0.0% | 0.0% | 100.0% | 0.250 |
| matthew-6-11-plus-philippians-4-19 | 1 | 0.0% | 0.0% | 100.0% | 0.200 |
| matthew-7-12-plus-mark-12-31 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| mephibosheth-at-davids-table | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| micah-7-8 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| paul-shipwreck | 1 | 0.0% | 100.0% | 100.0% | 0.333 |
| proverbs-15-1 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| proverbs-23-5 | 1 | 0.0% | 0.0% | 100.0% | 0.143 |
| proverbs-24-17 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| psalm-133-1 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| psalm-42-7 | 1 | 0.0% | 0.0% | 100.0% | 0.250 |
| woman-touches-jesus-garment | 1 | 0.0% | 100.0% | 100.0% | 0.500 |

## Per-prompt (verse-recall)

| id | category | family | gold mode | rank | R@1 | R@3 | R@10 | safe | passage | hard neg | label | lane used | top | gold |
|----|----------|--------|-----------|-----:|-----|-----|------|------|---------|----------|-------|-----------|-----|------|
| oh2-rare-01 | rare-phrase | proverbs-15-1 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Proverbs 15:1; Proverbs 24:18; Ezekiel 16:42; Numbers 25:11; Hosea 13:11; Jeremiah 8:5; John 14:1; Proverbs 15:2; 1 John 1:6; Judges 3:15 | Proverbs 15:1 |
| oh2-rare-02 | translation-drift-novel | psalm-42-7 | Any | 4 | — | — | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Psalm 42:8; Isaiah 29:15; Isaiah 40:4; Psalm 42:7; Revelation 2:24; 1 Samuel 28:15; Job 12:22; Ezekiel 47:4; Romans 9:2; Romans 11:29 | Psalm 42:7 |
| oh2-rare-03 | translation-drift-novel | hosea-6-6 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Hosea 6:6; Matthew 12:7; Matthew 9:13; Luke 22:15; Psalm 132:13; Romans 9:18; Psalm 103:8; Job 29:13; Matthew 6:14; Psalm 30:8 | Hosea 6:6 |
| oh2-rare-04 | translation-drift-novel | micah-7-8 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Micah 7:8; Isaiah 47:5; 1 Kings 18:45; Isaiah 42:7; Jeremiah 23:12; Psalm 105:28; Genesis 3:1; Job 30:27; Ezekiel 28:13; Jeremiah 13:16 | Micah 7:8 |
| oh2-rare-05 | rare-phrase | psalm-133-1 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Psalm 133:1; Psalm 147:1; Deuteronomy 3:25; Genesis 49:15; 1 Corinthians 11:33; Job 34:4; Psalm 23:6; Psalm 133:2; Proverbs 15:13; Matthew 5:43 | Psalm 133:1 |
| oh2-sem-01 | semantic-unseen | proverbs-24-17 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | dense | Proverbs 24:17; Matthew 5:44; Proverbs 24:18; Psalm 9:3; 1 Corinthians 13:6; Psalm 149:4; Ezekiel 18:32; Psalm 51:16; Hebrews 10:38; Luke 6:27 | Proverbs 24:17 |
| oh2-sem-02 | semantic-unseen | luke-10-7 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Isaiah 41:10; Romans 13:6; Matthew 25:35; Lamentations 3:64; Exodus 36:4; Psalm 28:4; John 10:37; Titus 2:7; Genesis 2:2; Titus 3:15 | Luke 10:7 |
| oh2-sem-03 | semantic-unseen | first-peter-4-9 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Matthew 10:12; Romans 15:7; Matthew 10:13; Luke 16:4; Matthew 10:14; Acts 10:32; Acts 11:12; 1 Kings 22:17; John 6:43; 1 Peter 5:14 | 1 Peter 4:9 |
| oh2-sem-04 | semantic-unseen | james-4-14 | Any | 3 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Hosea 13:3; Ezekiel 10:8; James 4:14; Hosea 6:4; Job 11:17; Isaiah 44:22; Psalm 90:6; 1 Peter 3:2; Psalm 90:5; 1 Corinthians 6:19 | James 4:14 |
| oh2-sem-05 | semantic-unseen | first-corinthians-12-26 | Any | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | dense | Psalm 34:18; 1 Corinthians 12:26; 1 Peter 5:10; 2 John 1:11; Romans 12:4; 1 Corinthians 6:15; Ephesians 5:30; Proverbs 14:10; Matthew 10:36; Psalm 69:27 | 1 Corinthians 12:26 |
| oh2-sem-06 | semantic-unseen | proverbs-23-5 | Any | 7 | — | — | ✓ | ✓ | n/a | n/a | n/a | dense | Psalm 49:16; Romans 3:18; Psalm 6:7; Lamentations 5:17; Zechariah 5:9; Psalm 88:9; Proverbs 23:5; Matthew 6:22; Psalm 119:105; Proverbs 23:6 | Proverbs 23:5 |
| oh2-sem-07 | semantic-unseen | galatians-6-1 | Any | 4 | — | — | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Psalm 19:12; Proverbs 12:1; Proverbs 9:7; Galatians 6:1; John 18:23; Ephesians 6:6; Jeremiah 1:12; Habakkuk 2:1; 1 Peter 2:20; Isaiah 1:17 | Galatians 6:1 |
| oh2-sem-08 | semantic-unseen | ecclesiastes-7-9 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Proverbs 14:17; Proverbs 14:29; Titus 1:7; Proverbs 10:8; Luke 24:25; Proverbs 12:23; 2 Timothy 1:7; Proverbs 14:18; Galatians 5:24; Proverbs 12:16 | Ecclesiastes 7:9 |
| oh2-event-01 | event-paraphrase-unseen | elisha-floating-axe-head | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | dense | 2 Kings 6:5; Hebrews 4:12; 2 Kings 13:15; Ecclesiastes 10:10; 2 Kings 6:6; Genesis 7:18; 2 Kings 4:3; Ezekiel 21:16; Matthew 26:52; 2 Kings 8:6 | 2 Kings 6:1-7 |
| oh2-event-02 | event-paraphrase-unseen | david-spares-saul-cave | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | 1 Samuel 24:5; Exodus 4:23; 1 Samuel 24:4; 1 Samuel 19:2; 1 Samuel 24:8; 1 Samuel 20:33; 1 Samuel 1:20; 1 Samuel 24:7; 1 John 1:9; 1 Samuel 24:6 | 1 Samuel 24:1-22 |
| oh2-event-03 | event-paraphrase-unseen | woman-touches-jesus-garment | Any | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | dense | Genesis 1:27; Mark 5:27; Mark 5:28; Matthew 14:14; Luke 6:19; Acts 5:16; Mark 6:56; Luke 22:51; 2 Corinthians 1:3; Matthew 9:22 | Mark 5:25-34 |
| oh2-event-04 | event-paraphrase-unseen | paul-shipwreck | Any | 3 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | dense | Acts 28:5; James 4:6; Acts 27:44; Acts 18:1; Mark 9:26; Acts 20:11; 1 Timothy 1:19; Acts 23:10; Acts 13:13; Romans 6:23 | Acts 27:13-44 |
| oh2-entity-01 | entity-event-unseen | mephibosheth-at-davids-table | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | dense | 2 Samuel 9:7; Luke 14:12; 2 Samuel 12:21; 1 Samuel 20:34; Luke 7:36; 1 Samuel 19:1; 2 Samuel 21:7; 2 Samuel 1:17; Daniel 2:20; Psalm 103:13 | 2 Samuel 9:1-13 |
| oh2-entity-02 | entity-event-unseen | lydia-conversion | Any | 4 | — | — | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Acts 16:33; Philippians 2:3; Leviticus 15:11; Acts 16:14; 1 Corinthians 1:16; 1 Corinthians 1:13; Galatians 3:27; Numbers 4:13; Isaiah 23:8; John 1:6 | Acts 16:11-15 |
| oh2-entity-03 | entity-event-unseen | deborah-barak-sisera | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | dense | Judges 4:9; Judges 9:52; Judges 4:6; Judges 4:12; Judges 4:14; Judges 4:16; Judges 4:22; Psalm 50:4; Proverbs 21:9; Judges 4:7 | Judges 4:1-24 |
| oh2-blend-01 | multi-intent-unseen | matthew-7-12-plus-mark-12-31 | All | — | — | — | — | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Genesis 16:6; John 13:34; Matthew 19:19; Galatians 5:14; Matthew 22:39; Mark 12:31; Psalm 37:4; Galatians 5:15; Ephesians 5:29; Leviticus 19:18 | Matthew 7:12; Mark 12:31 |
| oh2-blend-02 | multi-intent-unseen | matthew-6-11-plus-philippians-4-19 | All | 5 | — | — | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Matthew 7:7; Luke 11:3; Amos 4:5; Philippians 4:19; Matthew 6:11; Ezra 7:20; Judges 19:20; Ezekiel 21:7; John 15:7; Job 38:3 | Matthew 6:11; Philippians 4:19 |
| oh2-misremember-01 | false-detail-unseen | delilah-did-not-cut-hair | Any | — | — | — | — | ✓ | n/a | n/a | FAIL | n/a | Judges 16:14; Judges 16:4; Judges 16:6; Judges 16:10; Judges 16:17; Jeremiah 25:23; Leviticus 21:5; Judges 16:18; Hebrews 11:7; Numbers 14:24 | Judges 16:19 |
| oh2-nogold-01 | false-tradition-unseen | jesus-stable-tradition | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | John 8:58; John 18:37; Mark 10:10; 1 John 5:1; John 3:3; Matthew 1:16; Luke 2:5; Ephesians 4:29; Luke 4:16; Hebrews 4:12 |  |

## Notes

- **offline-keyword**: same full BSB TEXT, naive multi-term token overlap without stemming, synonym expansion, or phrase memory (proxy for classic keyword Bible search).
- **Live tools** (Bible Gateway, Blue Letter Bible / BibleHub): scores loaded from captured query results when available; environmentally unreachable tools are recorded as such rather than fabricated.
- Family registries require explicit family/split metadata and reject both literal-family and overlapping-passage leakage across split roles.
- `--release-gate` requires a family registry and enforces R@1 ≥ 60%, R@3 ≥ 85%, R@10 ≥ 95%, 100% confidence safety, 100% required structured-passage coverage, 100% hard-negative ordering, and 100% explicitly required confidence labels.
