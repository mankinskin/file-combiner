#!/usr/bin/env bash
set -euo pipefail

IMAGE="${1:?Usage: smoke-container.sh <image> <group-token> <probe-token>}"
GROUP_TOKEN="${2:?Usage: smoke-container.sh <image> <group-token> <probe-token>}"
PROBE_TOKEN="${3:?Usage: smoke-container.sh <image> <group-token> <probe-token>}"
PORT="${PORT:-8080}"
BASE_URL="http://127.0.0.1:${PORT}"
CONTAINER_NAME="file-combiner-smoke-$$"
WORKDIR="$(mktemp -d)"

# curl on this host may be a native Windows build that cannot resolve MSYS
# POSIX paths embedded inside a single -F argument; normalize when possible.
to_curl_path() {
  if command -v cygpath > /dev/null 2>&1; then
    cygpath -m "$1"
  else
    printf '%s' "$1"
  fi
}
WORKDIR_CURL="$(to_curl_path "$WORKDIR")"

cleanup() {
  docker rm -f "$CONTAINER_NAME" > /dev/null 2>&1 || true
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

docker run -d --rm \
  --name "$CONTAINER_NAME" \
  -p "${PORT}:8080" \
  -e COMBINE_BEARER_TOKEN="$GROUP_TOKEN" \
  -e COMBINE_PROBE_TOKEN="$PROBE_TOKEN" \
  -e COMBINE_AGENT_BACKEND="${COMBINE_AGENT_BACKEND:-fake}" \
  "$IMAGE" > /dev/null

READY=""
for _ in $(seq 1 30); do
  if curl -sf "${BASE_URL}/healthz" > /dev/null; then
    READY=1
    break
  fi
  sleep 1
done
[[ -n "$READY" ]] || { echo "container did not become healthy in time" >&2; exit 1; }

echo "first content" > "${WORKDIR}/first.txt"
echo "second content" > "${WORKDIR}/second.txt"
head -c 1100000 /dev/zero | tr '\0' 'a' > "${WORKDIR}/oversized.txt"
printf '\xFF\xFE\xFD' > "${WORKDIR}/invalid-utf8.txt"

cat > "${WORKDIR}/first.txt" <<'EOF'
---
version: 1
conversation:
  context: smoke test
  type: advisory
  subject: artifact delivery
role: adviser
personalization:
  - goals
artifacts:
  - name: preparation-brief
    format: markdown
    sections:
      - Priorities
---
# Preparation brief
EOF

expect_status() {
  local description="$1"
  local expected="$2"
  shift 2
  local actual
  actual="$(curl -s -o /dev/null -w '%{http_code}' "$@")"
  if [[ "$actual" != "$expected" ]]; then
    echo "FAIL: ${description} (expected ${expected}, got ${actual})" >&2
    exit 1
  fi
  echo "ok: ${description} (${actual})"
}

manifest="$(curl -sf \
  -X POST "${BASE_URL}/api/combine" \
  -H "Authorization: Bearer ${GROUP_TOKEN}" \
  -F "first=@${WORKDIR_CURL}/first.txt;type=text/plain" \
  -F "second=@${WORKDIR_CURL}/second.txt;type=text/plain")"
artifact_url="$(printf '%s' "$manifest" | sed -n 's/.*"download_url":"\([^"]*\)".*/\1/p')"
[[ -n "$artifact_url" ]] || { echo "FAIL: combine response omitted artifact download URL" >&2; exit 1; }
expect_status "authenticated artifact download succeeds" 200 \
  -H "Authorization: Bearer ${GROUP_TOKEN}" \
  "${BASE_URL}${artifact_url}"
echo "ok: authenticated combine returns an artifact manifest"

expect_status "missing credentials rejected" 401 \
  -X POST "${BASE_URL}/api/combine" \
  -F "first=@${WORKDIR_CURL}/first.txt;type=text/plain" \
  -F "second=@${WORKDIR_CURL}/second.txt;type=text/plain"

expect_status "wrong file count rejected" 400 \
  -X POST "${BASE_URL}/api/combine" \
  -H "Authorization: Bearer ${GROUP_TOKEN}" \
  -F "first=@${WORKDIR_CURL}/first.txt;type=text/plain"

expect_status "oversized file rejected" 413 \
  -X POST "${BASE_URL}/api/combine" \
  -H "Authorization: Bearer ${GROUP_TOKEN}" \
  -F "first=@${WORKDIR_CURL}/oversized.txt;type=text/plain" \
  -F "second=@${WORKDIR_CURL}/second.txt;type=text/plain"

expect_status "invalid UTF-8 rejected" 400 \
  -X POST "${BASE_URL}/api/combine" \
  -H "Authorization: Bearer ${GROUP_TOKEN}" \
  -F "first=@${WORKDIR_CURL}/invalid-utf8.txt;type=text/plain" \
  -F "second=@${WORKDIR_CURL}/second.txt;type=text/plain"

expect_status "probe token without probe header rejected" 401 \
  -X POST "${BASE_URL}/api/combine" \
  -H "Authorization: Bearer ${PROBE_TOKEN}" \
  -F "first=@${WORKDIR_CURL}/first.txt;type=text/plain" \
  -F "second=@${WORKDIR_CURL}/second.txt;type=text/plain"

expect_status "probe token with probe header accepted" 200 \
  -X POST "${BASE_URL}/api/combine" \
  -H "Authorization: Bearer ${PROBE_TOKEN}" \
  -H "x-combine-probe: 1" \
  -F "first=@${WORKDIR_CURL}/first.txt;type=text/plain" \
  -F "second=@${WORKDIR_CURL}/second.txt;type=text/plain"

echo "checking capacity: deterministic proof lives in the Rust integration test"
echo "(see backend/tests/combine.rs::fifth_concurrent_request_returns_service_busy);"
echo "ordinary fast container requests cannot reliably observe 429 under real timing, so this smoke script does not attempt to."

echo "container smoke checks passed"
