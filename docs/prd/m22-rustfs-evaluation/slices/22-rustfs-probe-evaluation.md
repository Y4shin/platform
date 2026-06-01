---
kind: capability
title: "RustFS probe + license/image evaluation"
slug: rustfs-probe-evaluation
issue: 22
prd: ../prd.md
mode: hitl
---

# Slice #22 — RustFS probe + license/image evaluation

Full design: [`24-M22-rustfs-storage.md` §Stage 1](../../../impl/24-M22-rustfs-storage.md#stage-1--spike-does-our-existing-surface-work-against-rustfs).

## What to build

A non-merging spike whose **deliverable is a landed probe test** plus a decision write-up.

- A `cargo test -p platform --test rustfs_smoke -- --ignored` probe that, against a
  `testcontainers`-managed RustFS, exercises every S3 operation our code issues and asserts
  success + correct response shape: `CreateBucket`, `PutObject` (+ `Content-Type`/metadata),
  `GetObject`, `HeadObject`, presigned PUT (v4 signature), presigned GET, `DeleteObject`.
- An evaluation covering: RustFS's actual `LICENSE` (read it, don't assume) + any
  patent/trademark constraints; whether a stable public Docker Hub / ghcr image exists to pin
  in CI; `rust-s3` 0.36 compatibility quirks (Content-MD5, error-XML envelope, region) that
  need a per-server flag; and image size / boot time / idle RSS vs MinIO.
- A **decision**: adopt or not-yet, written into the milestone doc's "Spike findings" section.

**First consumer:** the existing M10 `ObjectStore` integration tests in
`platform/src/storage/` — the probe is a sibling exercising the same call surface.

## Acceptance criteria

- [ ] `cargo test -p platform --test rustfs_smoke -- --ignored` runs every listed S3 op
      against a testcontainers RustFS and asserts the response shape.
- [ ] The probe lands in the repo regardless of the verdict (re-runnable on upgrades).
- [ ] The write-up records license, image availability, `rust-s3` quirks, and boot/size
      numbers, and states a clear **adopt / not-yet** verdict.

## Blocked by

- None — can start immediately.
