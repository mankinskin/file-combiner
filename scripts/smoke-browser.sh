#!/usr/bin/env bash
set -euo pipefail

TOKEN="${1:?Usage: smoke-browser.sh <token>}"
PORT="${PORT:-8080}"
BASE_URL="http://127.0.0.1:${PORT}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "$REPO_ROOT"

BACKEND_PID=""
cleanup() {
  if [[ -n "$BACKEND_PID" ]]; then
    kill "$BACKEND_PID" 2>/dev/null || true
    wait "$BACKEND_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

(cd frontend && trunk build --release)

# This checks the HTTP/UI flow deterministically; it must never require live
# model credentials, so it uses the fake combine backend by default.
COMBINE_BEARER_TOKEN="$TOKEN" COMBINE_PROBE_TOKEN="smoke-probe-token" PORT="$PORT" STATIC_DIR="frontend/dist" \
  COMBINE_AGENT_BACKEND="${COMBINE_AGENT_BACKEND:-fake}" \
  cargo run -p backend &
BACKEND_PID=$!

READY=""
for _ in $(seq 1 30); do
  if curl -sf "${BASE_URL}/healthz" > /dev/null; then
    READY=1
    break
  fi
  sleep 1
done
[[ -n "$READY" ]] || { echo "backend did not become healthy in time" >&2; exit 1; }

npm ci --prefix frontend/e2e
npx --prefix frontend/e2e playwright install chromium
npm run test --prefix frontend/e2e -- --base-url "${BASE_URL}" --token "${TOKEN}"
