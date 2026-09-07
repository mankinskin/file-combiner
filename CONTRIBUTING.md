# Contributing to File Combiner

File Combiner is a Rust workspace with an Axum backend and a Dioxus/Trunk
frontend. The backend serves the built frontend from the same origin.

## Before You Start

Install the project toolchain:

```bash
rustup toolchain install 1.90.0
rustup target add wasm32-unknown-unknown --toolchain 1.90.0
cargo install trunk --version 0.21.14 --locked
```

The workspace depends on the private `mankinskin/agent-builder` repository.
Add a key that has read access to that repository to your SSH agent before
running Cargo or Docker commands:

```bash
ssh-add ~/.ssh/agent-builder_ed25519
ssh -T git@github.com
```

Docker builds also pull the private `infrastructure-base` image. Sign in to
GitHub Container Registry with a token that has `read:packages` access when
the image is not already available locally:

```bash
docker login ghcr.io
```

Never commit tokens, private keys, generated `frontend/dist` assets, or local
environment files.

## Development Workflow

1. Start from an up-to-date `main` branch and make a focused change.
2. Keep the backend boundary clear:
   - `backend/src/combinator_agent/` owns template handling, artifact creation,
     and agent execution.
   - `backend/src/combine/` owns HTTP routing, authorization, multipart input,
     and retained artifact delivery.
   - Keep public imports routed through each module's `mod.rs` facade.
3. Add or update a focused regression test for changed behavior.
4. Run the checks in [Verification](#verification) before opening a pull
   request.

For a local browser workflow, build the frontend and start the backend with
distinct non-empty local tokens:

```bash
(cd frontend && trunk build --release)
COMBINE_BEARER_TOKEN=my-local-token \
COMBINE_PROBE_TOKEN=my-local-probe-token \
COMBINE_AGENT_BACKEND=fake \
cargo run -p backend
```

Open `http://127.0.0.1:8080` and enter `my-local-token` as the access key.
`COMBINE_AGENT_BACKEND=fake` is suitable for deterministic UI work; omit that
variable and provide a valid Copilot credential when validating real agent
execution.

The meta-workspace task runs the containerized local workflow:

```text
File Combiner: Start local Docker container
```

The task invokes `scripts/start-local-container.sh`. The script starts Docker
Desktop when needed, builds `file-combiner:local` with SSH forwarding, replaces
the `file-combiner-local` container, and waits for `/healthz`.

## Verification

Run the narrowest relevant check while developing. Before submitting a change,
run the full applicable set from the repository root:

```bash
cargo fmt -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p backend
(cd frontend && trunk build --release)
```

For HTTP and browser changes, also run:

```bash
bash scripts/smoke-browser.sh <disposable-token>
```

For Dockerfile, runtime configuration, or container-delivery changes, also run:

```bash
DOCKER_BUILDKIT=1 docker build --ssh default -t file-combiner:local .
bash scripts/smoke-container.sh file-combiner:local <group-token> <probe-token>
```

The CI workflow runs the same four gates: Rust quality, backend contract tests,
frontend browser smoke, and container API smoke. A pull request is ready only
when the relevant local checks pass or a concrete environment blocker is
documented in the pull request.

## Pull Requests

Keep each pull request limited to one behavior or architectural unit. Describe
the user-visible effect, tests added or updated, and commands run. Call out
configuration, migration, security, or compatibility changes explicitly.

Do not combine unrelated cleanup with feature work. Preserve diagnostics
redaction: request content, credentials, prompts, and filesystem paths must not
be exposed to browser-visible diagnostic messages.