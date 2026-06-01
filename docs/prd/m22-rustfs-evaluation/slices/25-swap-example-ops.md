---
kind: capability
title: "Swap example deployment + ops copy"
slug: swap-example-ops
issue: 25
prd: ../prd.md
mode: afk
---

# Slice #25 — Swap example deployment + ops copy

Full design: [`24-M22-rustfs-storage.md` §Stage 2c](../../../impl/24-M22-rustfs-storage.md#stage-2-only-if-stage-1-says-adopt--swap-dev--e2e--example).

> **Adopt-only.** Proceed only if Slice #22 concludes "adopt"; otherwise this slice closes
> without work.

## What to build

Point `examples/example-deployment/platform.toml` at RustFS (the example assumes the dev
stack is up — same swap), and update the "MinIO in dev" copy in
`docs/impl/11-M10-infra-capabilities.md` (+ the storage chapter of the plugin authoring guide
if it mentions MinIO).

**First consumer:** the worked example deployment booting against the swapped dev stack.

## Acceptance criteria

- [ ] The example deployment boots and round-trips storage against RustFS.
- [ ] The M10 doc + library-defaults copy no longer say "MinIO in dev" where RustFS now
      applies.
- [ ] `junius check` + `task ci` stay green.

## Blocked by

- #22 — needs the "adopt" verdict.
