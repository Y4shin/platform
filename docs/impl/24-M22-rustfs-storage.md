# 24. M22 — Evaluate RustFS as a MinIO alternative

> **Status:** 🚧 planned. Independent of M14–M21 — can land any time after
> [M10](11-M10-infra-capabilities.md) (which established the
> `rust-s3` / `ObjectStore` storage seam). Touches no plugin code: the
> evaluation is contained to the storage *server* (dev stack + E2E +
> deployment example), not the SDK abstraction.

One-line goal: end the milestone either with **`rustfs` swapped in
everywhere MinIO is used today** (dev stack, E2E orchestrator,
`examples/example-deployment`, ops guide) or with **a written-up
"not yet" record** that documents the specific compatibility gap or
risk that ruled it out, so the question doesn't keep getting re-raised
ad hoc.

## Why this milestone exists

The M10 storage capability picked **MinIO** as the dev/CI S3-compatible
server. It has paid for itself: known-stable, every plugin author and
every CI run has touched it, the existing `rust-s3` ObjectStore tests
green against it. But the trade-offs have moved against it since:

- **License pressure.** MinIO's AGPL community edition has been
  progressively narrowed (browser UI removed, console split out,
  features moved behind enterprise). The S3 surface we use isn't
  affected today, but the trajectory is clear and means the dev/CI
  stack carries an AGPL dependency for a feature where the alternatives
  are commodity.
- **Image size + boot time.** `minio/minio:latest` is ~150–200 MB
  pulled and adds ~1–2 s to every cold `task test:e2e` and CI E2E
  job. Net out across CI runs over a year that's measurable.
- **Aesthetic alignment.** The project is otherwise Rust-native — host
  binary, SDK, CLI, generated codegen, even the i18n catalogs are
  consumed via Lingui's Rust(-friendly) toolchain. A Rust-native
  S3 server keeps the stack's deps consistent.

[RustFS](https://github.com/rustfs/rustfs) is the candidate: an
S3-compatible object store written in Rust. It's much newer than MinIO
(0.x at time of writing), so the question is whether it's ready for
*our specific, narrow* use of S3 — bucket create, presigned PUT/GET, a
handful of object metadata operations, all driven by `rust-s3` 0.36
through the `ObjectStore` trait at
[`platform/src/storage/`](../../platform/src/storage/).

## Outcome / acceptance

Two acceptable outcomes; commit to one based on the spike's findings:

**A. Adopt.** RustFS replaces MinIO in:
- [`dev/docker-compose.yml`](../../dev/docker-compose.yml) (the dev
  stack — `task dev` and `task infra:up`).
- [`e2e/run-suite.ts`](../../e2e/run-suite.ts) and
  [`packages/e2e/src/env.ts`](../../packages/e2e/src/env.ts) (the
  testcontainers stack the E2E suite spins).
- [`examples/example-deployment/platform.toml`](../../examples/example-deployment/platform.toml)
  (the worked deployment example).
- [`docs/impl/11-M10-infra-capabilities.md`](11-M10-infra-capabilities.md)
  (the "MinIO in dev; AWS/R2/etc. in prod" sentence).

  Verified by:
  - Every M10 ObjectStore integration test green against RustFS.
  - `task test:e2e` cold-starts green with RustFS instead of MinIO.
  - The events plugin's storage path (M10 stage E: object upload +
    presigned download) round-trips bit-identically.
  - `junius check` and the platform smoke (`task ci`) green.

**B. Not yet.** A short "investigation" entry under this doc records
the specific gap that ruled RustFS out (e.g. `rust-s3` 0.36 sends a
header RustFS doesn't yet implement, the docker image isn't on a
public registry stable enough for CI, a presigned-URL signature edge
case, etc.). MinIO stays. The milestone is **closed**, not deferred
indefinitely — re-evaluation needs a fresh proposal, not an unfinished
M22 task.

