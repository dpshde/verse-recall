# Arctic-M 256 verse-only dense baseline

## Status

Accepted

Date: 2026-07-17

Supersedes informal “BGE verse/window release” candidates.

## Context

Meaning-only recall needs a dense lane. Candidates included BGE-small,
Arctic-S/M at multiple dims, MiniLM, Nomic, MPNet, enriched documents,
windows, Study-note second tables, query adapters, and cross-encoders.
Selection used **already-revealed** suites only, not sealed holdouts.

## Decision

- Retain **Snowflake Arctic-embed-m-v1.5**, Matryoshka-truncated to **256D**,
  **one L2-normalized int8 vector per canonical BSB verse**.
- Bind model, tokenizer, pooling, max length, query prefix, truncation, and
  normalization into a content-addressed **embedding-space ID**.
- Reject for the compact release profile (unless a later ADR supersedes):
  window/enriched multi-table dense defaults, Study/OpenAI 1536D alignment as
  required runtime, source-trained query adapters as required, generic
  cross-encoder rerank as required.
- Optional **PVEC** passage vectors may load beside LVEC for depth ablations;
  they are permanently `Related` and cannot mint recovery without LVEC support
  (see trade-off matrix v2).

## Consequences

- Compact dense pack (~7.6 MiB LVEC) and a clear encoder contract.
- Generalized semantic quality is **still not claimed** (sealed holdout below
  gates). The choice is “best compact revealed baseline,” not “solved.”
- Changing encoder requires new space ID, packs, query fixtures, and ADRs.
