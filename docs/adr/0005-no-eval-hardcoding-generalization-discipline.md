# No eval hardcoding; generalization discipline

## Status

Accepted

Date: 2026-07-17

## Context

Search quality work repeatedly produced green revealed suites that failed on
family-disjoint sealed holdouts. Patching individual misses with aliases,
prompt branches, or gold-derived data creates an illusion of progress and
violates product honesty.

## Decision

Codify in `AGENTS.md` and enforce in review:

- Never special-case evaluation prompts, gold refs, families, or revealed
  failures in production code or runtime data.
- Prefer query-independent, corpus-wide mechanisms (encoders, indexes, source
  datasets with provenance, generic planners).
- Revealed suites are **regression floors**. Sealed family-disjoint holdouts
  are the **generalization claim**. Reveal once; retire; do not patch misses
  into runtime.
- Curated knowledge is allowed only as a maintained source dataset with
  provenance, not as eval leakage.

## Consequences

- Slower path to green benchmarks; faster path to trustworthy quality claims.
- Eval sprawl is intentional: diagnostic / regression / contaminated ledgers.
- Contributors must justify mechanisms by transfer, not by one suite score.
