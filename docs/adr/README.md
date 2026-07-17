# Architecture decision log (ADL)

This directory is the **architecture decision log** for verse-recall.
Each file is one **architecture decision record (ADR)** using the
[Michael Nygard](https://github.com/architecture-decision-record/architecture-decision-record/tree/main/locales/en/templates/decision-record-template-by-michael-nygard)
template (Status / Context / Decision / Consequences).

Upstream reference:
[architecture-decision-record](https://github.com/architecture-decision-record/architecture-decision-record).

## Conventions

| Rule | Practice |
|---|---|
| Location | `docs/adr/` |
| Filename | `NNNN-short-imperative-title.md` (zero-padded, lowercase, dashes) |
| One decision per file | Split multi-topic choices into separate ADRs |
| Status | `Proposed` → `Accepted` → optionally `Deprecated` / `Superseded by ADR-NNNN` |
| Immutability | Do not rewrite history; supersede with a new ADR or append dated amendments |
| Scope | Architecturally significant choices only — not every PR |

When to write an ADR: portable boundary changes, pack formats, fusion policy,
encoder/space contracts, trust/authority rules, eval discipline that shapes
runtime data, or any decision future maintainers must not reverse by accident.

When **not** to write an ADR: local refactors, test-only fixtures, one-off
scripts, reversible experiments that never ship.

## Index

| ADR | Title | Status |
|---|---|---|
| [0001](./0001-record-architecture-decisions.md) | Record architecture decisions | Accepted |
| [0002](./0002-portable-host-owned-encoder-runtime.md) | Portable host-owned encoder runtime | Accepted |
| [0003](./0003-modular-pack-degradation.md) | Modular pack degradation | Accepted |
| [0004](./0004-canonical-bsb-display-hidden-evidence.md) | Canonical BSB display with hidden evidence | Accepted |
| [0005](./0005-no-eval-hardcoding-generalization-discipline.md) | No eval hardcoding; generalization discipline | Accepted |
| [0006](./0006-arctic-m-256-verse-only-dense-baseline.md) | Arctic-M 256 verse-only dense baseline | Accepted |
| [0007](./0007-trust-hierarchy-and-confidence-policy.md) | Trust hierarchy and confidence policy | Accepted |
| [0008](./0008-query-only-clause-planner.md) | Query-only clause planner | Accepted |

## Related documents

| Doc | Role |
|---|---|
| [`DESIGN.md`](../../DESIGN.md) | Living system design (how it works today) |
| [`STATUS.md`](../../STATUS.md) | Product quality position |
| [`AGENTS.md`](../../AGENTS.md) | Non-negotiable search-quality rules |
| [`ARTIFACTS.md`](../../ARTIFACTS.md) | Measured packs, hashes, portability |
| [`eval/method-tradeoffs-v2.md`](../../eval/method-tradeoffs-v2.md) | Method selection evidence |

**DESIGN.md** describes the system. **ADRs** record *why* a choice was made
and what was rejected. Prefer a new ADR over burying rationale only in chat
or eval reports.
