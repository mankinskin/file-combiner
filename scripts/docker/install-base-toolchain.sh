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

# Some PaaS build sandboxes only allow outbound HTTPS, blocking git's usual
# port 22. GitHub serves the same SSH service on 443 for exactly this case;
# host-key checking still matches against the `github.com` known_hosts entry
# above since only the transport port changes here, not the SSH hostname.
cat >> /root/.ssh/config <<'EOF'
Host github.com
  Hostname ssh.github.com
  Port 443
  User git
  HostKeyAlias github.com
EOF
chmod 600 /root/.ssh/config
