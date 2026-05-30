---
kind: capability
title: Precompiled-image follow-ups
slug: m25-precompiled-followups
milestone: M25
prd_issue: 26
slices: [27, 28, 29, 30]
status: issues-created
---

# Precompiled-image follow-ups

> Migrated from the M25 milestone doc
> [`docs/impl/27-M25-precompiled-followups.md`](../../impl/27-M25-precompiled-followups.md),
> which holds the **full design** (per-item file lists + verification). This PRD is the
> planning surface; it links back rather than duplicating. A **sweeper** for items deferred
> from [M24](../../impl/26-M24-precompiled-containers.md) Stage 7 — each item is roughly a
> single, independent change with no cross-dependencies on the others.

## Problem / why

M24 shipped three official `ghcr.io` images (`full` / `frontend` / `backend`) but deferred a
handful of independent polish items to keep the M24 PR focused on the deployment-path
delivery. Bundling them into M24 would have inflated review surface; landing them as a sweeper
keeps each focused. The four items, in roughly the order they're useful:

1. **`junius doctor`** — a way to diff a deployment's `[plugins].enabled` against an image's
   bundled set **before** the multi-hundred-MB pull.
2. **cosign keyless signing** — M24's GHA workflow publishes unsigned images.
3. **linux/arm64 multi-arch** — M24 ships amd64 only; Apple Silicon + cloud arm64 hosts are a
   real fraction of deployers.
4. **Nightly precompiled-image E2E** — the suites run against source-built artifacts; nothing
   catches drift between source-built and image-built behaviour.

## API surface

Four independent surfaces (each is its own slice):

- **`junius doctor`** — a new `Doctor` `Command` variant + `tools/junius/src/commands/doctor.rs`.
  Given a `platform.toml` + an image tag (or `:latest`), reports the diff between the
  deployment's `[plugins].enabled` and the image's bundled set, exiting non-zero on mismatch.
  Resolves the bundled set from `plugins/*/plugin.toml` in a checkout (behind the `develop` /
  `source-build` gates) **or** from the `junius.bundled_plugins` OCI label via
  `docker pull --quiet` when source isn't local. **No feature gate** — useful in both build
  modes.
- **cosign signing** — `permissions: id-token: write` on the M24 workflow, a `cosign sign`
  step against the pushed digest, a `cosign verify` step in the smoke job, and a verify note in
  the precompiled example's README.
- **arm64 multi-arch** — `platforms: linux/amd64,linux/arm64` on the
  `docker/build-push-action` step (the rust-builder stage cross-compiles cleanly — no native
  deps today).
- **nightly E2E** — `.github/workflows/images-nightly.yml` (`schedule: '0 3 * * *'`) pulling
  `<variant>:latest` and running `task ci:e2e` (+ `ci:e2e:split`) with
  `JUNIUS_E2E_USE_IMAGE=1`; plus a small change to the E2E orchestrators to skip the
  `cargo build` / `pnpm build` steps when an image is supplied.

## First consumer

- **`junius doctor`** — a deployment fixture with `enabled = ["events"]` against a monorepo
  bundling `["events", "admin"]` (the verification case).
- **cosign** — the M24 smoke job's `cosign verify` step against a just-published image.
- **arm64** — an arm64 image pull + boot.
- **nightly E2E** — the existing `e2e/` and `e2e/split/` specs, run against the published
  images.

## Encapsulation & layering

`junius doctor` is host/CLI tooling (no plugin or SDK change); it reads manifests / OCI
labels, never plugin internals. The other three are CI/release-pipeline changes
(`.github/workflows/`, the precompiled example) — no application code. Deployments that pin
their own image/registry are unaffected.

## Compatibility / versioning

All four are **additive**. `junius doctor` is a new subcommand; cosign/arm64/nightly extend
the M24 pipeline without changing the existing amd64 unsigned path (arm64 is added alongside
amd64; signing is added on top). Each lands independently as a focused commit.

## Out of scope

- **Image vulnerability scanning** (Trivy/Grype) — a separate follow-up if needed.
- **Helm charts / k8s manifests** — compose examples only.
- **Semver-tagged releases** — needs a versioning policy first.

## Open questions

No design-level blockers. Two practical gates, surfaced in their slices: cosign should land
**only once** the repo's `ghcr.io` OIDC trust is confirmed working end-to-end; arm64 should
land **only after** amd64 is proven stable (it doubles CI minutes per push). These are
sequencing notes, not unresolved design questions.

## Implementation notes
<!-- appended by implement-issue as slices land; empty for now -->
