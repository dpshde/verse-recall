#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINDING_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPOSITORY_ROOT="$(cd "${BINDING_ROOT}/../.." && pwd)"
# Keep CMake-imported libraries outside src/main/jniLibs. AGP packages imported
# native dependencies itself; placing the same library in jniLibs would make it
# appear twice during mergeNativeLibs.
OUTPUT_DIR="${1:-${BINDING_ROOT}/verse-recall/build/generated/rustJniLibs}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo is required" >&2
  exit 1
fi

if ! cargo ndk --version >/dev/null 2>&1; then
  echo "error: cargo-ndk is required; install it with 'cargo install cargo-ndk'" >&2
  exit 1
fi

mkdir -p "${OUTPUT_DIR}"

cd "${REPOSITORY_ROOT}"
cargo ndk \
  --platform 21 \
  -t arm64-v8a \
  -t armeabi-v7a \
  -t x86_64 \
  -o "${OUTPUT_DIR}" \
  build --locked -p recall-ffi --release

for abi in arm64-v8a armeabi-v7a x86_64; do
  library="${OUTPUT_DIR}/${abi}/librecall_ffi.so"
  if [[ ! -s "${library}" ]]; then
    echo "error: cargo-ndk did not produce ${library}" >&2
    exit 1
  fi
done

echo "Rust Android libraries written to ${OUTPUT_DIR}"
