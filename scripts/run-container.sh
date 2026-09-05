#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE="${IMAGE:-file-combiner:local}"
CONTAINER_NAME="${CONTAINER_NAME:-file-combiner-local}"
PORT="${PORT:-8080}"
COMBINE_BEARER_TOKEN="${COMBINE_BEARER_TOKEN:-local-access-token}"
COMBINE_PROBE_TOKEN="${COMBINE_PROBE_TOKEN:-local-probe-token}"
COMBINE_AGENT_BACKEND="${COMBINE_AGENT_BACKEND:-fake}"

if [[ "$COMBINE_BEARER_TOKEN" == "$COMBINE_PROBE_TOKEN" ]]; then
  echo "COMBINE_BEARER_TOKEN and COMBINE_PROBE_TOKEN must differ." >&2
  exit 1
fi

cd "$REPO_ROOT"

echo "Building ${IMAGE}..."
DOCKER_BUILDKIT=1 docker build --ssh default -t "$IMAGE" .

echo "Starting ${CONTAINER_NAME} at http://127.0.0.1:${PORT}"
echo "Use access key: ${COMBINE_BEARER_TOKEN}"

docker rm -f "$CONTAINER_NAME" > /dev/null 2>&1 || true

args=(
  run --rm
  --name "$CONTAINER_NAME"
  -p "${PORT}:8080"
  -e "COMBINE_BEARER_TOKEN=${COMBINE_BEARER_TOKEN}"
  -e "COMBINE_PROBE_TOKEN=${COMBINE_PROBE_TOKEN}"
  -e "COMBINE_AGENT_BACKEND=${COMBINE_AGENT_BACKEND}"
)

for optional_variable in GITHUB_COPILOT_TOKEN COPILOT_API_KEY COMBINE_GROUP_NAME COMBINE_GROUP_PASSWORD COMBINE_GROUP_COPILOT_KEY; do
  if [[ -n "${!optional_variable:-}" ]]; then
    args+=(-e "${optional_variable}=${!optional_variable}")
  fi
done

exec docker "${args[@]}" "$IMAGE"