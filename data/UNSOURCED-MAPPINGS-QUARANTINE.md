# Retired unsourced direct mappings

On 2026-07-16, `data/curated-concepts.json` was removed from the release inputs.
It contained eight hand-selected direct-answer mappings and had no immutable
source record, license, reviewer, review date, rationale, or dataset revision.
Its SHA-256 was
`4bf8da3a75d02673ded1cc30a917e7f368b1aa38b5668243f0920a0860cc97fb`
and its size was 2,647 bytes.

The overlay builder now rejects the legacy `--curated-concepts` option. This
identity is retained only so older generated packs can be audited and rejected;
the mapping contents are not a supported corpus and must not be reconstructed
from evaluations. Coverage must instead derive from comprehensive, versioned
topic/entity/event sources or the source-reviewed correction workflow.
