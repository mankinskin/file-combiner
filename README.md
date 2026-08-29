# file-combiner

Standalone Dioxus (frontend) and Axum (backend) application that combines exactly two authenticated text uploads into one downloadable plaintext file. See the [ROADMAP](../transcripts/27-08-2026_dioxus-axum-file-combiner-plan/ROADMAP.md) dossier for the full plan; this repository implements it waypoint by waypoint.

## Status

All four original waypoints, plus the browser UX reconciliation described in the [file-combiner UI reconciliation dossier](../transcripts/27-08-2026_app-ui-design-auth-crates/README.md), are implemented: the Cargo workspace and shared v1 contract, the authenticated `/api/combine` endpoint with bounded in-memory multipart handling, the centered Dioxus document workspace served same-origin by Axum, and the container image with GitHub Actions CI/CD to Render.

## Browser UX contract

- **Two ordered upload slots.** Each slot locally decodes the selected file with fatal (strict) UTF-8 decoding. A decoding failure rejects that slot with an accessible error, blocks submission, and shows no preview; it never falls back to binary/hex display.
- **Read-only previews only.** Valid text is rendered as sanitized, scrollable, keyboard-focusable text — never as injected HTML. Control characters other than tab/newline are visibly replaced. The preview cannot edit the file; the original selected bytes are what is submitted.
- **Privacy boundary.** Local validation and preview never leave the browser, and the workspace never displays raw backend logs, request headers, credentials, or server metadata — only the four fixed processing stages (access validation, file reading, combining, preparing download) and the final result.
- **Retry and reset.** A failed request keeps the access key, file selections, and previews in memory for retry. An explicit Reset, a successful download, or a page reload clears all local state.
- **Result.** A successful combine shows the response text in a read-only preview plus an explicit download action; the downloaded bytes are exactly the endpoint response (`first + "\n" + second`).

## Runtime configuration

The backend reads these environment variables at startup and refuses to bind if they are missing, empty, or invalid. No values are stored in source control.

- `COMBINE_BEARER_TOKEN` - shared trusted-group password for browser requests. Not individual user identity.
- `COMBINE_PROBE_TOKEN` - distinct token accepted only on the internal probe path (CI/CD post-deploy smoke check); never valid for browser-originated requests.
- `PORT` - TCP port to bind (defaults to `8080` when unset).
- `STATIC_DIR` - directory containing the built frontend assets served at every path other than `/healthz` and `/api/*` (defaults to `frontend/dist`).
- `ASSETS_DIR` - directory containing the `combinator-agent` configuration and template (`agent-builder.toml`, `templates/combinator-agent.md`); defaults to `backend/assets`, which is correct for `cargo run` from the repository root. The container image sets this to `/app/assets`.
- `COMBINE_AGENT_BACKEND` - set to `fake` to combine with plain concatenation instead of invoking the real Copilot-backed agent. Only for smoke tests (`scripts/smoke-browser.sh` and `scripts/smoke-container.sh` set this by default) that must not depend on live model credentials; leave unset in real deployments.

`/api/combine` runs a thin Agent Builder-backed `combinator-agent` (see `backend/assets/`) that holds the server's own Copilot credential internally; it is never sent to the browser, logged, or exposed in responses. The agent may create exactly one server-selected, bounded UTF-8 file in a fresh request-scoped temporary directory, which is deleted once the response is served or the request otherwise ends. A successful or failed combine response also carries an `X-Diagnostics-Capability` header: an opaque 256-bit token. Presenting that capability together with `COMBINE_BEARER_TOKEN` at `GET /api/diagnostics/{capability}` returns bounded, redacted, structured records for that request only, reusable for 15 minutes and then permanently gone. An invalid bearer returns `401`; a missing, invalid, expired, or already-cleaned capability returns `404`. Diagnostics never include raw process output, upload contents, secrets, or tool arguments.

## Prerequisites

On a fresh machine with Bash, Docker Engine, and Node.js `22.14.0` with npm:

```bash
rustup toolchain install 1.90.0
rustup target add wasm32-unknown-unknown --toolchain 1.90.0
cargo install trunk --version 0.21.14 --locked
```

### Access to the private `agent-builder` dependency

