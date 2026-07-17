# Bible search red-team report

## Run identity

- Evaluator: v0.1.0 (`/Users/user/Developer/verse-recall/target/release/recall-eval`, 1164432 bytes, FNV-1a `c70dc124d0edbbe4`)
- Base pack: `/Users/user/Developer/verse-recall/data/bsb-full.pack` (6433152 bytes, FNV-1a `42bb9909d5a56a3a`)
- Overlay: `/Users/user/Developer/verse-recall/data/bsb-expand.pack` (5627520 bytes, FNV-1a `2daba354133287df`)
- Overlay: `/Users/user/Developer/verse-recall/data/recall-overlay.pack` (7428864 bytes, FNV-1a `4fbf2c94cc0cf810`)
- Overlay: `/Users/user/Developer/verse-recall/data/bsb-semantic.pack` (23874304 bytes, FNV-1a `f937771db44d9b79`)
- Prompts: `/Users/user/Developer/verse-recall/eval/overfit-confirmation-v1.jsonl` (8589 bytes, FNV-1a `b82ac4a4b0740505`)
- Family registry: `/Users/user/Developer/verse-recall/eval/overfit-diagnostic-v1.jsonl` (8659 bytes, FNV-1a `ab447f8a978f5c22`)
- Family registry: `/Users/user/Developer/verse-recall/eval/semantic-quality-v2.jsonl` (12015 bytes, FNV-1a `e5aee9f70cb1681a`)
- Family registry: `/Users/user/Developer/verse-recall/eval/semantic-mutation-frontier-v1.jsonl` (1594 bytes, FNV-1a `0cbc4899b882d616`)
- Family registry: `/Users/user/Developer/verse-recall/eval/tot-prompts.jsonl` (6346 bytes, FNV-1a `6f81b6179788229f`)
- Family registry: `/Users/user/Developer/verse-recall/eval/overlay-adversarial-v1.jsonl` (11023 bytes, FNV-1a `06e867bd96632786`)
- Family registry: `/Users/user/Developer/verse-recall/eval/redteam-prompts.jsonl` (15913 bytes, FNV-1a `0959a2e627648b1c`)
- Query embeddings: `/private/tmp/bge-overfit-confirmation-v1-query.jsonl` (243102 bytes, FNV-1a `001df8a8ac5bd6b3`)
- Embedding space ID: `8744273cd632ff40dc59a2c17bae871a5eebf8ed51a8e96b815c5c65c9672225`
- Parameters: depth=10, split=Some("holdout-overfit-confirmation-v1"), baseline=true, release_gate=true, registry_count=6

Suite size: **30** prompts. Empty-gold safety cases are excluded from retrieval denominators.

Metrics: **Recall@1/3/10** and **MRR** over gold-bearing prompts; `gold_mode=all` requires every split/conflated gold; **confidence safety** requires every top-3 `Recovered` result to overlap gold (and forbids it for empty-gold prompts); named concepts may require all declared structured passages; hard negatives must rank below the completed gold; gold-bearing misquotes may require an explicit confidence label.

## Overall

| System | Retrieval n | R@1 | R@3 | R@10 | MRR | Confidence safety | Passage output | Hard negatives | Required labels |
|--------|------------:|----:|----:|-----:|----:|------------------:|---------------:|---------------:|----------------:|
| verse-recall | 28 | 71.4% | 85.7% | 89.3% | 0.783 | 100.0% (30/30) | 0.0% (0/0) | 0.0% (0/0) | 0.0% (0/0) |
| offline-keyword | 28 | 46.4% | 50.0% | 71.4% | 0.529 | 0.0% (0/0) | 0.0% (0/0) | 0.0% (0/0) | 0.0% (0/0) |

## By category

| Category | System | n | R@1 | R@3 | R@10 | MRR |
|----------|--------|--:|----:|----:|-----:|----:|
| entity-event-unseen | verse-recall | 3 | 100.0% | 100.0% | 100.0% | 1.000 |
| entity-event-unseen | offline-keyword | 3 | 0.0% | 0.0% | 66.7% | 0.150 |
| event-paraphrase-unseen | verse-recall | 9 | 66.7% | 88.9% | 88.9% | 0.759 |
| event-paraphrase-unseen | offline-keyword | 9 | 55.6% | 55.6% | 77.8% | 0.602 |
| false-detail-unseen | verse-recall | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| false-detail-unseen | offline-keyword | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| false-tradition-unseen | verse-recall | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| false-tradition-unseen | offline-keyword | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| multi-intent-unseen | verse-recall | 1 | 0.0% | 0.0% | 100.0% | 0.250 |
| multi-intent-unseen | offline-keyword | 1 | 0.0% | 0.0% | 100.0% | 0.200 |
| rare-phrase | verse-recall | 4 | 100.0% | 100.0% | 100.0% | 1.000 |
| rare-phrase | offline-keyword | 4 | 100.0% | 100.0% | 100.0% | 1.000 |
| semantic-unseen | verse-recall | 8 | 50.0% | 75.0% | 75.0% | 0.604 |
| semantic-unseen | offline-keyword | 8 | 12.5% | 25.0% | 37.5% | 0.219 |
| translation-drift-novel | verse-recall | 3 | 100.0% | 100.0% | 100.0% | 1.000 |
| translation-drift-novel | offline-keyword | 3 | 100.0% | 100.0% | 100.0% | 1.000 |

