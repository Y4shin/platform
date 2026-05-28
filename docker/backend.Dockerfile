# syntax=docker/dockerfile:1.7
#
# M24 `backend` image: headless juniusd built without `embed-frontend`,
# every monorepo plugin linked in. Pairs with the M24 `frontend` image
# (or any third-party static-asset host). Browser routes return a
# structured 404 (`FRONTEND_NOT_EMBEDDED`); the FE container handles them.
#
# Build strategy (M24, post-musl-pivot): the Rust binaries are built
# via `nix build .#…-static` against the flake derivations in
# `flake.nix`. The result is fully static, musl-linked, position-
# independent executables that run on a `FROM scratch` runtime base.
# No glibc, no shell, no toolchain in the runtime image.

# nixos/nix ships with experimental features off by default; the
# wrapper flag enables flakes for the `nix build` call below.
FROM nixos/nix:2.24.10 AS builder
ENV NIX_CONFIG="experimental-features = nix-command flakes"

WORKDIR /src
COPY . .

# Build the two binaries into /src/result-* via the flake outputs.
# `.#juniusd-headless-static` = juniusd without the embedded SPA.
# `.#junius-static` = the trimmed in-container CLI (--no-default-features).
RUN nix build .#juniusd-headless-static -o /out/juniusd-link \
 && nix build .#junius-static          -o /out/junius-link  \
 && cp -L /out/juniusd-link/bin/juniusd /out/juniusd \
 && cp -L /out/junius-link/bin/junius   /out/junius

# Scratch runtime: only the two binaries + ENV. No shell, no libc — the
# binaries are static. Operators mount their `platform.toml` at
# `/etc/junius/`; migrations run via `docker run … junius migrate up`.
FROM scratch AS runtime
COPY --from=builder /out/juniusd /usr/local/bin/juniusd
COPY --from=builder /out/junius  /usr/local/bin/junius
ENV JUNIUS_MODE=precompiled \
    JUNIUS_CONFIG=/etc/junius/platform.toml
VOLUME ["/etc/junius"]
EXPOSE 18080
# HEALTHCHECK omitted — `scratch` has no shell or `curl`/`wget`. The
# orchestrator's healthcheck (compose / k8s readinessProbe) probes
# the auth-free `/healthz` endpoint juniusd exposes.
ENTRYPOINT ["/usr/local/bin/juniusd"]

LABEL org.opencontainers.image.title="junius-backend" \
      org.opencontainers.image.description="Headless Junius backend (juniusd, no embedded SPA, all monorepo plugins). Pairs with the M24 `frontend` SSR image." \
      org.opencontainers.image.source="https://github.com/${GITHUB_REPOSITORY:-junius/junius}" \
      junius.variant="backend"
