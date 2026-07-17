# Query-only clause planner

## Status

Accepted

Date: 2026-07-17

## Context

Users often hold two memories in one query (“lost sheep and armor of God”).
A single whole-query embedding collapses intents. Corpus-aware split rules
risk leaking eval families into the runtime.

## Decision

- Implement a **query-only** planner with **no pack, verse, embedding, or eval
  access**.
- Emit the whole query plus up to **four** independently useful clauses under
  planner identity `verse-recall-clause-v2`.
- Host embeds each segment with the same space contract; engine retrieves and
  **composes** with head protection and distinct primary / bounded secondary
  coverage.
- Soft boundaries (alternative, subordinate, contrast) do not force independent
  result obligations the way strong/additive boundaries may.

## Consequences

- Generic multi-intent mechanism that can transfer to unseen wording.
- Multi-intent quality is still **not resolved** on revealed or sealed slices;
  the planner is retained for structure and safety, not as a solved product
  claim.
- Hosts must supply clause embeddings in planner order when using the segment
  API; conformance fixtures keep Rust/Python/JS/Swift aligned.
