# syntax=docker/dockerfile:1

# Shared base: every stage below (lint, test, frontend-smoke, builder) needs
# the same toolchain and the same private `agent-builder` Git dependency, so
# CI and Render both go through this single Dockerfile instead of each
# reimplementing their own dependency-fetch mechanism.
FROM rust:1.90.0-bookworm AS deps
WORKDIR /app
RUN rustup target add wasm32-unknown-unknown \
    && rustup component add rustfmt clippy \
    && cargo install trunk --version 0.21.14 --locked

# agent-builder is a private Git dependency fetched over SSH; forward the
# caller's ssh-agent (docker build --ssh default) rather than baking in a key.
# .cargo/config.toml (copied in below) makes cargo use the system git/ssh-agent.
RUN apt-get update \
    && apt-get install -y --no-install-recommends openssh-client \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p -m 0700 /root/.ssh \
    && ssh-keyscan -t rsa,ecdsa,ed25519 github.com >> /root/.ssh/known_hosts

COPY . .
# Local/CI builds forward a live ssh-agent (docker build --ssh default).
# Render's build sandbox has no agent socket to forward; it instead injects a
# configured Secret File as a Docker secret mount, and GIT_SSH_COMMAND points
# git at that key file directly. Neither mount is required, so whichever
# mechanism the caller didn't wire up is simply absent, not an error.
RUN --mount=type=ssh --mount=type=secret,id=agent_builder_ssh_key,dst=/run/secrets/agent_builder_ssh_key \
    if [ -f /run/secrets/agent_builder_ssh_key ]; then \
      export GIT_SSH_COMMAND="ssh -i /run/secrets/agent_builder_ssh_key -o UserKnownHostsFile=/root/.ssh/known_hosts -o IdentitiesOnly=yes"; \
    fi && \
    cargo fetch

# CI target: `docker build --ssh default --target lint .`
FROM deps AS lint
RUN --mount=type=ssh --mount=type=secret,id=agent_builder_ssh_key,dst=/run/secrets/agent_builder_ssh_key \
    cargo fmt -- --check && cargo clippy --workspace --all-targets -- -D warnings

# CI target: `docker build --ssh default --target test .`
FROM deps AS test
RUN --mount=type=ssh --mount=type=secret,id=agent_builder_ssh_key,dst=/run/secrets/agent_builder_ssh_key \
    cargo test -p backend

# CI target: `docker build --ssh default --target frontend-smoke .`
FROM deps AS frontend-smoke
RUN apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates gnupg \
    && curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*
RUN --mount=type=ssh --mount=type=secret,id=agent_builder_ssh_key,dst=/run/secrets/agent_builder_ssh_key \
    if [ -f /run/secrets/agent_builder_ssh_key ]; then \
      export GIT_SSH_COMMAND="ssh -i /run/secrets/agent_builder_ssh_key -o UserKnownHostsFile=/root/.ssh/known_hosts -o IdentitiesOnly=yes"; \
    fi && \
    npm ci --prefix frontend/e2e && \
    npx --prefix frontend/e2e playwright install --with-deps chromium && \
    cargo build -p backend && \
    bash scripts/smoke-browser.sh ci-smoke-token

# Production image target: `docker build --ssh default -t file-combiner .`
FROM deps AS builder
RUN --mount=type=ssh --mount=type=secret,id=agent_builder_ssh_key,dst=/run/secrets/agent_builder_ssh_key \
    if [ -f /run/secrets/agent_builder_ssh_key ]; then \
      export GIT_SSH_COMMAND="ssh -i /run/secrets/agent_builder_ssh_key -o UserKnownHostsFile=/root/.ssh/known_hosts -o IdentitiesOnly=yes"; \
    fi && \
    chmod +x ./build.sh && ./build.sh

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/backend ./server
COPY --from=builder /app/frontend/dist ./frontend/dist
COPY --from=builder /app/backend/assets ./assets

ENV STATIC_DIR=/app/frontend/dist
ENV ASSETS_DIR=/app/assets
EXPOSE 8080
ENTRYPOINT ["./server"]
