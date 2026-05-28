# syntax=docker/dockerfile:1.7
#
# M24 `backend` image: headless juniusd built WITHOUT `embed-frontend`,
# every monorepo plugin linked in. Pairs with the M24 `frontend` image (or
# any third-party static-asset host that proxies `/api`/`/h`/`/rpc` back).
# Browser routes return a structured 404 (`FRONTEND_NOT_EMBEDDED`); the
# FE container handles them.

ARG RUST_VERSION=1.88-bookworm
ARG RUNTIME_IMAGE=gcr.io/distroless/cc-debian12:nonroot

# ---------- Stage 1: rust-only builder (no node, no buf) -------------------
FROM rust:${RUST_VERSION} AS builder
ENV CARGO_TERM_COLOR=always \
    DEBIAN_FRONTEND=noninteractive
# `build-essential` ships gcc + binutils (ld); `rust:<ver>-bookworm` doesn't
# include them by default and cargo's native-dep crates need both to link.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
        build-essential pkg-config libssl-dev \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
# Backend-only image: skip the FE build. `--bundle-all` still produces the
# generated TS files (routes.ts etc.); they're harmless dead code in the
# Rust binary because `embed-frontend` is off.
RUN cargo run --release -p junius -- sync --bundle-all \
 && cargo build --release -p platform \
 && cargo build --release -p junius --no-default-features

# ---------- Stage 2: distroless runtime ------------------------------------
FROM ${RUNTIME_IMAGE} AS runtime
COPY --from=builder /src/target/release/juniusd /usr/local/bin/juniusd
COPY --from=builder /src/target/release/junius  /usr/local/bin/junius
ENV JUNIUS_MODE=precompiled \
    JUNIUS_CONFIG=/etc/junius/platform.toml
VOLUME ["/etc/junius"]
EXPOSE 18080
# Healthcheck via orchestrator (compose / k8s) against `/healthz` —
# distroless has no shell.
ENTRYPOINT ["/usr/local/bin/juniusd"]

LABEL org.opencontainers.image.title="junius-backend" \
      org.opencontainers.image.description="Headless Junius backend (juniusd, no embedded SPA, all monorepo plugins). Pairs with the M24 `frontend` SSR image." \
      org.opencontainers.image.source="https://github.com/${GITHUB_REPOSITORY:-junius/junius}" \
      junius.variant="backend"
