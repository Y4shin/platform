---
kind: capability
title: Evaluate RustFS as a MinIO alternative
slug: m22-rustfs-evaluation
milestone: M22
prd_issue: 21
slices: [22, 23, 24, 25]
status: issues-created
---

# Evaluate RustFS as a MinIO alternative

> Migrated from the M22 milestone doc
> [`docs/impl/24-M22-rustfs-storage.md`](../../impl/24-M22-rustfs-storage.md), which holds the
> **full design** (the probed S3 operations, the per-surface swap list, risks). This PRD is
> the planning surface; it links back rather than duplicating. Independent of M14–M21 — can
> land any time after M10 (which established the `rust-s3` / `ObjectStore` storage seam).
> Touches **no plugin or SDK code**: the evaluation is contained to the storage *server* (dev
> stack + E2E orchestrator + deployment example), not the abstraction.

## Problem / why

M10 picked **MinIO** as the dev/CI S3-compatible server and it has paid for itself. But the
trade-offs have shifted:

- **License pressure** — MinIO's AGPL community edition has been progressively narrowed; the
  S3 surface we use is unaffected today, but the trajectory means the dev/CI stack carries an
  AGPL dep for a commodity feature.
- **Image size + boot time** — `minio/minio:latest` is ~150–200 MB and adds ~1–2 s to every
  cold E2E run; measurable across a year of CI.
- **Aesthetic alignment** — the stack is otherwise Rust-native; a Rust-native S3 server keeps
  deps consistent.

[RustFS](https://github.com/rustfs/rustfs) is the candidate (an S3-compatible store in Rust),
but it's young (0.x). The question is whether it's ready for **our specific, narrow** use of
S3 — bucket create, presigned PUT/GET, a handful of object-metadata ops — all driven by
`rust-s3` 0.36 through the `ObjectStore` trait at `platform/src/storage/`. This is a **spike
with two acceptable outcomes**: **adopt** (swap MinIO out everywhere it's used in dev/CI/
example) or **not yet** (a written-up record of the specific gap that ruled it out, so the
question stops being re-raised ad hoc). Either way the milestone **closes** — re-evaluation
needs a fresh proposal, not an unfinished task.

## API surface

This capability is an evaluation of the **storage server** behind the existing `ObjectStore`
seam, not a new SDK API. The concrete surfaces:

- A landed probe test — `cargo test -p platform --test rustfs_smoke -- --ignored` — that
  exercises, against a `testcontainers`-managed RustFS, every S3 operation our code actually
  issues: `CreateBucket`, `PutObject` (+ `Content-Type`/metadata), `GetObject`, `HeadObject`,
  presigned PUT (v4), presigned GET, `DeleteObject`. **The probe is the deliverable** and
  stays in the repo to re-run on future RustFS upgrades.
- If adopted, the server swap touches `dev/docker-compose.yml`, the E2E testcontainers stack
  (`packages/e2e/src/env.ts`, `e2e/run-suite.ts`, `e2e/host-config.toml.tmpl`), and
  `examples/example-deployment/platform.toml`.

## First consumer

- The existing M10 `ObjectStore` integration tests in `platform/src/storage/` — the probe is
  a sibling test exercising the same call surface.
- The events plugin's storage path (M10 stage E: object upload + presigned download) — the
  end-to-end round-trip that must stay bit-identical if RustFS is adopted.

## Encapsulation & layering

The `ObjectStore` trait (M10) is the seam; this milestone changes the **server side only**.
No plugin code, no SDK change, no `rust-s3` replacement. Real deployments already point at
their own configured `[config.storage.buckets.<name>].endpoint` and are untouched — only the
dev/CI/example *defaults* move.

## Compatibility / versioning

- `rust-s3` 0.36 is historically strict about response shapes (Content-MD5, error-XML
  envelope, region quirks); the spike notes anything needing a per-server flag.
- RustFS is 0.x — gaps/signature edge cases are real, so Stage 2 (the swap) does **not** start
  until Stage 1 (the probe) is green. The `ObjectStore` seam keeps a future swap-back (or to a
  third option) the same shape of work.
- CI image availability is a gating risk: without a stable public Docker Hub / ghcr image, the
  outcome flips to "not yet" regardless of API compatibility.

## Out of scope

- Replacing `rust-s3` itself — the trait abstracts the SDK; M22 changes the server only.
- Multi-region / distributed RustFS — the dev/CI/example topology is single-bucket/single-node.
- A migration tool for existing MinIO data — fresh stacks only; no production state to migrate.
- Performance benchmarking past "boot + first round-trip ≥ as fast as MinIO."

## Open questions

Resolved at the design level (see the milestone doc): there are two acceptable outcomes
(adopt / not-yet), committed to based on the spike; the seam stays put either way. The open
empirical questions are exactly what Slice #1 answers (license, image availability, per-op
coverage, `rust-s3` quirks, boot characteristics). **The adopt-only slices (#2–#4) are
conditional on Slice #1 concluding "adopt"; if it concludes "not yet", they close without
work and the milestone finalizes on the probe + the write-up.**

## Implementation notes
<!-- appended by implement-issue as slices land; empty for now -->
