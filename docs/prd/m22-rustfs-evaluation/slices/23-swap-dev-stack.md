# Slice #23 — Swap dev stack to RustFS

**PRD:** ../prd.md · **kind:** capability · **mode:** afk

Full design: [`24-M22-rustfs-storage.md` §Stage 2a](../../../impl/24-M22-rustfs-storage.md#stage-2-only-if-stage-1-says-adopt--swap-dev--e2e--example).

> **Adopt-only.** Proceed only if Slice #22 concludes "adopt"; otherwise this slice closes
> without work.

## What to build

Replace the `minio` service with `rustfs` in `dev/docker-compose.yml` (same volume layout;
the matching root-user/password env vars or RustFS-native equivalents confirmed by the
spike). Bump the `task infra:up` / `task dev` smoke.

**First consumer:** the events plugin's storage demo running in a real browser session against
the dev stack (M10 stage E upload + presigned download).

## Acceptance criteria

- [ ] `task dev` / `task infra:up` bring up RustFS in place of MinIO.
- [ ] The events plugin's storage path round-trips (upload + presigned download) against the
      dev stack in a browser session.

## Blocked by

- #22 — needs the "adopt" verdict + the confirmed env/endpoint shape from the probe.
