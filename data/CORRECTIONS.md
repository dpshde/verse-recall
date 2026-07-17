# Source-backed correction data

`often-misremembered` is a safety claim, not a retrieval synonym. It may be
emitted only by the reviewed correction pipeline in
`scripts/build_correction_records.py`. Evaluation prompts and failed search
results are not valid sources for this data.

## Provenance and coverage

The candidate miner reads only the `aquifer_content` table in the
`study_bible.db` distributed by
[`djayatillake/studybible-mcp`](https://github.com/djayatillake/studybible-mcp).
Those BibleAquifer records are attributed as CC BY-SA 4.0. Both candidate and
reviewed artifacts retain the database SHA-256, row id, content id, resource
type, title/reference, source URL, license, and normalized content SHA-256.

For database snapshot
`b245186271abf8ae2644e11b98716b83b0728f05704e5358e70c949b8e8beca3`,
the deterministic `explicit-correction-language-v1` scan found 442 editorial
candidates among 102,673 Aquifer rows. This is candidate coverage, not 442
verified misconceptions. Candidate records have `runtime_eligible: false` and
cannot be packed as corrections. The reproduced JSONL is 679,199 bytes with
SHA-256 `f5c44667b66bcbd5d14911f9b541b9d56cf459c978fb5ba23d3a103763dde0b2`.

```sh
python3 scripts/build_correction_records.py \
  --study-bible-db /path/to/study_bible.db \
  --output build/aquifer-correction-candidates.jsonl
```

## Reviewed corpus v1

[`archive/reviewed-corrections-v1.jsonl`](./archive/reviewed-corrections-v1.jsonl)
is the source-only review of all 442 candidates in the artifact above. The
review used only the Aquifer candidate excerpt and its cited database row. No
evaluation prompt, result, family, or gold reference was inspected or used.

The retained corpus identifies its reviewer as `codex-source-review-v1`. It is
a Codex source review, not an independent human editorial review. A future
human review should use a distinct identity and produce a newly versioned,
content-addressed corpus rather than silently changing this claim.

A candidate was approved only when the source itself did all of the following:

- identified a held, traditional, common, or readily asserted misconception;
- supplied an explicit, unambiguous correction rather than an unresolved
  interpretation;
- supported every normalized false-claim surface without added facts or
  stronger certainty; and
- supplied a defensible canonical passage target.

Ordinary implicit details (who acted, how much time passed, an unspecified
location or motive), descriptions of beliefs with no source correction,
disputed interpretations, noncanonical background without a defensible target,
and claims requiring editorial inference were rejected. A candidate match by
itself never counted as approval.

| Aquifer resource type | Candidates | Approved | Rejected |
| --- | ---: | ---: | ---: |
| `dictionary` | 44 | 8 | 36 |
| `study_notes` | 18 | 7 | 11 |
| `translation_notes_sil` | 354 | 3 | 351 |
| `translation_notes_uw` | 26 | 2 | 24 |
| **Total** | **442** | **20** | **422** |

The 20 records contain 42 globally unique exact claim surfaces and target 15
canonical books. Each record carries the exact Aquifer row id, content id,
resource type, source title, database revision, normalized row SHA-256, source
excerpt, reviewer identity, review date, and rationale. The reviewed JSONL is
34,544 bytes with SHA-256
`3b472311afa191dcf5c4358d363b89e636ec2fef6a0110249a7f71973a8369c5`.
The immutable review counts, approved row ids, input hashes, schema hash, and
canonical collision result are recorded in
[`archive/reviewed-corrections-v1.manifest.json`](./archive/reviewed-corrections-v1.manifest.json).

## Reviewed corpus v2

[`reviewed-corrections-v2.jsonl`](./reviewed-corrections-v2.jsonl) is a second,
source-only review of the same complete 442-candidate queue. It retains all 20
v1 records after re-review and adds 26 records supported by explicit statements
in their cited Aquifer rows. No evaluation prompt, result, family, report,
attachment, or gold reference was an input. The v2 reviewer identity is
`codex-source-review-v2`; as with v1, this records a Codex source review rather
than an independent human editorial review.

The second pass applied the same review contract across every resource type,
including the much larger SIL translation-note slice. The added records cover
several source-derived claim classes rather than a selected query list:
identity and relationship overclaims, inferred agents and physical actions,
unstated quantities or timing, traditional symbol identifications, genre or
message assumptions, and lexical readings that the source explicitly
distinguishes from the passage wording.

The final audit rejected a proposed parable-historicity record because its
source offered only a majority scholarly probability, not an unambiguous
correction. Similar probabilistic, disputed, or merely possible readings remain
outside the runtime corpus even when the candidate miner finds correction-like
language.

| Aquifer resource type | Candidates | Approved | Rejected |
| --- | ---: | ---: | ---: |
| `dictionary` | 44 | 10 | 34 |
| `study_notes` | 18 | 7 | 11 |
| `translation_notes_sil` | 354 | 24 | 330 |
| `translation_notes_uw` | 26 | 5 | 21 |
| **Total** | **442** | **46** | **396** |

The 46 records contain 94 globally unique exact claim surfaces (48 aliases)
and target 21 canonical books. The reviewed JSONL is 94,769 bytes with SHA-256
`1a6bc97439a7b9f61c73e2aa48005623568decc4c8107ad025a81eec5ac07aa4`.
The immutable input, selection, rejection, approval, source, scripture, and
collision details are in
[`reviewed-corrections-v2.manifest.json`](./reviewed-corrections-v2.manifest.json),
whose SHA-256 is
`778ceb5e143c2f2740aaf69a94c4689227be389b74fd54dc4e2d2d420d17e239`.

V2 validation used the exact BibleAquifer database above plus already-local
BSB and KJV files. The original BSB source hash is
`c33e8c346a5f442b14b7b20d1ef0d61f8b942a5ca7e2637e7e9338fe5f77c097`
and its normalized compiler input hash is
`64f4d5187b17334d0ba10e19d63668ec5ede1290f939202bdf8999f19bed9c8a`.
The original KJV source hash is
`4d532b5596d1696815964707ca497f3c4f21723e8655e6f9b706da959f8470ad`
and its normalized compiler input hash is
`526cd8a801b981f5e9dc103d974fb5bf115753efd1cf5deb32e954819500a7b1`.
All 94 surfaces passed with zero phrase collisions. ESV was not used because
no already-local file with an explicit redistribution license was supplied.

## Review contract

A reviewer creates a separate reviewed JSONL. Every record has this exact
versioned shape (also published as
[`correction-record.schema.json`](./correction-record.schema.json)):

```json
{
  "schema_version": 1,
  "kind": "correction",
  "id": "aquifer.<stable-id>",
  "claim_surfaces": ["normalized false claim"],
  "correction_label": "Concise source-supported explanation",
  "anchor_ref": "Book 1:1",
  "target_ranges": [{"start_ref": "Book 1:1", "end_ref": "Book 1:2"}],
  "source": {
    "dataset": "BibleAquifer",
    "table": "aquifer_content",
    "row_id": 1,
    "content_id": 1,
    "resource_type": "study_notes",
    "source_ref": "Book 1:1",
    "source_url": "https://github.com/djayatillake/studybible-mcp",
    "revision": "sha256:<study-bible-db-sha256>",
    "license": "CC BY-SA 4.0",
    "content_sha256": "<normalized-source-row-sha256>",
    "evidence_excerpt": "An exact excerpt from the source row."
  },
  "review": {
    "status": "approved",
    "policy": "source-backed-correction-v1",
    "reviewed_by": "editor-id",
    "reviewed_at": "YYYY-MM-DD",
    "rationale": "Why the source supports this precise correction claim."
  }
}
```

The compiler verifies the record against the read-only SQLite row, requires
explicit correction language in that row, validates all exact canonical
references and ordered ranges, requires `aquifer.<row_id>` identity, and rejects
stale hashes, candidates, unknown fields, duplicated claims, and unlicensed
sources. Claim surfaces must be 2–24 normalized words.

The end-to-end runtime verifier additionally requires every claim surface to
return its anchor at rank one with the correction confidence, label,
provenance, and every structured target range declared by that source record.

The reviewed corpus can be validated without writing an overlay:

```sh
python3 scripts/build_correction_records.py \
  --study-bible-db /path/to/study_bible.db \
  --validate-reviewed data/reviewed-corrections-v2.jsonl \
  --canonical /path/to/bsb-ref-text.jsonl \
  --alternate /path/to/kjv-ref-text.jsonl
```

Repeat `--alternate /path/to/ref-text.jsonl` for every alternate translation
available to a release build. The overlay build repeats collision checks
against every alternate supplied to it.

## Runtime safety

Reviewed records join the ordinary overlay build:

```sh
python3 scripts/build_recall_overlay.py \
  --canonical data/bsb.jsonl \
  --study-bible-db /path/to/study_bible.db \
  --reviewed-corrections data/reviewed-corrections-v2.jsonl \
  --output build/recall-overlay.jsonl
```

The build rejects a correction claim if it occurs as a contiguous phrase in
the canonical Bible or any supplied alternate translation. The Rust runtime
repeats this check when the packed overlay loads, so a hand-written or stale
overlay fails closed. Runtime matching is exact over the complete reviewed
surface: there is no fuzzy correction matching, semantic expansion, query
fragment matching, or benchmark-derived aliasing. The current v2 corpus has 46
approved records; the remaining 396 candidates stay non-runtime. The release
overlay was rebuilt from this v2 corpus, and all 94 source-derived surfaces
pass the end-to-end runtime verifier. No revealed prompt or gold answer was
added to production code or data.
