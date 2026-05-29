# syntax=docker/dockerfile:1.7
#
# M24 `frontend` image: the M23 SSR Node server bundled with every
# monorepo plugin's frontend. Browser routes are SSR'd; `/api`, `/h`,
# `/rpc` are proxied to the `backend` image at `JUNIUS_BE_INTERNAL_URL`.
# No `junius` CLI inside (no DB connection, no `platform.toml` to
# consume).
#
# Build strategy: the FE bundle is produced by `nix build
# .#frontend-ssr-bundle` against the flake (`pnpm.fetchDeps` + Vite SSR
# build with `noExternal: true` in `vite.node.config.ts`, so
# `dist/node-server/server.js` is a single self-contained file). The
# Dockerfile copies that bundle into a `node:22-slim` runtime — no nix
# sandbox inside docker buildkit, no `node_modules`, no source files.
#
# Build wrapper (run before `docker build`): see `tools/build-images.sh`.
# It dereferences the `nix build .#frontend-ssr-bundle` output into
# `docker/staging/frontend-dist/` (docker COPY doesn't follow symlinks
# pointing outside the build context).

ARG BUNDLE_PATH=docker/staging/frontend-dist

FROM node:22-bookworm-slim AS runtime
ARG BUNDLE_PATH
ENV NODE_ENV=production \
    PORT=3000
COPY ${BUNDLE_PATH} /app/dist
WORKDIR /app
EXPOSE 3000
# Healthcheck via orchestrator against `/healthz` (auth-free, returns
# 200 without touching the BE so an SSR<>BE network partition doesn't
# mark the FE unhealthy).
ENTRYPOINT ["node", "--enable-source-maps", "dist/node-server/server.js"]

LABEL org.opencontainers.image.title="junius-frontend" \
      org.opencontainers.image.description="SSR Node server for Junius (M23 split topology, all monorepo plugin frontends bundled into a single Node entry)" \
      org.opencontainers.image.source="https://github.com/${GITHUB_REPOSITORY:-junius/junius}" \
      junius.variant="frontend"
