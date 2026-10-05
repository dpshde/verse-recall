#!/usr/bin/env bash
# Install Kache and apply it to this user's Cargo config. Not part of ci.
# `kache init` edits ~/.cargo/config.toml (and the login shell on Unix). It does
# not add a Cargo.toml dependency and it does not change GitHub Actions.
set -euo pipefail

export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:${PATH}"

if ! command -v kache >/dev/null 2>&1; then
  echo "kache is not on PATH. Trying cargo install kache --locked (needs Rust 1.95 or newer)."
  if ! cargo install kache --locked; then
    echo "cargo install failed. Installing the mise-pinned binary (github:kunobi-ninja/kache)."
    mise install
  fi
fi

run_kache() {
  if command -v kache >/dev/null 2>&1; then
    kache "$@"
  else
    mise exec -- kache "$@"
  fi
}

if ! command -v kache >/dev/null 2>&1 && ! mise exec -- kache --version >/dev/null; then
  echo "kache is still not available after cargo install and mise install." >&2
  exit 1
fi

run_kache init --yes
run_kache doctor
