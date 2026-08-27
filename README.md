# file-combiner

Standalone Dioxus (frontend) and Axum (backend) application that combines exactly two authenticated text uploads into one downloadable plaintext file. See the [ROADMAP](../transcripts/27-08-2026_dioxus-axum-file-combiner-plan/ROADMAP.md) dossier for the full plan; this repository implements it waypoint by waypoint.

## Status

Waypoints 1-3 are implemented: the Cargo workspace and shared v1 contract, the authenticated `/api/combine` endpoint with bounded in-memory multipart handling, and the Dioxus browser UI (token entry, two ordered file slots, submit, and download) served same-origin by Axum alongside `/healthz`. The container image and CI/CD land in the final waypoint.

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
   cd frontend && trunk build --release && cd ..
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
```

`scripts/smoke-browser.sh` builds the frontend, starts the backend on an ephemeral token, waits for `/healthz`, then runs the Playwright harness in `frontend/e2e/` against the running instance.
