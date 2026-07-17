#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINDING_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
AAR="${1:-${BINDING_ROOT}/verse-recall/build/outputs/aar/verse-recall-release.aar}"

if [[ ! -f "${AAR}" ]]; then
  echo "error: AAR not found: ${AAR}" >&2
  exit 1
fi

if ! command -v unzip >/dev/null 2>&1; then
  echo "error: unzip is required" >&2
  exit 1
fi

entries="$(unzip -Z1 "${AAR}")"

require_entry() {
  local entry="$1"
  if ! grep -Fqx "${entry}" <<<"${entries}"; then
    echo "error: missing AAR entry: ${entry}" >&2
    exit 1
  fi
}

require_entry "classes.jar"
for abi in arm64-v8a armeabi-v7a x86_64; do
  require_entry "jni/${abi}/librecall_ffi.so"
  require_entry "jni/${abi}/librecall_jni.so"
done

size_bytes="$(wc -c <"${AAR}" | tr -d ' ')"
echo "verified ${AAR} (${size_bytes} bytes)"
echo "verified classes.jar and two native libraries for 3 ABIs"
