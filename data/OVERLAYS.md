# Recall overlay inputs

`scripts/build_recall_overlay.py` creates a deterministic intermediate JSONL for
data-driven recall lanes. It aligns alternate translation text to canonical BSB
references, loads reviewed concept-to-passage mappings, groups the BSB source's
event labels into passage ranges, and aligns Selah topics and Study Bible graph
records to validated BSB passages. `recall-packer overlay` compiles those
records into the typed `VROVL001` body stored in `PHRASE`; the runtime does not
parse or ship the build JSONL.

The generated file is build input, not a second displayed Bible. Search hits from
`alternate_text` records must return the canonical `target_ref` and BSB display
text. Concept hits should use `anchor_ref` for verse-oriented clients while
retaining `target_ranges` for passage-aware clients.

At runtime, alternate text is a precision phrase lane: the query must be a
contiguous phrase of at least four words and occur in no more than three verses
of that alternate corpus. Only a unique match in a corpus covering at least 95%
of canonical refs may claim direct `Recovered` priority, and every adequately
covered hidden translation must agree on that target. Cross-translation
disagreement, ambiguity in any complete corpus, and sparse research-corpus
matches remain ordinary `Related` evidence. High-frequency formulas such as
`and it came to pass` receive no direct priority. A selective exact canonical
BSB phrase outranks alternate-only evidence on collisions.

```bash
python3 scripts/build_recall_overlay.py \
  --canonical data/bsb.jsonl \
  --canonical-topics /path/to/canonical-topics.browser.json \
  --topic-verse-rankings /path/to/topic-verse-rankings.browser.json \
  --study-bible-db /path/to/study_bible.db \
  --reviewed-corrections data/reviewed-corrections-v2.jsonl \
  --alternate KJV=/path/to/kjv.jsonl \
  --output /tmp/recall-overlay.jsonl
```

`--canonical` should be the event-bearing BSB JSONL (`data/bsb.jsonl` after
`ingest_bsb.py`, or the upstream enriched export). Display-only JSONL without
an `events` field is still accepted for Selah topics and alternate alignment,
but it cannot emit ranked BSB event concepts. Study Bible `--study-bible-db`
adds Theographic people/places/`event_shadow` records; those remain score-zero
and do not substitute for BSB `events`.

Additional translations use the same argument repeatedly:

```bash
python3 scripts/build_recall_overlay.py \
  --canonical /path/to/bsb.jsonl \
  --alternate KJV=/path/to/kjv.jsonl \
  --alternate ESV=/path/to/licensed-esv.jsonl \
  --output data/recall-overlay.jsonl
```

ESV is copyrighted. Only use text obtained and packaged under an appropriate
license; do not commit or redistribute a generated ESV overlay under this
repository's MIT license.

## Study Bible graph contract