## By family (verse-recall)

| Family | n | R@1 | R@3 | R@10 | MRR |
|--------|--:|----:|----:|-----:|----:|
| amos-5-24 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| ananias-sapphira | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| dry-bones-vision | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| ecclesiastes-1-9 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| ecclesiastes-11-1 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| eden-apple-tradition | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| ehud-eglon | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| esther-before-king | 1 | 0.0% | 100.0% | 100.0% | 0.333 |
| first-corinthians-1-27 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| first-john-4-18 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| first-peter-4-8-plus-matthew-18-22 | 1 | 0.0% | 0.0% | 100.0% | 0.250 |
| first-thessalonians-4-13 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| gideon-fleece | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| golden-calf | 1 | 0.0% | 100.0% | 100.0% | 0.500 |
| hebrews-13-3 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| james-1-19 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| james-3-1 | 1 | 0.0% | 0.0% | 0.0% | 0.000 |
| luke-12-7 | 1 | 0.0% | 100.0% | 100.0% | 0.333 |
| matthew-18-20 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| naaman-healed | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| peter-walks-water | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| philip-ethiopian-official | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| proverbs-23-4 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| psalm-34-8 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| rahab-hides-spies | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| romans-12-19 | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| romans-8-19-21 | 1 | 0.0% | 100.0% | 100.0% | 0.500 |
| ruth-meets-boaz | 1 | 100.0% | 100.0% | 100.0% | 1.000 |
| seven-deadly-sins-list | 0 | 0.0% | 0.0% | 0.0% | 0.000 |
| solomon-asks-wisdom | 1 | 100.0% | 100.0% | 100.0% | 1.000 |

## Per-prompt (verse-recall)

