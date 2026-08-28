#!/usr/bin/env bash
# Installs the shared Rust/wasm toolchain and the SSH client used to fetch
# the private `agent-builder` git dependency. Runs before `COPY . .` so this
# layer only invalidates when this script itself changes, not on every
# source edit.
set -euo pipefail

rustup target add wasm32-unknown-unknown
rustup component add rustfmt clippy
cargo install trunk --version 0.21.14 --locked

apt-get update
apt-get install -y --no-install-recommends openssh-client
rm -rf /var/lib/apt/lists/*
mkdir -p -m 0700 /root/.ssh
ssh-keyscan -t rsa,ecdsa,ed25519 github.com >> /root/.ssh/known_hosts