`--study-bible-db` reads the `graph_people`, `graph_places`, `graph_events`, and
`graph_verse_mentions` tables from the database distributed by
[`djayatillake/studybible-mcp`](https://github.com/djayatillake/studybible-mcp).
The project's published database endpoint is
[`study_bible.db`](https://studybible-mcp.fly.dev/download/study_bible.db).
It does not require or load sqlite-vec. The database is opened read-only, and
the large source file is not copied into the repository.

The graph tables derive from
[`robertrouse/theographic-bible-metadata`](https://github.com/robertrouse/theographic-bible-metadata),
which is CC BY-SA 4.0. Generated graph overlays must retain attribution and be
distributed under compatible terms; they are not relicensed by this project's
MIT code license. The manifest records the source URL and license on every
graph concept source.

The observed `study_bible.db` is 679,141,376 bytes (647.68 MiB), but it is
strictly a build input. The builder opens it read-only and consumes only the
graph tables above. It does not load sqlite-vec or reuse the database's
precomputed vectors; no SQLite runtime or database is required by native,
Swift, Kotlin, or WASM consumers. The measured compiled knowledge/KJV overlay
is 7,475,904 bytes with the full source/reviewed-correction build; see
[`../ARTIFACTS.md`](../ARTIFACTS.md).

All 66 Theographic OSIS book abbreviations are explicitly mapped to canonical
BSB book/chapter/verse coordinates. People and places with at least one valid
canonical mention are imported, including `also_called` person aliases. All
events with a valid mention are imported as `event_shadow`; timeline labels are
not promoted to reviewed direct answers. Contiguous mentions are compacted into
passage ranges. Missing canonical refs, unknown entity ids, and entities with no
valid target are counted in the manifest, making coverage loss auditable.

At runtime these records are score-zero evidence. Exact person/place names,
including single-token names, can explain a verse independently retrieved by a
text lane, but cannot create a candidate, change its score, or raise its
confidence. Homonyms therefore remain multiple non-ranking evidence records.
Multi-word event labels follow the same contract. Engine opening never changes
a graph record's source type, promotes an `event_shadow`, or synthesizes an
event/place alias from canonical wording. Only build-time BSB `event` records
described under event annotation hygiene below enter the ranked event lane.
Provenance-free `curated` records are not accepted by the release builder.

The reproduced full build emitted 3,067 people, 1,264 places, and 450 event
shadows. All 450 remain score-zero at runtime; runtime graph promotions and
runtime-generated graph aliases are both zero. Missing entities and 28
noncanonical verse mentions are explicit in the manifest rather than guessed
or silently shifted.

## Reviewed correction contract

`--reviewed-corrections` is optional and requires `--study-bible-db`. It adds
only source-review-approved BibleAquifer correction records whose database
revision, row identity, content hash, license, source excerpt, and review
metadata all validate. Candidate mining and the exact JSONL review schema are
documented in [`CORRECTIONS.md`](./CORRECTIONS.md).

The retained v2 records identify `codex-source-review-v2`; they are not
described as an independent human editorial review. The complete source-only
review contains 46 approved records and 94 globally unique claim surfaces;
396 mined candidates remain non-runtime.

Correction candidates never enter an overlay. Approved claim surfaces use an
exact full-query match and are rejected if the same contiguous wording occurs
in BSB or any supplied alternate translation. The pack encoder and Rust
runtime independently reject missing approval/provenance metadata. Thus a
correction can emit `OftenMisremembered` only through reviewed source data; it
cannot be inferred from search failures or semantic similarity.

## Retired provenance-free mappings

The former `--curated-concepts` input is rejected by the builder. Its compact
hand-authored schema had no immutable source, license, reviewer, review date, or
evidence revision, yet its records received direct-answer priority. The eight
checked-in mappings were removed and the release overlay was rebuilt without
them. `data/UNSOURCED-MAPPINGS-QUARANTINE.md` records the retired artifact's
identity without preserving a runtime query-to-answer table.

New event/entity/topic coverage must come from comprehensive versioned sources.
Misconception claims must use the separately reviewed correction contract;
evaluation prompts and failures are never runtime data sources.

## Selah topic contract

The topic inputs are optional, but they must be supplied together. The builder
accepts only the browser artifacts' compact tuple schemas:

- `canonical-topics.browser.json`: `[topic_id, label]`
- `topic-verse-rankings.browser.json`:
  `[topic_id, [[canonical_ref, raw_score, rank], ...]]`

The current Selah artifacts contain 6,687 unique topics and one top-ranked ref
per topic. The reproduced full build resolved all 6,687 targets, with no
missing, unknown, or skipped topics. Topic ids, labels, scores, ranks, and any
unexpected tuple fields are never treated as verse refs. Ranking refs must be
strict OSIS singles, same-chapter ranges (`GEN.3.1-24`), or explicit same-book
cross-chapter ranges
(`EPH.4.31-EPH.5.2`). Both endpoints are looked up by exact
book/chapter/verse coordinates in the BSB; row position is never used.

These browser rankings are heuristic top-1 associations, not curated doctrinal
claims. An ordinary exact topic remains score-zero shadow metadata and is
attached only when a scored lane independently retrieves its anchor or a verse
inside its validated range. It cannot cast a vote or change confidence.

Topics may enter semantic Lane C only through a constrained multi-cue bridge:

- a two-to-five-content-stem label requires every stem to be supported by a
  different typed query-expansion group, with at least one inferred edge;
- a genuinely one-word source label requires two independent inferred query
  cues converging on that stem;
- aliases and duplicate labels are consolidated per canonical anchor;
- dense/topic promotion requires a topic score of at least 0.76 and an
  independent top-10 dense verse inside the validated passage;
- every topic result remains `Related`, even after dense agreement.

This gives broad topic recall without letting one noisy label become an
authoritative resolver. Sensitive and ambiguous topics remain permanent
red-team cases rather than candidates for automatic direct promotion.

Topic concepts are typed as `topic` and written after typed BSB `event`
concepts. For example, `the fall` routes to the bounded Genesis 3 event;
Selah's Romans 5:12 association cannot override that ranked event. The runtime
also adds a data-driven event alias only when a two-to-five-word topic surface
is contained by exactly one longer BSB event name, the topic/event passages
overlap, and both are in the same book. This is how `ten commandments` can
activate `Ten Commandments Given` without a hardcoded sample; ambiguous
substring matches are rejected. The manifest reports every overlap, missing
ranking, unknown topic, missing BSB endpoint, and skipped topic.

For natural article variation, a multi-word topic can receive an add-only
leading-`the` alias (`good samaritan` → `the good samaritan`). The builder never
strips `the`, never creates the alias for a single word, and suppresses aliases
that collide with an existing event or topic key. Thus `fall` cannot activate
the higher-priority `The Fall` event.

## Event annotation hygiene

BSB `events` are annotation evidence, not curated claims. The builder forms
canonically contiguous ranges and selects a primary by event-type cue,
label/content overlap, length, then canonical order. It chooses a salient anchor
inside that primary range and exposes only that validated primary as structured
output; secondary raw tags remain diagnostics until separately reviewed. An
event is quarantined when its primary range is shorter than 2 verses, longer
than 200 verses, lacks a required birth/death cue, or is fragmented without a
unique semantic lead. `Lifetime of …` summaries are always quarantined because
the raw tags commonly join a birth range to an isolated age/death verse.

This policy chooses Deuteronomy 34:1–12 (anchored at 34:5) over the stray
Genesis 34:1 tag for `Death of Moses`, and Judges 3:9–11 over the generic Judges
2 summary for `Deliverance by Othniel`. It quarantines lifetime summaries and
over-broad spans such as `Tabernacle Built`. Runtime raw-event hits receive
resolver priority but remain `Related`; they cannot claim direct `Recovered`.
The manifest includes emitted and
quarantined counts plus a deterministic sample of labels, reasons, and primary
ranges, so each source revision can be audited.

### Corpus-wide event signatures

Direct-safe BSB events also receive build-generated search signatures. These
are not hand-authored aliases: the builder applies normalizer v2 morphology to
every label in the complete BSB event inventory, builds one global collision
registry (including quarantined events), and attaches only signatures owned by
one normalized event key. Ownership is conservative containment: every source
spelling for every event key owns any candidate whose cues it contains, even
when casing, label length, or quarantine status would prevent that other label
from generating the candidate itself. The signatures are encoded as reserved aliases in
the intermediate/binary overlay, but the runtime parses them as typed evidence;
it never exposes the marker itself as an ordinary exact alias.

The policy is deliberately bounded:

- a full signature contains two to eight sorted unique content stems;
- a partial signature is generated only for a three-to-five-stem label, omits
  exactly one stem, retains a lower-case source word, and cannot consist only
  of generic event-role stems;
- a one-token signature must be at least seven letters, occur lower-case in the
  source label, be globally event-unique, occur in the selected primary
  passage, and appear in at most five canonical verses;
- title-cased person/place tokens cannot become singleton routes, and
  `Lifetime of …`, `Reign of …`, and `Judgeship of …` records receive no
  signatures;
- a candidate that collides with any source event is suppressed even when the
  other event is quarantined and has no ranked record.

At runtime, morphology can activate a full, partial, or distinctive singleton
signature. `EXPAND` can activate only a multi-token signature when different
query groups cover every stem, at least one cue is original, at least one is
inferred, and every inferred weight is at least 0.35. Equal-priority matches to
different events are suppressed. Exact labels, morphology matches, and
expansion matches all remain `Related`; exact labels retain direct tier zero,
while generated morphology and expansion evidence uses subordinate tiers. The
matcher is installed only on source-typed BSB `event` records; Study Bible
people, places, topics, and `event_shadow` records retain their existing
score-zero contract. Expansion assignment uses an eight-stem bitmask dynamic
program, so runtime work is bounded even when a long query supplies many
candidate expansion groups.

The enriched BSB source currently contains 402 normalized event keys. A
source-only temporary build emits 247 direct-safe event records and quarantines
155. Across all 402 keys it considers 657 signature candidates and observes 25
colliding signatures owned by 62 event-key occurrences. After all suppression
rules, it emits 396 signatures on 195 events: 195 full, 200 partial, and one
singleton. The other 52 direct-safe events receive no signature. Suppressions
are manifested as 31 collision candidates, 44 candidates from 44 unsafe era
summaries, 14 singletons absent from their primary passage, and five singletons
too common in canonical text. The temporary event-only JSONL is 93,607 bytes
and compiles to a 50,816-byte overlay, versus 31,936 bytes for the
same 247 records without signatures. Production artifacts are not replaced by
this measurement.

The event manifest declares total aliases, source aliases, generated
signatures, signature kinds, coverage, every suppression category, collision
owners (bounded sample), and normalizer identity. The JSONL compiler reconciles
both total aliases and generated-marker counts before packing; the binary
overlay header independently reconciles its aggregate alias count. Runtime
loading validates marker shape, BSB source type, target collisions, and
Related-only authority before constructing the lane. Signature scheme v1 is
bound to normalizer v2: a generated-signature manifest must declare that exact
identity, the compiler runs the runtime validator over the emitted binary body,
and unknown reserved marker versions fail closed.

Every alternate must cover at least 99% of canonical refs by default. Source-only
verse numbers are not shifted or guessed: they are omitted and counted in the
manifest. Use `--min-coverage` only when intentionally building a partial research
overlay.

The reproduced KJV alignment matched 31,085 of 31,086 canonical BSB
coordinates (99.9967%). The missing canonical coordinate and all 16
source-only KJV coordinates are recorded in the manifest. This comprehensive
alignment still does not make a repeated KJV phrase a direct recovery: the
runtime selectivity and cross-translation agreement rules above remain in
force.

The build JSONL begins with a `manifest`, followed by sorted event concepts,
topic concepts, Study graph records, reviewed corrections, and then
canonical-order `alternate_text` records. Keys are sorted and no paths or
build timestamps are embedded, so identical inputs produce identical bytes.

Both the packer and runtime reconcile manifest record/alias counts against the
actual JSONL, and runtime also reconciles overlay META identity/counts against
the base pack. Truncated overlays, dirty counts, conflicting ranked aliases,
and conflicting ranked ranges fail closed.

The binary compiler then validates and encodes concept, alias, range,
translation, reference, and alternate-text records with explicit count and
size bounds.

Run the focused validation with:

```bash
python3 -m unittest tests/test_build_recall_overlay.py
```

Compile and search the deterministic JSONL with:

```bash
cargo run -p recall-packer --release -- overlay \
  --input data/recall-overlay.jsonl \
  --output data/recall-overlay.pack
cargo run -p recall-cli --release -- search \
  --pack data/bsb-full.pack \
  --overlay data/recall-overlay.pack \
  "the fall"
```