| id | category | family | gold mode | rank | R@1 | R@3 | R@10 | safe | passage | hard neg | label | lane used | top | gold |
|----|----------|--------|-----------|-----:|-----|-----|------|------|---------|----------|-------|-----------|-----|------|
| oc1-rare-01 | rare-phrase | psalm-34-8 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+alternate) | Psalm 34:8; Proverbs 24:13; Luke 9:27; Jeremiah 24:3; 2 Samuel 16:12; Matthew 16:28; 1 Peter 2:3; 1 Peter 3:2; Genesis 1:29; Psalm 4:6 | Psalm 34:8 |
| oc1-rare-02 | rare-phrase | first-john-4-18 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | 1 John 4:18; Colossians 3:14; 1 John 4:12; Jeremiah 7:15; 2 Corinthians 7:1; John 17:23; Psalm 37:4; Psalm 119:103; 1 Corinthians 13:6; Proverbs 15:13 | 1 John 4:18 |
| oc1-rare-03 | rare-phrase | ecclesiastes-1-9 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Ecclesiastes 1:9; Acts 17:21; Ecclesiastes 8:15; Ecclesiastes 2:11; Ecclesiastes 1:5; Luke 5:38; Lamentations 3:22; 2 Corinthians 5:17; Proverbs 25:14; Ecclesiastes 6:5 | Ecclesiastes 1:9 |
| oc1-rare-04 | translation-drift-novel | amos-5-24 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Amos 5:24; Psalm 78:16; Micah 1:4; Ezekiel 32:14; Acts 10:11; Luke 5:4; Deuteronomy 25:1; Matthew 24:44; Psalm 37:4; Isaiah 59:14 | Amos 5:24 |
| oc1-rare-05 | rare-phrase | matthew-18-20 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Matthew 18:20; Luke 12:52; Job 33:29; 1 Chronicles 11:20; 2 Samuel 23:18; 1 Corinthians 14:29; Genesis 25:24; John 17:21; Genesis 2:2; John 17:20 | Matthew 18:20 |
| oc1-rare-06 | translation-drift-novel | james-1-19 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | James 1:19; Psalm 103:8; Psalm 145:8; Jonah 4:2; Proverbs 15:18; Psalm 86:15; Matthew 6:16; Psalm 102:2; Romans 10:17; Isaiah 29:4 | James 1:19 |
| oc1-rare-07 | translation-drift-novel | ecclesiastes-11-1 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Ecclesiastes 11:1; Lamentations 3:53; Proverbs 9:17; Psalm 22:10; Psalm 24:2; John 12:31; Genesis 9:3; Psalm 65:9; Exodus 23:25; Genesis 18:5 | Ecclesiastes 11:1 |
| oc1-sem-01 | semantic-unseen | luke-12-7 | Any | 3 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+dense) | Matthew 10:29; Matthew 10:30; Luke 12:7; Luke 21:18; Psalm 40:12; Leviticus 19:27; Psalm 139:4; 2 Samuel 14:26; Numbers 6:5; Psalm 69:4 | Luke 12:7 |
| oc1-sem-02 | semantic-unseen | james-3-1 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | 1 Timothy 5:12; 1 Timothy 6:3; Titus 2:10; James 2:13; Psalm 119:66; 1 Timothy 1:19; Psalm 119:108; Romans 14:1; Psalm 119:135; Romans 2:3 | James 3:1 |
| oc1-sem-03 | semantic-unseen | proverbs-23-4 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+dense) | Proverbs 23:4; Luke 12:33; Ruth 3:3; Revelation 3:17; Deuteronomy 22:5; Genesis 13:2; Deuteronomy 8:4; Hebrews 1:11; Deuteronomy 29:5; Luke 18:5 | Proverbs 23:4 |
| oc1-sem-04 | semantic-unseen | first-thessalonians-4-13 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | 1 Thessalonians 4:13; 2 Corinthians 7:10; Hebrews 11:6; Romans 10:14; Lamentations 3:33; James 4:9; Matthew 5:4; John 14:1; 1 Thessalonians 5:17; Romans 12:15 | 1 Thessalonians 4:13 |
| oc1-sem-05 | semantic-unseen | romans-8-19-21 | Any | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+dense) | Isaiah 40:31; Romans 8:19; Psalm 51:10; Psalm 119:166; Psalm 104:30; Romans 8:22; Lamentations 3:26; Ezra 4:21; Matthew 26:59; Acts 6:2 | Romans 8:19-21 |
| oc1-sem-06 | semantic-unseen | first-corinthians-1-27 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+dense) | 1 Corinthians 1:27; 1 Corinthians 1:25; 1 Corinthians 1:21; 1 Corinthians 3:19; James 4:4; Job 11:6; 1 Peter 2:15; Hebrews 10:26; Job 32:13; 1 Corinthians 1:20 | 1 Corinthians 1:27 |
| oc1-sem-07 | semantic-unseen | hebrews-13-3 | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Luke 3:20; Acts 16:27; Psalm 102:20; Zechariah 9:12; Matthew 27:16; Psalm 107:10; Song Of Solomon 4:12; Isaiah 63:9; Galatians 3:23; Ezekiel 23:13 | Hebrews 13:3 |
| oc1-sem-08 | semantic-unseen | romans-12-19 | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+dense) | Romans 12:19; Leviticus 19:18; Psalm 59:5; John 6:67; 1 Corinthians 7:15; Job 21:14; 1 Kings 17:3; Acts 18:7; Ezra 6:7; 1 Samuel 10:9 | Romans 12:19 |
| oc1-event-01 | event-paraphrase-unseen | gideon-fleece | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Judges 6:38; Judges 6:39; Judges 6:40; Judges 6:37; Judges 8:18; Job 31:20; 2 Corinthians 9:7; Leviticus 15:11; Mark 11:24; Judges 6:17 | Judges 6:36-40 |
| oc1-event-02 | event-paraphrase-unseen | esther-before-king | Any | 3 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | Psalm 110:2; Esther 2:17; Esther 5:2; Esther 8:4; Esther 4:11; Esther 8:1; Genesis 49:8; Psalm 122:6; Esther 5:1; Luke 12:11 | Esther 5:1-8 |
| oc1-event-03 | event-paraphrase-unseen | peter-walks-water | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | Matthew 14:29; 1 John 2:6; John 6:19; Mark 6:54; John 21:3; Acts 9:40; Isaiah 43:18; Proverbs 3:5; John 21:7; Matthew 14:28 | Matthew 14:22-33 |
| oc1-event-04 | event-paraphrase-unseen | philip-ethiopian-official | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+dense) | Acts 8:30; Acts 8:28; Acts 8:27; Acts 8:29; Acts 8:38; Isaiah 38:4; Isaiah 37:5; 2 Kings 19:5; Matthew 4:14; Matthew 12:17 | Acts 8:26-40 |
| oc1-event-05 | event-paraphrase-unseen | naaman-healed | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+phrase) | 2 Kings 5:10; Mark 1:42; Luke 4:27; Leviticus 13:58; Matthew 18:22; Leviticus 13:54; Leviticus 13:5; Matthew 6:7; Leviticus 13:6; Psalm 118:24 | 2 Kings 5:1-14 |
| oc1-event-06 | event-paraphrase-unseen | dry-bones-vision | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+dense) | Ezekiel 37:10; Ezekiel 37:9; Psalm 144:4; 1 Chronicles 14:13; Deuteronomy 3:29; 2 Samuel 5:22; 1 Samuel 17:3; Revelation 11:11; Job 26:13; Acts 21:40 | Ezekiel 37:1-14 |
| oc1-event-07 | event-paraphrase-unseen | golden-calf | Any | 2 | — | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+dense) | Exodus 24:18; Exodus 32:4; Numbers 31:51; Exodus 24:15; Deuteronomy 10:5; Leviticus 10:12; Exodus 24:13; Numbers 20:28; Exodus 32:31; Exodus 19:3 | Exodus 32:1-35 |
| oc1-event-08 | event-paraphrase-unseen | ruth-meets-boaz | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+dense) | Ruth 2:3; Ruth 2:17; Ruth 2:23; Ruth 2:15; Ruth 3:7; Ruth 2:2; Ruth 2:8; Ruth 3:16; Ruth 4:13; Matthew 1:5 | Ruth 2:1-23 |
| oc1-event-09 | event-paraphrase-unseen | ananias-sapphira | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Matthew 20:11; Psalm 78:36; Luke 14:20; Deuteronomy 18:8; 2 Kings 12:15; Matthew 25:18; Matthew 25:27; Matthew 19:9; 2 Kings 12:4; Deuteronomy 24:5 | Acts 5:1-11 |
| oc1-entity-01 | entity-event-unseen | rahab-hides-spies | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+dense) | Joshua 2:1; Joshua 6:25; Joshua 6:17; Joshua 2:2; Joshua 2:8; Joshua 6:23; Joshua 2:6; 2 Samuel 11:2; Numbers 21:32; Judges 1:23 | Joshua 2:1-21 |
| oc1-entity-02 | entity-event-unseen | solomon-asks-wisdom | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand) | 1 Kings 3:11; 2 Samuel 18:32; 2 Samuel 18:29; 2 Chronicles 36:17; Proverbs 28:11; Ruth 2:5; 1 Kings 13:6; John 3:16; Proverbs 22:6; 2 Samuel 13:25 | 1 Kings 3:5-15 |
| oc1-entity-03 | entity-event-unseen | ehud-eglon | Any | 1 | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand+dense) | Judges 3:21; Judges 3:22; Judges 9:54; 2 Samuel 23:21; Judges 3:15; Ezekiel 21:14; 1 Chronicles 11:23; 1 Samuel 17:51; Joshua 12:12; Exodus 5:21 | Judges 3:12-30 |
| oc1-blend-01 | multi-intent-unseen | first-peter-4-8-plus-matthew-18-22 | All | 4 | — | — | ✓ | ✓ | n/a | n/a | n/a | fuse(lexical+expand); fuse(lexical+expand+phrase) | Matthew 18:22; John 13:34; Matthew 18:21; 1 Peter 4:8; Luke 17:4; Luke 7:47; Colossians 3:12; Matthew 6:14; Ephesians 4:32; 1 Corinthians 13:8 | 1 Peter 4:8; Matthew 18:22 |
| oc1-nogold-01 | false-tradition-unseen | seven-deadly-sins-list | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Revelation 3:1; 2 Chronicles 29:21; John 8:24; Revelation 17:9; Ephesians 2:1; Numbers 19:11; Luke 17:4; Proverbs 6:16; Leviticus 26:21; Revelation 4:5 |  |
| oc1-nogold-02 | false-detail-unseen | eden-apple-tradition | Any | — | — | — | — | ✓ | n/a | n/a | n/a | n/a | Leviticus 19:23; Joel 1:12; Psalm 17:8; Leviticus 4:2; Amos 8:1; Genesis 1:11; John 15:4; Hosea 10:1; Proverbs 7:2; Proverbs 2:16 |  |

## Notes

- **offline-keyword**: same full BSB TEXT, naive multi-term token overlap without stemming, synonym expansion, or phrase memory (proxy for classic keyword Bible search).
- **Live tools** (Bible Gateway, Blue Letter Bible / BibleHub): scores loaded from captured query results when available; environmentally unreachable tools are recorded as such rather than fabricated.
- Family registries require explicit family/split metadata and reject both literal-family and overlapping-passage leakage across split roles.
- `--release-gate` requires a family registry and enforces R@1 ≥ 60%, R@3 ≥ 85%, R@10 ≥ 95%, 100% confidence safety, 100% required structured-passage coverage, 100% hard-negative ordering, and 100% explicitly required confidence labels.
