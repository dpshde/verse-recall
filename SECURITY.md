# Security policy

## Supported versions

This project is pre-1.0. Security fixes target the default branch (`main`).

## What this software does (threat model sketch)

`recall-core` is a **deterministic offline** retrieval engine:

- No network I/O
- No filesystem I/O at search time
- Caller supplies pack buffers and optional query embeddings

Hosts remain responsible for: downloading packs, running encoders, sandboxing
untrusted input length, and TLS when serving search over a network.

## Reporting a vulnerability

Please **do not** open a public GitHub issue for security-sensitive reports.

Prefer one of:

1. GitHub **private vulnerability reporting** on this repository (if enabled), or
2. Contact the maintainers via the repository owner’s private channel.

Include:

- Affected crate/binary and version or commit
- Reproduction steps
- Impact (e.g. panic DoS, memory safety, pack parser crash, path escape in tools)

## Out of scope (usually)

- “Search returned a wrong verse” quality bugs (use issues, not security)
- Missing sealed-holdout generalization
- Host encoder supply-chain issues outside this repository
- License compliance of third-party Bible texts you distribute yourself

## Safe defaults for pack parsing

Pack parsers should fail closed on malformed input. If you find a crash or
memory-safety issue in `recall-pack` / FFI boundary, treat it as security-relevant.
