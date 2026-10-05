# Contributing to verse-recall

Thanks for helping. This project values **portable engineering** and
**honest retrieval quality**. Please read this before opening a PR.

## Quick development loop

Prerequisites:

- Rust stable (edition as in workspace `Cargo.toml`)
- Python 3.11+ (stdlib + `numpy` for some builder tests)
- Optional: Node for JS/WASM bindings
- Optional but recommended for faster local Rust builds: [Kache](https://kunobi.ninja/docs/kache/getting-started/installation) via `mise run kache:setup` (`cargo install kache` when Rust is 1.95+, else the mise pin, then `kache init --yes`). Not a `Cargo.toml` dependency. CI wrapping is Relay's `kunobi-ninja/kache-action@v1` follow-up.

```bash
# Core tests
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings

# Python builder contracts
python3 -m unittest discover -s tests -q

# Release pack identities (requires checked-in data/*.pack)
python3 scripts/verify_release_artifacts.py

# Manual search UI (recall tier: base + expand + overlay)
python3 scripts/serve_search_ui.py --port 4173
# open http://127.0.0.1:4173/
```

With checked-in packs, lexical search needs no external sources:

```bash
cargo run -p recall-cli --release -- search \
  --pack data/bsb-full.pack \
  --overlay data/bsb-expand.pack \
  --overlay data/recall-overlay.pack \
  "lost sheep"
```

## Repository map

| Path | Purpose |
|---|---|
| `crates/` | Engine, packs, CLI, eval, FFI, WASM |
| `bindings/` | Swift, Kotlin, JS |
| `data/` | Canonical JSONL, release packs, contracts |
| `eval/` | Active suites + governing reports (`archive/`, `contaminated/`) |
| `scripts/` | Builders and bake-off tools ([map](./scripts/README.md)) |
| `docs/adr/` | Architecture decision records |
| `AGENTS.md` | Search-quality policy (mandatory for ranking/data PRs) |
| `STATUS.md` | Current quality position |
| `DESIGN.md` | Living architecture description |

## Non-negotiable search-quality rules

See **[AGENTS.md](./AGENTS.md)** in full. In short:

- Do **not** hardcode evaluation prompts, gold references, or suite-specific
  boosts into production code or runtime data.
- Prefer corpus-wide, query-independent mechanisms.
- Revealed suites are regression floors, not generalization proof.
- Sealed holdouts: freeze → one-shot reveal → retire; never patch misses into
  runtime aliases.

PRs that improve a revealed score by special-casing examples will be rejected.

## Architecture changes

If you change portable boundaries, pack formats, fusion/confidence policy,
encoder contracts, or trust rules, add or update an ADR under
[`docs/adr/`](./docs/adr/README.md).

## Pull requests

1. Keep PRs focused; separate refactors from quality experiments.
2. Include test evidence (`cargo test`, relevant Python tests, eval commands).
3. For quality claims, state which suites were available and that no sealed
   holdout was patched after reveal.
4. Do not commit secrets, local absolute paths, or multi-gigabyte model weights.
5. Prefer relative paths in docs (`data/…`, `eval/…`).

## Coding style

- Rust: `rustfmt` + Clippy with `-D warnings` in CI.
- Python: keep builders deterministic and free of eval-gold inputs.
- Match surrounding comment density; prefer clear names over cleverness.

## Licensing

By contributing, you agree your contributions are licensed under the
repository MIT license for code. Do not contribute text or data you cannot
license under the stated source terms (see [NOTICE](./NOTICE) and
[ARTIFACTS.md](./ARTIFACTS.md)).