`backend` depends on `agent-builder` as a private Git dependency over SSH (`ssh://git@github.com/mankinskin/agent-builder`). Cargo and Docker never receive a copy of your private key; they use your running `ssh-agent` instead.

1. Generate a dedicated key (do not reuse an existing personal key) and add it to your agent:

   ```bash
   ssh-keygen -t ed25519 -f ~/.ssh/agent-builder_ed25519 -C "file-combiner build access"
   ssh-add ~/.ssh/agent-builder_ed25519
   ```

2. Ask the `mankinskin/agent-builder` repository owner to add the **public** key (`~/.ssh/agent-builder_ed25519.pub`) as a read-only [deploy key](https://docs.github.com/en/authentication/connecting-to-github-with-ssh/managing-deploy-keys#deploy-keys), or add it yourself if you own the repository. Never share or commit the private key.
3. Confirm access: `ssh -T git@github.com` should greet you by the identity tied to that key.
4. Local `cargo build`/`cargo test`/`cargo check` pick up the agent automatically; no extra flags are needed.
5. `docker build` needs the agent forwarded explicitly with BuildKit enabled:

   ```bash
   DOCKER_BUILDKIT=1 docker build --ssh default -t file-combiner:local .
   ```

CI loads the same kind of key from the `AGENT_BUILDER_DEPLOY_KEY` repository secret via `webfactory/ssh-agent` and forwards it into both `cargo` steps and the Docker build (`--ssh default`). Rotate that secret the same way you would rotate any other deploy credential.

## Run it locally

The frontend and backend are served from a single Axum process, so there's one build step and one run step.

1. Build the browser frontend (produces `frontend/dist`):

   ```bash
   (cd frontend && trunk build --release)
   ```

2. Start the backend with your own tokens (pick any non-empty, distinct values):

   ```bash
   COMBINE_BEARER_TOKEN=my-local-token COMBINE_PROBE_TOKEN=my-local-probe-token cargo run -p backend
   ```

   By default this binds to `http://127.0.0.1:8080` and serves the frontend from `frontend/dist`.

3. Open [http://127.0.0.1:8080](http://127.0.0.1:8080) in a browser. Enter the same value you set for `COMBINE_BEARER_TOKEN` in the **Access key** field, choose a first and second UTF-8 text file (each under 1 MiB) to see their read-only previews, then click **Combine** to see the processing panel and, on success, the result preview and **Download combined.txt** action.

Re-run step 1 after any frontend change; the backend only needs restarting after a backend change (`STATIC_DIR` always points at the same `frontend/dist` folder built in step 1).

## Validation

```bash
cargo fmt -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p backend
(cd frontend && trunk build --release)
bash scripts/smoke-browser.sh <disposable-token>
DOCKER_BUILDKIT=1 docker build --ssh default -t file-combiner:local .
bash scripts/smoke-container.sh file-combiner:local <group-token> <probe-token>
```

Every step above resolves the private `agent-builder` dependency through your `ssh-agent`; see [Access to the private `agent-builder` dependency](#access-to-the-private-agent-builder-dependency) if any step fails to fetch it.

`scripts/smoke-browser.sh` builds the frontend, starts the backend on an ephemeral token, waits for `/healthz`, then runs the Playwright harness in `frontend/e2e/` against the running instance.

`scripts/smoke-container.sh` starts the built image with disposable tokens, waits for `/healthz`, then exercises success, unauthorized, wrong-count, oversized, invalid-UTF-8, and probe-header fixtures against the running container.

## Container image

The repository root `Dockerfile` builds `FROM` the `infrastructure` repo's published `infrastructure-base` GHCR image (digest-pinned; see [Migration notes](#migration-notes-package-07-adopting-infrastructuretemplate) below), which supplies the shared Rust/wasm/trunk toolchain, `cargo-chef`, and the generic `scripts/docker/*` helpers this Dockerfile now inherits instead of vendoring its own copies. On top of that base, this repo's own `planner`/`deps-*`/`lint`/`test`/`frontend-smoke`/`builder`/`runtime` stages compile the Trunk frontend and the release `backend` binary from this Rust workspace; the `runtime` stage copies only the built binary and `frontend/dist` into a slim Debian image. The backend binds to `0.0.0.0:$PORT` and serves the frontend, `/healthz`, and `/api/*` from that single process — set `COMBINE_BEARER_TOKEN` and `COMBINE_PROBE_TOKEN` when running the container, since the image itself never bakes in any secret value.

The `deps-*`/`lint`/`test`/`frontend-smoke`/`builder` stages fetch `agent-builder` over SSH using a forwarded `ssh-agent` (`RUN --mount=type=ssh`), via the base image's inherited `scripts/docker/with-git-ssh-key.sh` (pointed at this repo's private dependency through the `SSH_SECRET_FILE`/`SSH_VERIFY_REMOTE_URL` env vars set once in the `chef` stage); it never bakes a private key into any layer. Build with `DOCKER_BUILDKIT=1 docker build --ssh default ...` as shown above, and see the SSH access section for how to whitelist a key. The private `infrastructure-base` GHCR package additionally requires `docker login ghcr.io` with `read:packages` access; without that, build with `--build-arg BASE_IMAGE=infrastructure-base:local` after building it locally in a sibling `infrastructure` checkout (`docker build -t infrastructure-base:local --target base .`).

## Migration notes (package 07: adopting infrastructure/template)

This repo's Docker/CI layer now depends on the `infrastructure` repo instead of vendoring its own toolchain setup. Docker target names, build contexts, and CI job names are unchanged — this is a behavior-equivalent migration, not a rewrite of the app.

| Surface | Before | After | Rollback step |
|---|---|---|---|
| Docker build targets | `chef` stage built `FROM rust:1.90.0-bookworm`, installing the toolchain via this repo's own `scripts/docker/install-base-toolchain.sh` + `cargo install cargo-chef` | `chef` stage is `FROM ${BASE_IMAGE}` (default: the pinned `infrastructure-base` digest), which already has the toolchain and `cargo-chef` installed; `planner`/`deps-*`/`lint`/`test`/`frontend-smoke`/`builder`/`runtime` stage names and behavior unchanged | `git revert` this branch's commits; restore `Dockerfile` from commit `5f126e5` (pre-refactor) |
| `scripts/docker/*` | This repo vendored its own copies of `install-base-toolchain.sh`, `with-git-ssh-key.sh` (hardcoded to `agent-builder`), `install-node.sh`, `lint.sh`, `build-release.sh`, `frontend-smoke.sh` | Removed; the base image supplies genericized equivalents. `with-git-ssh-key.sh`'s hardcoded `agent-builder` references became the `SSH_SECRET_FILE`/`SSH_VERIFY_REMOTE_URL` env vars set in the `chef` stage | Restore `scripts/docker/` from commit `5f126e5` and point the `chef` stage back at `FROM rust:1.90.0-bookworm` |
| CI job triggers | `.github/workflows/ci.yml` jobs `rust-quality`, `backend-contract`, `frontend-browser`, `container-api`, `deploy-render` | Unchanged — same job names, same Docker `target:` values, since this Dockerfile keeps the same stage names `infrastructure`'s `templates/ci-template.yml` also uses | No change needed; CI config untouched by this migration |
| Secret/env names | `AGENT_BUILDER_DEPLOY_KEY`, `RENDER_API_KEY`, `RENDER_SERVICE_ID`, `RENDER_EXTERNAL_URL`, `COMBINE_PROBE_TOKEN` | Unchanged. The Render deploy scripts (`scripts/deploy-render.sh`, `scripts/smoke-deploy.sh`) remain owned by this repo — `infrastructure` does not yet implement a `providers/render/` adapter, so this is a deliberate, documented deviation from the plan's original wording, not an oversight | No rollback needed; nothing changed here |
| Private-dependency access | BuildKit `--ssh` / Render Secret File via this repo's own `scripts/docker/with-git-ssh-key.sh` | Same mechanism, now via the base image's inherited, genericized `scripts/docker/with-git-ssh-key.sh`, targeted at `agent-builder` through env vars instead of hardcoding | Restore `scripts/docker/with-git-ssh-key.sh` from commit `5f126e5` |
| Base image reference | N/A (built `FROM rust:1.90.0-bookworm` from scratch each time) | `ARG BASE_IMAGE` defaults to the digest-verified `ghcr.io/mankinskin/infrastructure-base@sha256:d0823f9fa4b323a2abdc23058b22dfdf947489e6558a1d4e0c180dcceec61247` (proven pullable by package 06's `verify-digest-pull.sh`); override with `--build-arg BASE_IMAGE=infrastructure-base:local` if unavailable | Revert to building `FROM rust:1.90.0-bookworm` from scratch (restore `Dockerfile` from commit `5f126e5`) if the digest becomes unpullable |

**Rollback decision thresholds:** (1) if any smoke fixture result (`scripts/smoke-container.sh`, `scripts/smoke-browser.sh`, `scripts/smoke-deploy.sh`) changes from the pre-refactor baseline, do not merge this branch — fix or roll back first; (2) if the pinned digest becomes unpullable after merge (registry outage, retention expiry), temporarily restore `Dockerfile` from commit `5f126e5` while `infrastructure` republishes; (3) if the private-dependency handoff fails, restore `scripts/docker/with-git-ssh-key.sh` from commit `5f126e5` as an interim fix.

**Validation status:** `cargo fmt --check`, `cargo clippy --workspace --all-targets`, and `cargo test -p backend` were re-run locally against this branch and pass unchanged. `docker build --target lint` was attempted locally and confirmed the base-image swap itself works correctly (the `chef`/`planner` stages pull the pinned digest and build cleanly); it then fails, as expected and unchanged from before this migration, at the `agent-builder` SSH-forwarding step, since this authoring environment has no deploy key for that private repository. The remaining Docker-based checks (`--target test`/`frontend-smoke`/`builder`, and `scripts/smoke-container.sh`/`smoke-browser.sh` against a built image) require the same live `ssh-agent` access, available only in this repo's own CI (`AGENT_BUILDER_DEPLOY_KEY`) — they must be confirmed via a gated CI run on this branch before merge, per this package's own validation method (item 3: "succeed locally or in a gated branch").

## CI/CD

`.github/workflows/ci.yml` runs four validation jobs on every push and pull request: `rust-quality` (fmt + clippy), `backend-contract` (`cargo test -p backend`), `frontend-browser` (Trunk build plus the Playwright smoke), and `container-api` (Docker build plus the container smoke). Every job loads the `AGENT_BUILDER_DEPLOY_KEY` repository secret into an `ssh-agent` (via `webfactory/ssh-agent`) and forwards it to `cargo` and to `docker build --ssh default`, since all four resolve the private `agent-builder` dependency. A fifth job, `deploy-render`, depends on all four and only runs on a push to the protected `production` branch; it calls `scripts/deploy-render.sh` to create and poll a Render deployment through the Render API, then `scripts/smoke-deploy.sh` to verify `/healthz` and a probe-authenticated combine request post-deploy. Neither script ever prints a token, an authorization header, or file content.

### Operator prerequisites

Before the first production deployment, an operator must:

- Protect the `production` branch in GitHub.
- Create the Render service for this repository.
- Store distinct, non-empty `COMBINE_BEARER_TOKEN` and `COMBINE_PROBE_TOKEN` values as Render environment secrets.
- Store a `GITHUB_COPILOT_TOKEN` or `COPILOT_API_KEY` value as a Render environment secret. This is the credential the live `combinator-agent` uses at request time (distinct from `AGENT_BUILDER_DEPLOY_KEY`, which only grants build-time access to the private `agent-builder` source); without it, every real `/api/combine` request fails with "the combine agent failed to run" even though the deploy itself succeeds.
- Configure GitHub Actions repository secrets: `RENDER_API_KEY`, `RENDER_SERVICE_ID`, `RENDER_EXTERNAL_URL`, a `COMBINE_PROBE_TOKEN` matching the value stored in Render, and `AGENT_BUILDER_DEPLOY_KEY` (a private key whose public half is whitelisted as a read-only deploy key on `mankinskin/agent-builder`).
- In the Render service's *Environment* settings, add a **Secret File** named `agent_builder_ssh_key` containing that same private key's contents. Render builds its own Docker image server-side from this repo — it has no persistent host key to pre-whitelist and no `ssh-agent` socket to forward, so this Secret File is Render's equivalent of the `--ssh default` forwarding used locally and in CI. The Dockerfile mounts it at build time and points `git` at it directly; it is never baked into the image.

Note that Render's GitHub App connection (used to clone *this* repo, and any of its own Git submodules) does not extend to `agent-builder`: that dependency is fetched by `cargo`/`git` from inside the Docker build, an independent process Render's OAuth scope doesn't cover.
