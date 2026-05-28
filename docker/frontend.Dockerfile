# syntax=docker/dockerfile:1.7
#
# M24 `frontend` image: the M23 SSR Node server bundled with every
# monorepo plugin's frontend. Browser routes are SSR'd; `/api`, `/h`,
# `/rpc` are proxied to the `backend` image at `JUNIUS_BE_INTERNAL_URL`.
# No `junius` CLI inside (no DB connection, no `platform.toml` to
# consume).
#
# Build strategy: pure `nix build`. The flake's `frontend-ssr-bundle`
# output runs `pnpm install` against the hermetic `pnpm.fetchDeps`
# cache, then `pnpm --filter @junius/shell-ssr build`, then bundles
# the entire pnpm workspace tree into the output (sources +
# node_modules) so the Node runtime can `tsx` server.ts and resolve
# the workspace's `@junius/*` cross-package imports through the
# `.pnpm/node_modules/@junius/*` symlink graph.

FROM nixos/nix:2.24.10 AS builder
ENV NIX_CONFIG="experimental-features = nix-command flakes"

WORKDIR /src
COPY . .

RUN nix build .#frontend-ssr-bundle -o /out/bundle-link \
 && mkdir -p /out/bundle \
 && cp -RL /out/bundle-link/. /out/bundle/

# Runtime: node:22-slim. Distroless Node would shave a few hundred MB
# but doesn't ship the loader hooks `tsx` relies on; the slim variant
# is the M23 baseline.
FROM node:22-bookworm-slim AS runtime
ENV NODE_ENV=production \
    PORT=3000
COPY --from=builder /out/bundle /app
WORKDIR /app/platform/frontend-ssr
EXPOSE 3000
# Healthcheck via orchestrator against `/healthz` (auth-free, returns
# 200 without touching the BE so an SSR<>BE network partition doesn't
# mark the FE unhealthy).
ENTRYPOINT ["node", "--enable-source-maps", "--import", "tsx", "src/server.ts"]

LABEL org.opencontainers.image.title="junius-frontend" \
      org.opencontainers.image.description="SSR Node server for Junius (M23 split topology, all monorepo plugin frontends bundled)" \
      org.opencontainers.image.source="https://github.com/${GITHUB_REPOSITORY:-junius/junius}" \
      junius.variant="frontend"