Either way, the spike output is its own value (we'll know exactly
what RustFS does and doesn't support for our surface) and removes the
ambient "should we switch?" question.

## Design

### Stage 1 — Spike: does our existing surface work against RustFS?

A **non-merging** branch + one explicit-output report. No code in the
main repo changes yet.

The spike answers:

1. **License + redistributability.** What's RustFS's license? (Verify;
   don't assume Apache-2.0 from the README — read `LICENSE`.) Are
   there any patent or trademark constraints? Is there an official
   container image on Docker Hub / ghcr that's safe to pin in CI?
2. **API surface coverage.** For each operation we actually issue
   (collated from `platform/src/storage/` + the events plugin's
   storage path):
   - `CreateBucket` (testcontainers post-start step).
   - `PutObject` with `Content-Type` + optional metadata.
   - `GetObject` (range requests? we don't use them today, confirm).
   - `HeadObject` (the `record_object` repo path uses this).
   - **Presigned PUT** (v4 signature; `rust-s3` issues these; the
     juniusd-mediated upload path in M10 stage D depends on it).
   - **Presigned GET** (download URLs handed to the browser).
   - `DeleteObject` (cleanup path).

   For each, write a one-shot probe (a small Rust binary or a `cargo
   test --ignored` in `platform/src/storage/`) that exercises the call
   against a `testcontainers`-managed RustFS and asserts success +
   correct response shape. **The probe is the deliverable** — it stays
   in the repo as `cargo test -p platform --test rustfs_smoke
   -- --ignored` and re-runs cheaply on later RustFS upgrades.

3. **`rust-s3` compatibility quirks.** rust-s3 has historically been
   strict about response shapes; MinIO and AWS S3 differ in
   `Content-MD5` handling, error-XML envelope, region quirks. Note
   anything that needs a per-server flag.

4. **Image + boot characteristics.** Measure: pulled image size,
   container start time, idle RSS. These go into the adopt/not-yet
   write-up.

**Output of Stage 1**: a status update on this doc (under a new
"Spike findings" section), plus the probe test landed regardless of
the decision.

### Stage 2 (only if Stage 1 says "adopt") — Swap dev + E2E + example

One commit per surface, mirroring the M17 stage pattern:

- **2a — dev stack.** Update [`dev/docker-compose.yml`](../../dev/docker-compose.yml)
  to replace the `minio` service with `rustfs` (same volume layout,
  same `MINIO_ROOT_USER`/`PASSWORD` env vars if rustfs accepts them,
  or the rustfs-native equivalents — to be confirmed in the spike).
  Bump `task infra:up` and `task dev` smoke; verify the events plugin's
  storage demo works in a real browser session against the dev stack.

- **2b — E2E orchestrator.** Update
  [`packages/e2e/src/env.ts`](../../packages/e2e/src/env.ts)'s
  `startStack()` to spin a rustfs container instead of minio. Same
  testcontainers shape (random-port mapping, label-tagged for the
  M17 sweep). The rendered host config at
  [`e2e/host-config.toml.tmpl`](../../e2e/host-config.toml.tmpl)'s
  `[config.storage.buckets.main]` section needs whatever endpoint
  shape rustfs requires (probably identical; verify in the spike).

- **2c — example deployment + ops guide.** Update
  [`examples/example-deployment/platform.toml`](../../examples/example-deployment/platform.toml)
  to point at rustfs (the example assumes the dev stack is up, so
  same swap). Update
  [`docs/impl/11-M10-infra-capabilities.md`](11-M10-infra-capabilities.md)'s
  "MinIO in dev" copy. Add a section to the plugin authoring guide if
  the storage chapter (M10) needs to mention the swap.

### Stage 3 — Documentation + decision-log entry

- [`docs/design/14-decision-log.md`](../design/14-decision-log.md):
  one entry, dated, summarising the swap (why, with the spike data
  inline) — same shape as the M10 decision-log entry that picked
  rust-s3 over aws-sdk-s3.
- [`docs/impl/README.md`](README.md): M22 row → ✅ (or, if outcome B,
  ✅ with a note that the verdict was "not yet").
- [`docs/impl/11-M10-infra-capabilities.md`](11-M10-infra-capabilities.md):
  one-line "updated by M22" footnote next to the storage row, matching
  the M14/M17 pattern that M12's CI-jobs section uses.

## Risks & mitigations

- **RustFS is a 0.x.** S3 surface gaps, signature edge cases, or
  unflagged feature mismatches are real. Mitigation: the probe test
  from Stage 1 is the truth source; Stage 2 doesn't start until
  Stage 1 is green.
- **CI image availability.** If the only RustFS image is a self-hosted
  registry, CI gets brittle. Mitigation: Stage 1 confirms a Docker
  Hub / ghcr image is available + actively maintained. Without that,
  outcome flips to B regardless of API compatibility.
- **Deployments that already point at S3/R2/AWS-S3 stay untouched.**
  This milestone changes the dev/CI/example *defaults*; a real
  deployment's `[config.storage.buckets.<name>].endpoint` is its own
  choice.
- **The rust-s3 + RustFS combination is doubly new.** Both are
  smaller communities than aws-sdk-s3 + MinIO. Mitigation: keep the
  ObjectStore trait seam so a future swap back (or to a third option)
  is the same shape of work; the seam is exactly what M10 set up for
  this reason.

## Out of scope

- Replacing `rust-s3` itself. The ObjectStore trait abstracts the SDK;
  M22 changes the server side only.
- Multi-region / distributed RustFS setups. The dev/CI/example
  topology is a single bucket on a single node; production deployments
  pick their own.
- A migration tool for existing MinIO data. M22 ships fresh
  dev/CI/example stacks; there is no production state to migrate
  (each deployment points at its own configured S3 endpoint).
- Performance benchmarking past "boot + first round-trip is at least
  as fast as MinIO." Real throughput tuning is a later concern only if
  it actually bites.

## Downstream doc updates (if outcome A)

- [`docs/impl/README.md`](README.md) — M22 row flipped.
- [`docs/impl/11-M10-infra-capabilities.md`](11-M10-infra-capabilities.md)
  — "MinIO in dev" copy + the library-defaults table.
- [`docs/design/14-decision-log.md`](../design/14-decision-log.md) —
  M22 entry summarising the swap with the spike numbers.
- [`docs/plugin-authoring-guide.md`](../plugin-authoring-guide.md) —
  the §11 storage chapter mentions MinIO in passing; update to rustfs
  if the swap lands.
