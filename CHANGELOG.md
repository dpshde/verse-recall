# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/) once
versioned releases are tagged.

## [Unreleased]

### Added

- Portable offline tip-of-the-tongue retrieval engine (`recall-core`) with
  modular packs (lexical, expansion, overlay, dense LVEC, optional PVEC).
- Multi-platform bindings: C ABI, Swift, Kotlin/JNI, JavaScript/WASM.
- Deterministic evaluator (`recall-eval`), pack tools, and builder scripts.
- Architecture decision log under `docs/adr/`.
- Method trade-off matrix v2 and open-source contributor docs
  (`CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`, `NOTICE`).

### Notes

- Pre-1.0: pack formats and APIs may still evolve with ADRs.
- Generalized semantic quality is **not** claimed as resolved; see `STATUS.md`.
