# syntax=docker/dockerfile:1.7
#
# M24 `frontend` image: the M23 SSR Node server bundled with every monorepo
# plugin's frontend. Browser routes are SSR'd; `/api`, `/h`, `/rpc` are
# proxied to the `backend` image at `JUNIUS_BE_INTERNAL_URL`. No `junius`
# CLI inside (no DB connection, no `platform.toml` to consume).

ARG NODE_VERSION=22-bookworm-slim

# ---------- Stage 1: node + pnpm builder -----------------------------------
FROM node:${NODE_VERSION} AS builder
ENV PNPM_HOME=/usr/local/pnpm \
    PATH=/usr/local/pnpm:$PATH
RUN corepack enable \
 && corepack prepare pnpm@11.1.1 --activate

WORKDIR /src
COPY . .

# Workspace install + dual-bundle SSR build (client + server).
RUN pnpm install --frozen-lockfile \
 && pnpm --filter @junius/shell-ssr build

# ---------- Stage 2: slim node runtime -------------------------------------
FROM node:${NODE_VERSION} AS runtime
ENV NODE_ENV=production \
    PORT=3000
# Production-mode SSR server: imports the prebuilt bundle from
# `dist/server/entry-server.js` and serves `dist/client/` for assets. Keep
# `tsx` available since `package.json::scripts.start` runs `server.ts`
# under it (~tiny dep cost; the bundle compile already happened above).
WORKDIR /app
# Bring only what runtime needs: the SSR pkg's dist + its dependencies
# (resolved via pnpm in the builder). A pnpm-deploy is cleaner but adds a
# stage; landing the simpler copy-everything path for v1 and shrinking
# later.
COPY --from=builder /src/platform/frontend-ssr /app/platform/frontend-ssr
COPY --from=builder /src/platform/frontend     /app/platform/frontend
COPY --from=builder /src/packages              /app/packages
COPY --from=builder /src/plugins               /app/plugins
COPY --from=builder /src/node_modules          /app/node_modules
COPY --from=builder /src/package.json          /app/package.json
COPY --from=builder /src/pnpm-workspace.yaml   /app/pnpm-workspace.yaml
COPY --from=builder /src/pnpm-lock.yaml        /app/pnpm-lock.yaml

WORKDIR /app/platform/frontend-ssr
EXPOSE 3000
# `JUNIUS_BE_INTERNAL_URL` is required; the SSR transport refuses to boot
# without it (entry-server.ts enforces). Set this in compose / k8s to the
# backend service's internal URL, e.g. `http://backend:18080`.
# Healthcheck via orchestrator against `/healthz` (auth-free, returns 200
# without touching the BE so an SSR<>BE partition doesn't mark the FE
# unhealthy).
ENTRYPOINT ["node", "--enable-source-maps", "--import", "tsx", "src/server.ts"]

LABEL org.opencontainers.image.title="junius-frontend" \
      org.opencontainers.image.description="SSR Node server for Junius (M23 split topology, all monorepo plugin frontends bundled)" \
      org.opencontainers.image.source="https://github.com/${GITHUB_REPOSITORY:-junius/junius}" \
      junius.variant="frontend"
