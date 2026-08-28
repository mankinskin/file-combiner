#!/usr/bin/env bash
# Installs Node 22, needed only by the frontend-smoke stage to run Playwright.
set -euo pipefail

apt-get update
apt-get install -y --no-install-recommends curl ca-certificates gnupg
curl -fsSL https://deb.nodesource.com/setup_22.x | bash -
apt-get install -y --no-install-recommends nodejs
rm -rf /var/lib/apt/lists/*
