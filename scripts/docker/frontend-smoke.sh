#!/usr/bin/env bash
set -euo pipefail

npm ci --prefix frontend/e2e
npx --prefix frontend/e2e playwright install --with-deps chromium
cargo build -p backend
bash scripts/smoke-browser.sh ci-smoke-token
