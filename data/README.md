# Canonical BSB and modular packs

Finished baseline inventory, hashes, and honest quality claims:
[`../STATUS.md`](../STATUS.md) and [`../ARTIFACTS.md`](../ARTIFACTS.md).

Checked-in `data/bsb.jsonl` and `data/*.pack` are enough for local search and
CI verification. Rebuilds from upstream enriched sources are optional.

`reviewed-corrections-v2.*` is the release correction corpus.
`archive/reviewed-corrections-v1.*` is superseded (do not use in builds).

## Sources (rebuild only)

1. **Enriched BSB JSONL** (31,086 verses) from your local Bible data pipeline,
   e.g. a selah-tools conclusions export — path is environment-specific.
2. **Arweave mirror:**  
   `https://arweave.net/B6yeNb3lk_VkiIp-fTWVh13TlM94LjLK6kC63BPXa8s`

Source lines look like:

```json
{"ref":"Gen.1.1","book":"Genesis","chapter":"1","verseNum":"1","text":"...","events":[...],"entities":[...]}
```

## Build display-ref JSONL + indexed base pack

```bash
# From repo root — convert book/chapter/verseNum → "Genesis 1:1" style refs
python3 scripts/ingest_bsb.py \
  --input /path/to/enriched-bsb.jsonl \
  --output data/bsb.jsonl

cargo run -p recall-packer --release -- pack \
  --input data/bsb.jsonl \
  --output data/bsb-full.pack \
  --translation BSB

cargo run -p recall-cli --release -- inspect --pack data/bsb-full.pack
# expect: META translation=BSB verse_count=31086 and TEXT/LEX/POST sections
```

### Search with checked-in packs (no rebuild)

```bash
cargo run -p recall-cli --release -- search \
  --pack data/bsb-full.pack \
  --overlay data/bsb-expand.pack \
  --overlay data/recall-overlay.pack \
  "lost sheep"
```

The checked-in `data/bsb-full.pack` is the compact production index with
`META + TEXT + LEX + POST` and packed BM25 Lane A. Legacy `META + TEXT` packs
still open through the higher-memory TEXT scan fallback.

The other runtime assets are intentionally separate:

```bash
# Deterministic weighted expansion JSONL → META + EXPAND
cargo run -p recall-packer --release -- expand \
  --input /tmp/bsb-expand.jsonl \
  --output data/bsb-expand.pack

# Deterministic knowledge/alternate JSONL → META + typed binary PHRASE
cargo run -p recall-packer --release -- overlay \
  --input /tmp/recall-overlay.jsonl \
  --output data/recall-overlay.pack

# Arctic-M v1.5 Matryoshka-256 normalized f32 rows → META + int8 LVEC v2
cargo run -p recall-packer --release -- semantic \
  --verse-vectors /tmp/arctic-m256-bsb-verse.f32 \
  --dims 256 \
  --translation BSB \
  --embedding-space-id 36db5d816aeac4c6f6631619d587d7caea5f829fcddfff51450d60823e58d744 \
  --output data/bsb-semantic.pack
```

The exact model/tokenizer/pooling/prefix/truncation contract is
[`semantic-space-arctic-m-v1.5-m256.json`](./semantic-space-arctic-m-v1.5-m256.json).
The release dense pack contains only the canonical verse table. Window,
concatenated-context, and independently ranked Study Note tables were tested
and rejected because their extra depth did not survive broad regression
floors.

The engine accepts the indexed BSB pack plus any combination of compatible
optional packs. All must identify the same canonical translation and 31,086
verse coordinates. The measured release-candidate files and tier totals are in
[`../ARTIFACTS.md`](../ARTIFACTS.md); generated packs and large source assets
are not assumed to be covered by the repository's MIT code license.

## Search / eval

```bash
cargo run -p recall-cli --release -- search --pack data/bsb-full.pack "lost sheep"
cargo run -p recall-eval --release -- \
  --pack data/bsb-full.pack \
  --overlay data/bsb-expand.pack \
  --overlay data/recall-overlay.pack \
  --prompts eval/tot-prompts.jsonl \
  --report /tmp/regression-report.md
```

The evaluator calls every additional pack an `--overlay`, including canonical
`EXPAND` and `LVEC` section packs. A semantic evaluation also supplies
`--query-embeddings`; ordinary lexical search does not need a query model.

See [`OVERLAYS.md`](./OVERLAYS.md) for the data trust hierarchy, comprehensive
Selah/KJV/Study Bible build, and ESV licensing boundary.
The separately reviewed misconception pipeline, Aquifer provenance contract,
and exact-text false-positive guard are documented in
[`CORRECTIONS.md`](./CORRECTIONS.md).
