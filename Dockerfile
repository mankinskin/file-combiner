# syntax=docker/dockerfile:1

# Shared base: every stage below (lint, test, frontend-smoke, builder) needs
# the same toolchain and the same private `agent-builder` Git dependency, so
# CI and Render both go through this single Dockerfile instead of each
# reimplementing their own dependency-fetch mechanism. Docker-specific setup
# lives in scripts/docker/ — see that directory for what each script does.
FROM rust:1.90.0-bookworm AS deps
WORKDIR /app
COPY scripts/docker/ scripts/docker/
RUN bash scripts/docker/install-base-toolchain.sh

COPY . .
# Local/CI builds forward a live ssh-agent (docker build --ssh default).
# Render's build sandbox has no agent socket to forward; it instead injects a
# configured Secret File as a Docker secret mount, and with-git-ssh-key.sh
# points git at that key file directly. Neither mount is required, so
# whichever mechanism the caller didn't wire up is simply absent, not an error.
#
# The registry/git cache mounts persist across builds independent of image
# layer caching, so a source change elsewhere never forces re-downloading
# crates that haven't changed.
RUN --mount=type=ssh --mount=type=secret,id=agent_builder_ssh_key,dst=/run/secrets/agent_builder_ssh_key \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    bash scripts/docker/with-git-ssh-key.sh cargo fetch

# CI target: `docker build --ssh default --target lint .`
FROM deps AS lint
RUN --mount=type=ssh --mount=type=secret,id=agent_builder_ssh_key,dst=/run/secrets/agent_builder_ssh_key \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target,id=cargo-target-lint \
    bash scripts/docker/with-git-ssh-key.sh bash scripts/docker/lint.sh

# CI target: `docker build --ssh default --target test .`
FROM deps AS test
RUN --mount=type=ssh --mount=type=secret,id=agent_builder_ssh_key,dst=/run/secrets/agent_builder_ssh_key \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target,id=cargo-target-test \
    bash scripts/docker/with-git-ssh-key.sh cargo test -p backend

# CI target: `docker build --ssh default --target frontend-smoke .`
FROM deps AS frontend-smoke
RUN bash scripts/docker/install-node.sh
RUN --mount=type=ssh --mount=type=secret,id=agent_builder_ssh_key,dst=/run/secrets/agent_builder_ssh_key \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target,id=cargo-target-frontend-smoke \
    bash scripts/docker/with-git-ssh-key.sh bash scripts/docker/frontend-smoke.sh

# Production image target: `docker build --ssh default -t file-combiner .`
FROM deps AS builder
RUN --mount=type=ssh --mount=type=secret,id=agent_builder_ssh_key,dst=/run/secrets/agent_builder_ssh_key \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target,id=cargo-target-builder \
    bash scripts/docker/with-git-ssh-key.sh bash scripts/docker/build-release.sh

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/dist/backend ./server
COPY --from=builder /app/frontend/dist ./frontend/dist
COPY --from=builder /app/backend/assets ./assets

ENV STATIC_DIR=/app/frontend/dist
ENV ASSETS_DIR=/app/assets
EXPOSE 8080
ENTRYPOINT ["./server"]
