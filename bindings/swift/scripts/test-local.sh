#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SWIFT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_DIR="$(cd "$SWIFT_DIR/../.." && pwd)"
CACHE_ROOT="${TMPDIR:-/tmp}/verse-recall-swift-cache"
# Never share SwiftPM output with the XCFramework build.  A binary target is
# copied beside the test executable, where @loader_path precedes our explicit
# Rust rpath and can otherwise shadow the freshly built system library.
SYSTEM_LIBRARY_SCRATCH="$CACHE_ROOT/system-library-build"
RUST_TARGET_DIR="$CACHE_ROOT/rust-target"
HOST_TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
if [[ -z "$HOST_TRIPLE" ]]; then
  echo "Could not determine the host Rust target" >&2
  exit 1
fi
RUST_LIBRARY_DIR="$RUST_TARGET_DIR/$HOST_TRIPLE/debug"

cmp -s \
  "$SWIFT_DIR/Sources/CVerseRecall/include/recall.h" \
  "$REPO_DIR/crates/recall-ffi/include/recall.h" || {
  echo "Swift C header is stale; synchronize it with crates/recall-ffi/include/recall.h" >&2
  exit 1
}

CARGO_TARGET_DIR="$RUST_TARGET_DIR" \
  cargo build --locked --manifest-path "$REPO_DIR/Cargo.toml" \
    -p recall-ffi --target "$HOST_TRIPLE"
test -s "$RUST_LIBRARY_DIR/librecall_ffi.dylib" || {
  echo "Fresh Rust library is missing: $RUST_LIBRARY_DIR/librecall_ffi.dylib" >&2
  exit 1
}
mkdir -p "$CACHE_ROOT/clang" "$CACHE_ROOT/swiftpm"

cd "$SWIFT_DIR"
CLANG_MODULE_CACHE_PATH="$CACHE_ROOT/clang" \
SWIFTPM_MODULECACHE_OVERRIDE="$CACHE_ROOT/swiftpm" \
VERSE_RECALL_USE_SYSTEM_LIBRARY=1 \
VERSE_RECALL_TEST_PACK="$REPO_DIR/data/bsb-full.pack" \
DYLD_LIBRARY_PATH="$RUST_LIBRARY_DIR${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}" \
swift test \
  --scratch-path "$SYSTEM_LIBRARY_SCRATCH" \
  -Xswiftc -strict-concurrency=complete \
  -Xswiftc -warnings-as-errors \
  -Xlinker -L \
  -Xlinker "$RUST_LIBRARY_DIR" \
  -Xlinker -rpath \
  -Xlinker "$RUST_LIBRARY_DIR"
