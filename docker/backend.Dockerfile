# syntax=docker/dockerfile:1.7
#
# M24 `backend` image: headless juniusd built without `embed-frontend`,
# every monorepo plugin linked in. Pairs with the M24 `frontend` image
# (or any third-party static-asset host). Browser routes return a
# structured 404 (`FRONTEND_NOT_EMBEDDED`); the FE container handles them.
#
# Build strategy (M24, post-nix-pivot): the Rust binaries are produced
# by `nix build .#juniusd-headless-static` and `nix build .#junius-static`
# (against the flake derivations in `flake.nix`) — fully static, musl-
# linked, position-independent. The Dockerfile does no compilation; it
# stages the pre-built binaries into a `FROM scratch` runtime image.
# This split keeps the nix sandbox out of docker buildkit (where it
# routinely OOMs the container or fills the overlay filesystem).
#
# Build wrapper (run before `docker build`): see `tools/build-images.sh`.
# That script calls `nix build` for each derivation, dereferences the
# resulting nix-store symlinks into `docker/staging/` (docker COPY
# doesn't follow symlinks pointing outside the build context), then
# invokes `docker build` against those staged files.
#
# Default ARG values target `docker/staging/`; override only if you've
# staged elsewhere.

ARG JUNIUSD_PATH=docker/staging/juniusd-headless
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
# orchestrator's healthcheck (compose / k8s readinessProbe) probes
# the auth-free `/healthz` endpoint juniusd exposes.
ENTRYPOINT ["/usr/local/bin/juniusd"]

LABEL org.opencontainers.image.title="junius-backend" \
      org.opencontainers.image.description="Headless Junius backend (juniusd, no embedded SPA, all monorepo plugins). Pairs with the M24 `frontend` SSR image." \
      org.opencontainers.image.source="https://github.com/${GITHUB_REPOSITORY:-junius/junius}" \
      junius.variant="backend"
