# Record architecture decisions

## Status

Accepted

Date: 2026-07-17

## Context

verse-recall grew through red-team iterations, encoder bake-offs, and
overfitting audits. Rationales lived in chat transcripts, long eval Markdown,
and a living `DESIGN.md`. That is enough to operate the system today, but it
is hard for a new contributor (or future self) to see **which choices are
load-bearing**, which experiments were **rejected on purpose**, and which
docs are **historical vs current**.

We want a lightweight, git-native decision log following the
[architecture-decision-record](https://github.com/architecture-decision-record/architecture-decision-record)
practice, without turning every PR into paperwork.

## Decision

1. Store ADRs under `docs/adr/` using the Michael Nygard template
   (Status / Context / Decision / Consequences).
2. Name files `NNNN-short-imperative-title.md`.
3. Keep `DESIGN.md` as the living *description* of the architecture; ADRs are
   the *decision log* (why, alternatives, status).
4. Backfill accepted ADRs for decisions already enforced in code and policy
   (this series 0002–0008).

## Consequences

- Easier onboarding and review of architecture-changing PRs.
- Rejected paths (e.g. all-lane RRF, eval hardcoding) stay visible as deliberate
  choices rather than forgotten experiments.
- Slight process overhead: significant architecture changes should open or
  update an ADR.
