# file-combiner

Standalone Dioxus (frontend) and Axum (backend) application that combines exactly two authenticated text uploads into one downloadable plaintext file. See the [ROADMAP](../transcripts/27-08-2026_dioxus-axum-file-combiner-plan/ROADMAP.md) dossier for the full plan; this repository implements it waypoint by waypoint.

## Status

Waypoint 1 (foundation and contract) is implemented: the Cargo workspace, shared v1 contract (capacity limits, JSON error schema), configuration validation, and `GET /healthz`. The authenticated `/api/combine` handler, browser UI, container image, and CI/CD land in later waypoints.

## Runtime configuration

The backend reads these environment variables at startup and refuses to bind if they are missing, empty, or invalid. No values are stored in source control.

- `COMBINE_BEARER_TOKEN` - shared trusted-group password for browser requests. Not individual user identity.
- `COMBINE_PROBE_TOKEN` - distinct token accepted only on the internal probe path (CI/CD post-deploy smoke check); never valid for browser-originated requests.
- `PORT` - TCP port to bind (defaults to `8080` when unset).

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
```
