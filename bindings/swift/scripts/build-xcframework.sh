#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SWIFT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_DIR="$(cd "$SWIFT_DIR/../.." && pwd)"
ARTIFACT_DIR="${1:-$SWIFT_DIR/Artifacts}"
XCFRAMEWORK="$ARTIFACT_DIR/CVerseRecall.xcframework"
STAGING="$ARTIFACT_DIR/.xcframework-staging"
HEADER_SOURCE="$SWIFT_DIR/Sources/CVerseRecall/include/recall.h"
MODULE_SOURCE="$SCRIPT_DIR/module.binary.modulemap"
CANONICAL_HEADER="$REPO_DIR/crates/recall-ffi/include/recall.h"

command -v cargo >/dev/null || { echo "cargo is required" >&2; exit 1; }
command -v rustup >/dev/null || { echo "rustup is required" >&2; exit 1; }
command -v xcodebuild >/dev/null || { echo "xcodebuild is required" >&2; exit 1; }
command -v lipo >/dev/null || { echo "lipo is required" >&2; exit 1; }
command -v install_name_tool >/dev/null || { echo "install_name_tool is required" >&2; exit 1; }
command -v plutil >/dev/null || { echo "plutil is required" >&2; exit 1; }
command -v jq >/dev/null || { echo "jq is required" >&2; exit 1; }
cmp -s "$HEADER_SOURCE" "$CANONICAL_HEADER" || {
  echo "Swift C header is stale; synchronize it with $CANONICAL_HEADER" >&2
  exit 1
}

mkdir -p "$ARTIFACT_DIR"
rm -rf "$XCFRAMEWORK" "$STAGING"
mkdir -p "$STAGING"

installed_targets="$(rustup target list --installed)"
declare -a xcframework_args=()
declare -a built_slices=()

has_target() {
  grep -qx "$1" <<<"$installed_targets"
}

build_target() {
  local target="$1"
  echo "Building recall-ffi for $target"
  SOURCE_DATE_EPOCH=0 ZERO_AR_DATE=1 \
    cargo build --manifest-path "$REPO_DIR/Cargo.toml" \
      --locked -p recall-ffi --release --target "$target"
  local library="$REPO_DIR/target/$target/release/librecall_ffi.dylib"
  test -s "$library" || { echo "missing library: $library" >&2; exit 1; }
  printf '%s\n' "$library"
}

stage_slice() {
  local name="$1"
  shift
  local slice_dir="$STAGING/$name"
  local headers="$slice_dir/Headers"
  local output="$slice_dir/librecall_ffi.dylib"
  mkdir -p "$headers"
  cp "$HEADER_SOURCE" "$headers/recall.h"
  cp "$MODULE_SOURCE" "$headers/module.modulemap"
  if [[ "$#" -eq 1 ]]; then
    cp "$1" "$output"
  else
    lipo -create "$@" -output "$output"
  fi
  # Cargo's local cdylib install name contains its build-directory path. A
  # relocatable rpath ID is required once SwiftPM embeds the binary.
  install_name_tool -id "@rpath/librecall_ffi.dylib" "$output"
  xcframework_args+=( -library "$output" -headers "$headers" )
  built_slices+=("$name")
}

declare -a macos_archives=()
if has_target aarch64-apple-darwin; then
  macos_archives+=("$(build_target aarch64-apple-darwin | tail -n 1)")
fi
if has_target x86_64-apple-darwin; then
  macos_archives+=("$(build_target x86_64-apple-darwin | tail -n 1)")
fi
if [[ "${#macos_archives[@]}" -gt 0 ]]; then
  stage_slice macos "${macos_archives[@]}"
fi

if has_target aarch64-apple-ios; then
  stage_slice ios-device "$(build_target aarch64-apple-ios | tail -n 1)"
fi

declare -a simulator_archives=()
if has_target aarch64-apple-ios-sim; then
  simulator_archives+=("$(build_target aarch64-apple-ios-sim | tail -n 1)")
fi
if has_target x86_64-apple-ios; then
  simulator_archives+=("$(build_target x86_64-apple-ios | tail -n 1)")
fi
if [[ "${#simulator_archives[@]}" -gt 0 ]]; then
  stage_slice ios-simulator "${simulator_archives[@]}"
fi

if [[ "${#xcframework_args[@]}" -eq 0 ]]; then
  echo "No supported installed Apple Rust targets were found." >&2
  echo "Install at least aarch64-apple-darwin or aarch64-apple-ios with rustup." >&2
  exit 1
fi

xcodebuild -create-xcframework "${xcframework_args[@]}" -output "$XCFRAMEWORK"

# xcodebuild may emit AvailableLibraries in a different order for identical
# inputs. Canonicalize both array and dictionary ordering before hashing so an
# unchanged build is byte-for-byte reproducible.
info_plist="$XCFRAMEWORK/Info.plist"
sorted_info="$STAGING/Info.sorted.json"
plutil -convert json -o - "$info_plist" \
  | jq -S '.AvailableLibraries |= sort_by(.LibraryIdentifier)' > "$sorted_info"
plutil -convert xml1 -o "$info_plist" "$sorted_info"

manifest="$ARTIFACT_DIR/CVerseRecall.build-manifest.txt"
{
  echo "schema=1"
  echo "crate=recall-ffi"
  echo "profile=release"
  echo "linkage=dynamic"
  printf 'slices=%s\n' "$(IFS=,; echo "${built_slices[*]}")"
  cargo --version
  rustc --version
  xcodebuild -version | tr '\n' ' '
  echo
  find "$XCFRAMEWORK" -type f -print0 \
    | LC_ALL=C sort -z \
    | xargs -0 shasum -a 256 \
    | sed "s|$XCFRAMEWORK/||"
} > "$manifest"

rm -rf "$STAGING"
echo "Created $XCFRAMEWORK"
echo "Recorded $manifest"
