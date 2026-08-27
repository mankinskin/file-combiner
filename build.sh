#!/usr/bin/env bash
set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")" || exit 1

rustup target add wasm32-unknown-unknown
if ! command -v trunk >/dev/null || ! trunk --version | grep -q '0.21.14'; then
  cargo install trunk --version 0.21.14 --locked
fi

(cd frontend && trunk build --release)
cargo build --release -p backend
