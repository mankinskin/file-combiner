# syntax=docker/dockerfile:1

# Shared toolchain + cargo-chef, installed once and inherited by every
# downstream stage via Docker's layer cache. Docker-specific setup lives in
# scripts/docker/ — see that directory for what each script does.
FROM rust:1.90.0-bookworm AS chef
WORKDIR /app
COPY scripts/docker/ scripts/docker/
RUN bash scripts/docker/install-base-toolchain.sh
RUN cargo install cargo-chef --locked

# Captures the workspace's dependency graph (Cargo.toml/Cargo.lock only, no
# source), so this recipe only changes when a manifest or lockfile does —
# never on an ordinary source edit.
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Compiles only third-party + `agent-builder` dependencies (host target, dev
# profile, every target) into a real image layer keyed on the recipe alone.
# Unlike a `--mount=type=cache` volume, a normal image layer is
# exportable/importable through `cache-from`/`cache-to: type=gha`, so CI's
# ephemeral runners can actually reuse it — a source-only change never
# re-triggers this (expensive) step.
#
# `cargo clippy` compiles dependencies under its own fingerprint namespace,
# separate from plain `cargo build`/`cargo test` — cooking only once would
# leave clippy's cache cold, so both are cooked here. Each cook's package
# scope must exactly match what the downstream script actually runs
# (`-p backend` vs `--workspace`); a scope mismatch changes cargo's feature
# unification and silently produces a cache-incompatible fingerprint.
FROM chef AS deps-dev
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=ssh --mount=type=secret,id=agent_builder_ssh_key,dst=/run/secrets/agent_builder_ssh_key \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    bash scripts/docker/with-git-ssh-key.sh bash -c " \
      cargo chef cook --recipe-path recipe.json --package backend --all-targets && \
      cargo chef cook --clippy --recipe-path recipe.json --workspace --all-targets"

# Same idea, for the frontend's wasm target in release profile — matching
# how `trunk build --release` actually compiles it.
FROM deps-dev AS deps-wasm
RUN --mount=type=ssh --mount=type=secret,id=agent_builder_ssh_key,dst=/run/secrets/agent_builder_ssh_key \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    bash scripts/docker/with-git-ssh-key.sh cargo chef cook --release --recipe-path recipe.json --target wasm32-unknown-unknown --package frontend

# Same idea again, release profile, for the production backend binary.
# Scoped to `--package backend`, matching `build.sh`'s `cargo build --release
# -p backend` exactly (see the note on deps-dev above).
FROM chef AS deps-release
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=ssh --mount=type=secret,id=agent_builder_ssh_key,dst=/run/secrets/agent_builder_ssh_key \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    bash scripts/docker/with-git-ssh-key.sh cargo chef cook --release --recipe-path recipe.json --package backend

# Adds the same frontend wasm32 release cook on top, for the production
# image's `trunk build --release` step.
FROM deps-release AS deps-release-wasm
RUN --mount=type=ssh --mount=type=secret,id=agent_builder_ssh_key,dst=/run/secrets/agent_builder_ssh_key \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    bash scripts/docker/with-git-ssh-key.sh cargo chef cook --release --recipe-path recipe.json --target wasm32-unknown-unknown --package frontend

# CI target: `docker build --ssh default --target lint .`
FROM deps-dev AS lint
COPY . .
RUN --mount=type=ssh --mount=type=secret,id=agent_builder_ssh_key,dst=/run/secrets/agent_builder_ssh_key \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    bash scripts/docker/with-git-ssh-key.sh bash scripts/docker/lint.sh

# CI target: `docker build --ssh default --target test .`
FROM deps-dev AS test
COPY . .
RUN --mount=type=ssh --mount=type=secret,id=agent_builder_ssh_key,dst=/run/secrets/agent_builder_ssh_key \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    bash scripts/docker/with-git-ssh-key.sh cargo test -p backend

# CI target: `docker build --ssh default --target frontend-smoke .`
FROM deps-wasm AS frontend-smoke
RUN bash scripts/docker/install-node.sh
COPY . .
RUN --mount=type=ssh --mount=type=secret,id=agent_builder_ssh_key,dst=/run/secrets/agent_builder_ssh_key \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    bash scripts/docker/with-git-ssh-key.sh bash scripts/docker/frontend-smoke.sh

# Production image target: `docker build --ssh default -t file-combiner .`
FROM deps-release-wasm AS builder
COPY . .
RUN --mount=type=ssh --mount=type=secret,id=agent_builder_ssh_key,dst=/run/secrets/agent_builder_ssh_key \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
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
