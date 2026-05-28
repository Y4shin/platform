# syntax=docker/dockerfile:1.7
#
# M24 `full` image: single-container deployment. `juniusd` with
# `--features embed-frontend` (the SPA baked in via `rust-embed`), every
# monorepo plugin linked in. The image entrypoint sets
# `JUNIUS_MODE=precompiled`, so the boot check refuses to start unless
# `[plugins].enabled` exactly matches the bundled set.
#
# Build strategy: pure `nix build`. The flake's `juniusd-static` output
# depends on `frontend-bundle` (a `pnpm.fetchDeps` + `pnpm shell build`
# derivation) and stages the FE dist into the cargo source tree before
# the rust musl-static link runs. Result: a fully static binary on
# `scratch` — no shell, no libc, no toolchain in the runtime image.
#
# Operators run migrations via:
#   docker run --rm <image> junius migrate up --config /etc/junius/platform.toml
# then start the long-running container.

FROM nixos/nix:2.24.10 AS builder
ENV NIX_CONFIG="experimental-features = nix-command flakes"

WORKDIR /src
COPY . .

RUN nix build .#juniusd-static -o /out/juniusd-link \
 && nix build .#junius-static  -o /out/junius-link  \
 && cp -L /out/juniusd-link/bin/juniusd /out/juniusd \
 && cp -L /out/junius-link/bin/junius   /out/junius

FROM scratch AS runtime
COPY --from=builder /out/juniusd /usr/local/bin/juniusd
COPY --from=builder /out/junius  /usr/local/bin/junius
ENV JUNIUS_MODE=precompiled \
    JUNIUS_CONFIG=/etc/junius/platform.toml
VOLUME ["/etc/junius"]
EXPOSE 18080
# HEALTHCHECK omitted — `scratch` has no shell or `curl`/`wget`. The
# orchestrator's probe (compose `healthcheck:` / k8s readinessProbe)
# hits the auth-free `/healthz` endpoint juniusd exposes.
ENTRYPOINT ["/usr/local/bin/juniusd"]

LABEL org.opencontainers.image.title="junius-full" \
      org.opencontainers.image.description="Single-container Junius deployment (juniusd + embedded SPA + all monorepo plugins)" \
      org.opencontainers.image.source="https://github.com/${GITHUB_REPOSITORY:-junius/junius}" \
      junius.variant="full"
