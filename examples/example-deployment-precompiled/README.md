# Example deployment — precompiled images (M24)

Two example topologies, both pulling prebuilt images from `ghcr.io`
instead of running `junius build` against a local toolchain:

|                        | `full/`                                       | `split/`                                                       |
|------------------------|-----------------------------------------------|----------------------------------------------------------------|
| Containers             | 1 (`juniusd`)                                 | 2 (`juniusd` + `frontend`)                                     |
| Image(s)               | `junius-full:<tag>`                           | `junius-backend:<tag>` + `junius-frontend:<tag>`               |
| FE delivery            | Embedded SPA via `rust-embed`                 | Node SSR + hydration                                           |
| Browser sees           | `juniusd` directly                            | `frontend` only (proxies `/api`/`/h`/`/rpc` to `juniusd`)     |
| `[build].mode`         | `precompiled`                                 | `precompiled`                                                  |
| `[build].frontend`     | (default `embedded`)                          | `none`                                                         |
| Best for               | Single-binary self-contained deployments      | SSR for first paint, CDN-fronted FE, independent FE/BE scaling |

Picking between **precompiled** and **source-built** is a separate
choice — see the matrix in [`../example-deployment-split/README.md`](../example-deployment-split/README.md)
and the ops guide.

## How the bundle/config check works

Both images set `JUNIUS_MODE=precompiled` in their entrypoint. At boot,
`juniusd` checks that the `[plugins].enabled` list in your
`platform.toml` **exactly matches** the plugin set linked into the
binary. Any mismatch (missing or extra) fails fast with a message
naming the difference. There are two escape hatches:

- Add the missing plugins to `[plugins].enabled` (the usual fix when
  the monorepo gains a new plugin and your image bumped to a tag that
  includes it).
- Switch to the source-build path
  ([../example-deployment/](../example-deployment/) or
  [../example-deployment-split/](../example-deployment-split/)) if you
  want a different subset.

## Tags vs SHA pins

The CI workflow publishes two tags per variant on every push to `main`:

- `<image>:latest` — follows whatever shipped from the latest `main`.
  Convenient for dev/CI; **don't pin in production**.
- `<image>:<short-sha>` — immutable. Pin this in production
  `docker-compose.yml` / k8s manifests; bump deliberately.

Both examples default to `:latest` for the demo but accept a
`JUNIUS_TAG` env var to override:

```sh
export JUNIUS_TAG=abc1234
docker compose up
```

## Run

```sh
# Both examples need these three secrets.
export OIDC_CLIENT_SECRET=… SESSION_KEY=… ROLE_PW_SECRET=…

cd full   # or `cd split`
docker compose up
```

The example compose files **don't** include an IdP — bring your own
(Authentik, Keycloak, etc.). Copy the Authentik stanza from
`dev/docker-compose.yml` if you want a turnkey local IdP.

## Migrations

`juniusd` doesn't auto-migrate on boot. Run once before first start
(and on every release that ships new migrations):

```sh
docker compose run --rm juniusd junius migrate up
```

Inside the image, the `junius` CLI carries only the runtime-relevant
commands (`check`, `migrate`, `plugin {list,info}`, `i18n`,
`provision`). Source-build commands (`build`, `cache`, `plugin
{enable,disable}`, `sync`, `dev`, `new`, `rpc`) are compiled out —
they have nothing to do inside a precompiled image.

## What's deferred (M25)

- **cosign verification.** The CI workflow currently publishes
  unsigned images. cosign keyless signing + a verification step on
  pull will land in M25 once the repo's OIDC trust is set up.
- **Multi-arch.** The first M24 batch ships `linux/amd64` only. Arm64
  joins once amd64 is proven.
- **`junius doctor`.** A small CLI subcommand that diffs your
  `platform.toml`'s `enabled` list against an image's bundled
  plugins — so you can preview compatibility before pulling. Pairs
  with `:latest` upgrade flows.

## Healthcheck wiring

Both `juniusd` and the SSR `frontend` server expose `/healthz`
(auth-free, returns `{"status":"ok"}`). The Dockerfiles intentionally
**omit** the in-image `HEALTHCHECK` directive because the runtime
bases are distroless / node-slim and don't ship `curl`/`wget`. Use the
orchestrator's healthcheck instead:

- docker-compose: `healthcheck:` block as shown in the example
  `docker-compose.yml` (`wget` runs on the host).
- Kubernetes: `readinessProbe.httpGet` + `livenessProbe.httpGet` on
  `/healthz`.

## Image surface

Every published image carries OCI labels for `docker inspect`:

- `org.opencontainers.image.title` — `junius-{full,backend,frontend}`.
- `org.opencontainers.image.description` — what the variant is for.
- `org.opencontainers.image.source` — the repo URL the image was
  built from.
- `junius.variant` — `full` / `backend` / `frontend`.
