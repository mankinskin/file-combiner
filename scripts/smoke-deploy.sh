#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${1:?Usage: smoke-deploy.sh <base-url> <probe-token>}"
PROBE_TOKEN="${2:?Usage: smoke-deploy.sh <base-url> <probe-token>}"

# Never trace this script: the probe token must not appear in any log.
set +x

WORKDIR="$(mktemp -d)"
cleanup() { rm -rf "$WORKDIR"; }
trap cleanup EXIT

to_curl_path() {
  if command -v cygpath > /dev/null 2>&1; then
    cygpath -m "$1"
  else
    printf '%s' "$1"
  fi
}

if ! curl -sf "${BASE_URL%/}/healthz" > /dev/null; then
  echo "smoke-deploy: healthz check failed" >&2
  exit 1
fi
echo "smoke-deploy: healthz ok"

echo "first" > "${WORKDIR}/first.txt"
echo "second" > "${WORKDIR}/second.txt"
WORKDIR_CURL="$(to_curl_path "$WORKDIR")"

status="$(curl -s -o /dev/null -w '%{http_code}' \
  -X POST "${BASE_URL%/}/api/combine" \
  -H "Authorization: Bearer ${PROBE_TOKEN}" \
  -H "x-combine-probe: 1" \
  -F "first=@${WORKDIR_CURL}/first.txt;type=text/plain" \
  -F "second=@${WORKDIR_CURL}/second.txt;type=text/plain")"

if [[ "$status" != "200" ]]; then
  echo "smoke-deploy: probe-authenticated combine check failed (status ${status})" >&2
  exit 1
fi
echo "smoke-deploy: probe-authenticated combine ok"
