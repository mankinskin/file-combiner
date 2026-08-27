# file-combiner

Standalone Dioxus (frontend) and Axum (backend) application that combines exactly two authenticated text uploads into one downloadable plaintext file. See the [ROADMAP](../transcripts/27-08-2026_dioxus-axum-file-combiner-plan/ROADMAP.md) dossier for the full plan; this repository implements it waypoint by waypoint.

## Status

All four waypoints are implemented: the Cargo workspace and shared v1 contract, the authenticated `/api/combine` endpoint with bounded in-memory multipart handling, the Dioxus browser UI served same-origin by Axum, and the container image with GitHub Actions CI/CD to Render.

## Runtime configuration

The backend reads these environment variables at startup and refuses to bind if they are missing, empty, or invalid. No values are stored in source control.

- `COMBINE_BEARER_TOKEN` - shared trusted-group password for browser requests. Not individual user identity.
- `COMBINE_PROBE_TOKEN` - distinct token accepted only on the internal probe path (CI/CD post-deploy smoke check); never valid for browser-originated requests.
- `PORT` - TCP port to bind (defaults to `8080` when unset).
- `STATIC_DIR` - directory containing the built frontend assets served at every path other than `/healthz` and `/api/*` (defaults to `frontend/dist`).

## Prerequisites

On a fresh machine with Bash, Docker Engine, and Node.js `22.14.0` with npm:

```bash
rustup toolchain install 1.90.0
rustup target add wasm32-unknown-unknown --toolchain 1.90.0
cargo install trunk --version 0.21.14 --locked
```

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

3. Open [http://127.0.0.1:8080](http://127.0.0.1:8080) in a browser. Enter the same value you set for `COMBINE_BEARER_TOKEN` in the **Token** field, choose a first and second text file (each under 1 MiB), then click **Combine and download** to receive `combined.txt`.

Re-run step 1 after any frontend change; the backend only needs restarting after a backend change (`STATIC_DIR` always points at the same `frontend/dist` folder built in step 1).

## Validation

```bash
cargo fmt -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p backend
(cd frontend && trunk build --release)
bash scripts/smoke-browser.sh <disposable-token>
docker build -t file-combiner:local .
bash scripts/smoke-container.sh file-combiner:local <group-token> <probe-token>
```

`scripts/smoke-browser.sh` builds the frontend, starts the backend on an ephemeral token, waits for `/healthz`, then runs the Playwright harness in `frontend/e2e/` against the running instance.

`scripts/smoke-container.sh` starts the built image with disposable tokens, waits for `/healthz`, then exercises success, unauthorized, wrong-count, oversized, invalid-UTF-8, and probe-header fixtures against the running container.

## Container image

The repository root `Dockerfile` is a two-stage build: the `builder` stage compiles the Trunk frontend and the release `backend` binary from the same Rust workspace, and the `runtime` stage copies only the built binary and `frontend/dist` into a slim Debian image. The backend binds to `0.0.0.0:$PORT` and serves the frontend, `/healthz`, and `/api/*` from that single process — set `COMBINE_BEARER_TOKEN` and `COMBINE_PROBE_TOKEN` when running the container, since the image itself never bakes in any secret value.

## CI/CD

`.github/workflows/ci.yml` runs four validation jobs on every push and pull request: `rust-quality` (fmt + clippy), `backend-contract` (`cargo test -p backend`), `frontend-browser` (Trunk build plus the Playwright smoke), and `container-api` (Docker build plus the container smoke). A fifth job, `deploy-render`, depends on all four and only runs on a push to the protected `production` branch; it calls `scripts/deploy-render.sh` to create and poll a Render deployment through the Render API, then `scripts/smoke-deploy.sh` to verify `/healthz` and a probe-authenticated combine request post-deploy. Neither script ever prints a token, an authorization header, or file content.

### Operator prerequisites

Before the first production deployment, an operator must:

- Protect the `production` branch in GitHub.
- Create the Render service for this repository.
- Store distinct, non-empty `COMBINE_BEARER_TOKEN` and `COMBINE_PROBE_TOKEN` values as Render environment secrets.
- Configure GitHub Actions repository secrets: `RENDER_API_KEY`, `RENDER_SERVICE_ID`, `RENDER_EXTERNAL_URL`, and a `COMBINE_PROBE_TOKEN` matching the value stored in Render.
