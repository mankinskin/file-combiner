#!/usr/bin/env bash
set -euo pipefail

IMAGE="file-combiner:local"
CONTAINER_NAME="file-combiner-local"
PORT="${PORT:-8080}"
BASE_URL="http://127.0.0.1:${PORT}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! docker info >/dev/null 2>&1; then
  echo "Starting Docker Desktop..."
  powershell.exe -NoProfile -Command 'Start-Process -FilePath "$env:ProgramFiles\Docker\Docker\Docker Desktop.exe"'
  for _ in $(seq 1 120); do
    if docker info >/dev/null 2>&1; then
      break
    fi
    sleep 1
  done
  docker info >/dev/null 2>&1 || {
    echo "Docker Desktop did not become ready within two minutes." >&2
    exit 1
  }
fi

if ! ssh-add -l >/dev/null 2>&1; then
  echo "File Combiner Docker build needs a running SSH agent with access to github.com/mankinskin/agent-builder." >&2
  echo "On Windows, enable and start OpenSSH Authentication Agent, then run: ssh-add ~/.ssh/agent-builder_ed25519" >&2
  exit 1
fi

cd "$REPO_ROOT"
export DOCKER_BUILDKIT=1
# Docker Desktop's Windows file sharing can report a `COPY . .` layer as
# unchanged even after a source edit, silently reusing a stale `planner`/
# `builder` layer (and every step after it) instead of recompiling. Force
# those two source-copying stages to always re-run; the slow `deps-*`
# dependency-cook layers, keyed only on Cargo.lock/recipe.json, still cache
# normally.
docker build --ssh default --no-cache-filter=planner,builder -t "$IMAGE" .
docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
docker run -d --rm \
  --name "$CONTAINER_NAME" \
  -p "${PORT}:8080" \
  -e COMBINE_BEARER_TOKEN=file-combiner-local-token \
  -e COMBINE_PROBE_TOKEN=file-combiner-local-probe-token \
  "$IMAGE" >/dev/null

for _ in $(seq 1 30); do
  if curl -sf "${BASE_URL}/healthz" >/dev/null; then
    echo "File Combiner server is available at ${BASE_URL}"
    exit 0
  fi
  sleep 1
done

echo "${CONTAINER_NAME} did not become healthy" >&2
exit 1