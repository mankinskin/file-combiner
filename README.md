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

## Validation

```bash
cargo fmt -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p backend
(cd frontend && trunk build --release)
bash scripts/smoke-browser.sh <disposable-token>
```

`scripts/smoke-browser.sh` builds the frontend, starts the backend on an ephemeral token, waits for `/healthz`, then runs the Playwright harness in `frontend/e2e/` against the running instance.
