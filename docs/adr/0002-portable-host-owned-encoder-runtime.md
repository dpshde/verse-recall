# Portable host-owned encoder runtime

## Status

Accepted

Date: 2026-07-17

## Context

Tip-of-the-tongue scripture search must run offline on native, mobile, and web
hosts. Bundling a model runtime (ONNX, PyTorch, Core ML) inside the retrieval
engine would couple pack size, licenses, and platform ABIs to every consumer.
Different hosts already have different preferred inference stacks.

## Decision

- Implement `recall-core` as **deterministic pure CPU** over caller-provided,
  read-only pack buffers.
- Perform **no file, network, SQLite, or model I/O** inside the engine.
- The **host owns query encoding**. The engine accepts whole-query (and optional
  clause) float vectors only when they match a content-addressed
  **embedding-space ID**.
- Expose the same semantics through Rust, C/JSON ABI, Swift, Kotlin/JNI, and
  WebAssembly.

## Consequences

- Engine data stays modular and small (≈11–28 MiB packs); full offline semantic
  search adds a host encoder (≈132 MiB total with Arctic ONNX).
- Server-side encoding is a first-class deployment: clients can ship packs only.
- Same-dimensional vectors from another model fail closed rather than silently
  retrieving garbage.
- Hosts must implement or call an encoder; the library does not “just work”
  for dense mode without one.
