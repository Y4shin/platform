# syntax=docker/dockerfile:1.7
#
# M24 `full` image: single-container deployment topology. `juniusd` built
# with `--features embed-frontend`, every monorepo plugin linked in, the SPA
# bundled into the binary via `rust-embed`. Pair with postgres + (your IdP);
# no separate FE container. The image entrypoint sets `JUNIUS_MODE=precompiled`
# so the boot check refuses to start unless `[plugins].enabled` matches the
# bundled set exactly.
#
# Operators run migrations via:
#   docker run --rm <image> junius migrate up --config /etc/junius/platform.toml
# then start the long-running container as normal.

ARG RUST_VERSION=1.88-bookworm
ARG NODE_VERSION=22-bookworm-slim
ARG RUNTIME_IMAGE=gcr.io/distroless/cc-debian12:nonroot

# ---------- Stage 1: bring up node + pnpm + buf in a rust-builder base -----
FROM rust:${RUST_VERSION} AS builder
ENV CARGO_TERM_COLOR=always \
    DEBIAN_FRONTEND=noninteractive \
    PNPM_HOME=/usr/local/pnpm \
    PATH=/usr/local/pnpm:$PATH
# Node 22 + pnpm via corepack; buf binary pinned via the official release.
# `rust:<ver>-bookworm` ships gcc but NOT binutils — cargo's native-dep
# crates fail to link with `cannot find 'ld'` without it. Install binutils
# + gcc + libc6-dev explicitly (the `build-essential` metapackage's
# Depends are partially pre-satisfied in the base image, which is why
# pulling just `build-essential` isn't enough).
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
        binutils gcc g++ make libc6-dev \
        ca-certificates curl gnupg pkg-config libssl-dev \
 && curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
 && apt-get install -y --no-install-recommends nodejs \
 && corepack enable \
 && corepack prepare pnpm@11.1.1 --activate \
 && BUF_VERSION=1.50.0 \
 && curl -fsSL -o /usr/local/bin/buf \
        "https://github.com/bufbuild/buf/releases/download/v${BUF_VERSION}/buf-Linux-x86_64" \
 && chmod +x /usr/local/bin/buf \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /src
# Copy the workspace. Lockfiles (Cargo.lock + pnpm-lock.yaml) drive layer
# caching; the .dockerignore strips target/ + node_modules/ + examples/.
COPY . .

# 1) Bundle-all sync: enumerates every plugin under `plugins/` and writes
#    the codegen files into the source tree. No deployment toml consulted.
RUN cargo run --release -p junius -- sync --bundle-all

# 2) FE bundle (embedded into the binary in the next step).
RUN pnpm install --frozen-lockfile \
 && pnpm --filter @junius/shell build

# 3) Host binary with the SPA embedded + the trimmed in-container junius CLI.
RUN cargo build --release -p platform --features embed-frontend \
 && cargo build --release -p junius --no-default-features

# ---------- Stage 2: distroless runtime ------------------------------------
FROM ${RUNTIME_IMAGE} AS runtime
COPY --from=builder /src/target/release/juniusd /usr/local/bin/juniusd
COPY --from=builder /src/target/release/junius  /usr/local/bin/junius
# The deployment mounts its `platform.toml` (+ any `file:`-mode secrets)
# under `/etc/junius/`. `JUNIUS_MODE=precompiled` makes the boot check
# enforce an exact bundle/enabled match.
ENV JUNIUS_MODE=precompiled \
    JUNIUS_CONFIG=/etc/junius/platform.toml
VOLUME ["/etc/junius"]
EXPOSE 18080
# No in-image HEALTHCHECK: distroless lacks a shell + curl/wget. Use the
# orchestrator's probe (docker-compose `healthcheck:` or k8s
# readinessProbe) against the auth-free `/healthz` endpoint that
# `juniusd` exposes.
ENTRYPOINT ["/usr/local/bin/juniusd"]

LABEL org.opencontainers.image.title="junius-full" \
      org.opencontainers.image.description="Single-container Junius deployment (juniusd + embedded SPA + all monorepo plugins)" \
      org.opencontainers.image.source="https://github.com/${GITHUB_REPOSITORY:-junius/junius}" \
      junius.variant="full"
