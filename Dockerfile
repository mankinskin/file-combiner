# syntax=docker/dockerfile:1

FROM rust:1.90.0-bookworm AS builder
WORKDIR /app
RUN rustup target add wasm32-unknown-unknown \
    && cargo install trunk --version 0.21.14 --locked

COPY . .
RUN chmod +x ./build.sh && ./build.sh

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/backend ./backend
COPY --from=builder /app/frontend/dist ./frontend/dist

ENV STATIC_DIR=/app/frontend/dist
EXPOSE 8080
ENTRYPOINT ["./backend"]
