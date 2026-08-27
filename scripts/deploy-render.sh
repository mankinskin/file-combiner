#!/usr/bin/env bash
set -euo pipefail

API_KEY="${1:?Usage: deploy-render.sh <api-key> <service-id>}"
SERVICE_ID="${2:?Usage: deploy-render.sh <api-key> <service-id>}"
API_BASE="https://api.render.com/v1"
POLL_INTERVAL_SECS=15
MAX_WAIT_SECS=600

# Never trace this script: the API key must not appear in any log.
set +x

create_response="$(curl -sf -X POST \
  -H "Authorization: Bearer ${API_KEY}" \
  -H "Content-Type: application/json" \
  -d '{}' \
  "${API_BASE}/services/${SERVICE_ID}/deploys")"

deploy_id="$(printf '%s' "$create_response" | node -e '
let data = "";
process.stdin.on("data", (chunk) => (data += chunk));
process.stdin.on("end", () => {
  const parsed = JSON.parse(data);
  process.stdout.write(String(parsed.id ?? parsed.deploy?.id ?? ""));
});
')"

if [[ -z "$deploy_id" ]]; then
  echo "deploy-render: failed to read a deploy id from the Render API response" >&2
  exit 1
fi

echo "deploy-render: created deployment ${deploy_id}, polling for completion"

elapsed=0
status=""
while (( elapsed < MAX_WAIT_SECS )); do
  status="$(curl -sf \
    -H "Authorization: Bearer ${API_KEY}" \
    "${API_BASE}/services/${SERVICE_ID}/deploys/${deploy_id}" \
    | node -e '
let data = "";
process.stdin.on("data", (chunk) => (data += chunk));
process.stdin.on("end", () => {
  const parsed = JSON.parse(data);
  process.stdout.write(String(parsed.status ?? ""));
});
')"

  case "$status" in
    live)
      echo "deploy-render: deployment ${deploy_id} is live"
      exit 0
      ;;
    build_failed|update_failed|canceled|deactivated)
      echo "deploy-render: deployment ${deploy_id} ended in terminal state '${status}'" >&2
      exit 1
      ;;
  esac

  sleep "$POLL_INTERVAL_SECS"
  elapsed=$((elapsed + POLL_INTERVAL_SECS))
done

echo "deploy-render: deployment ${deploy_id} did not reach a terminal state within ${MAX_WAIT_SECS}s (last status: '${status}')" >&2
exit 1
