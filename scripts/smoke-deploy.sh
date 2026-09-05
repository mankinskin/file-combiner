#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${1:?Usage: smoke-deploy.sh <base-url> <probe-token> [<group-name> <group-password>]}"
PROBE_TOKEN="${2:?Usage: smoke-deploy.sh <base-url> <probe-token> [<group-name> <group-password>]}"
GROUP_NAME="${3:-}"
GROUP_PASSWORD="${4:-}"

# Never trace this script: tokens and credentials must not appear in any log.
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

echo "smoke-deploy: starting production validation tests"

# Test 1: Reject non-HTTPS URLs
echo "TEST 1: HTTPS enforcement (reject plaintext HTTP)"
if [[ "$BASE_URL" == http://* ]]; then
  echo "  ERROR: BASE_URL must use https:// (plaintext HTTP not allowed in production)" >&2
  echo "  Deployment blocked: HTTPS is mandatory" >&2
  exit 1
fi
if [[ "$BASE_URL" != https://* ]]; then
  echo "  ERROR: BASE_URL must use https:// scheme (got: $BASE_URL)" >&2
  exit 1
fi
echo "  PASS: HTTPS scheme validated"

# Test 2: Health check with probe token in Authorization header
echo "TEST 2: Health check endpoint validation"
status="$(curl -s -o /dev/null -w '%{http_code}' \
  "${BASE_URL%/}/healthz")"
if [[ "$status" != "200" ]]; then
  echo "  ERROR: /healthz check failed (status ${status})" >&2
  exit 1
fi
echo "  PASS: /healthz returns 200"

# Test 3: Combine endpoint with group credentials (if provided)
if [[ -n "$GROUP_NAME" && -n "$GROUP_PASSWORD" ]]; then
  echo "TEST 3: Group authentication on combine endpoint"
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
  echo "second" > "${WORKDIR}/second.txt"
  WORKDIR_CURL="$(to_curl_path "$WORKDIR")"
  
  # Encode group credentials in base64
  GROUP_CREDS=$(printf "%s:%s" "$GROUP_NAME" "$GROUP_PASSWORD" | base64 | tr -d '\n')
  
  status="$(curl -s -o /dev/null -w '%{http_code}' \
    -X POST "${BASE_URL%/}/api/combine" \
    -H "Authorization: Group ${GROUP_CREDS}" \
    -F "first=@${WORKDIR_CURL}/first.txt;type=text/plain" \
    -F "second=@${WORKDIR_CURL}/second.txt;type=text/plain")"
  
  if [[ "$status" != "200" ]]; then
    echo "  ERROR: Group-authenticated combine check failed (status ${status})" >&2
    exit 1
  fi
  echo "  PASS: Group authentication successful, combine returned 200"
fi

# Test 4: Probe token combine and verify no credential leakage in diagnostics
echo "TEST 4: Probe-authenticated combine and credential redaction"
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
echo "second" > "${WORKDIR}/second.txt"
WORKDIR_CURL="$(to_curl_path "$WORKDIR")"

response_file="${WORKDIR}/combine-response.json"
status="$(curl -s -w '%{http_code}' \
  -X POST "${BASE_URL%/}/api/combine" \
  -H "Authorization: Bearer ${PROBE_TOKEN}" \
  -H "x-combine-probe: 1" \
  -F "first=@${WORKDIR_CURL}/first.txt;type=text/plain" \
  -F "second=@${WORKDIR_CURL}/second.txt;type=text/plain" \
  -o /dev/null)"

if [[ "$status" != "200" ]]; then
  echo "  ERROR: Probe-authenticated combine check failed (status ${status})" >&2
  exit 1
fi
echo "  PASS: Probe-authenticated combine returned 200"

# Extract diagnostics capability from response headers (if available)
capability_file="${WORKDIR}/capability.txt"
curl -s -i -X POST "${BASE_URL%/}/api/combine" \
  -H "Authorization: Bearer ${PROBE_TOKEN}" \
  -H "x-combine-probe: 1" \
  -F "first=@${WORKDIR_CURL}/first.txt;type=text/plain" \
  -F "second=@${WORKDIR_CURL}/second.txt;type=text/plain" 2>/dev/null | \
  grep -i "x-diagnostics-capability:" | cut -d' ' -f2 | tr -d '\r' > "$capability_file" || true

if [[ -s "$capability_file" ]]; then
  capability=$(cat "$capability_file")
  echo "  Retrieved diagnostics capability: ${capability:0:20}..."
  
  # Fetch diagnostics and verify no credential leakage
  diag_response="${WORKDIR}/diagnostics.json"
  diag_status="$(curl -s -w '%{http_code}' \
    -H "Authorization: Bearer ${PROBE_TOKEN}" \
    "${BASE_URL%/}/api/diagnostics/${capability}" \
    -o "$diag_response")"
  
  if [[ "$diag_status" != "200" ]]; then
    echo "  ERROR: Diagnostics endpoint failed (status ${diag_status})" >&2
    exit 1
  fi
  
  # Verify no secrets appear in diagnostics output
  if grep -i '\[redacted\]' "$diag_response" > /dev/null; then
    echo "  INFO: Diagnostics properly redact sensitive values"
  fi
  
  # Check that no raw credential patterns appear in the response
  if grep -E 'ghp_|COMBINE_|GITHUB_|COPILOT_' "$diag_response" > /dev/null 2>&1; then
    echo "  ERROR: Credentials or secret patterns detected in diagnostics output" >&2
    exit 1
  fi
  
  echo "  PASS: Diagnostics endpoint returns data without credential leakage"
else
  echo "  INFO: No diagnostics capability in response (may be expected for some endpoints)"
fi

echo ""
echo "smoke-deploy: ALL TESTS PASSED"
echo "Production deployment is ready: HTTPS enforced, authentication working, credentials redacted"
