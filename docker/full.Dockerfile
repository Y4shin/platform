# syntax=docker/dockerfile:1.7
#
# M24 `full` image: single-container deployment. `juniusd` with
# `--features embed-frontend` (the SPA baked in via `rust-embed`), every
# monorepo plugin linked in. The image entrypoint sets
# `JUNIUS_MODE=precompiled`, so the boot check refuses to start unless
# `[plugins].enabled` exactly matches the bundled set.
#
# Build strategy: the binary is produced by `nix build .#juniusd-static`
# against the flake (depends on `frontend-bundle` for the SPA dist). The
# Dockerfile only stages the result into a `FROM scratch` runtime image
# — no nix sandbox inside docker buildkit (it routinely OOMs / fills
# the overlay filesystem).
#
# Build wrapper (run before `docker build`): see `tools/build-images.sh`.
# It calls `nix build` for each derivation, dereferences the resulting
# nix-store symlinks into `docker/staging/` (docker COPY doesn't follow
# symlinks pointing outside the build context), then invokes
# `docker build` against those staged files.

ARG JUNIUSD_PATH=docker/staging/juniusd-full
ARG JUNIUS_PATH=docker/staging/junius

FROM scratch AS runtime
ARG JUNIUSD_PATH
ARG JUNIUS_PATH
COPY --chmod=0755 ${JUNIUSD_PATH} /usr/local/bin/juniusd
COPY --chmod=0755 ${JUNIUS_PATH}  /usr/local/bin/junius
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
