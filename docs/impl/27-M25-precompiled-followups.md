# 27. M25 — Precompiled-image follow-ups

> **Status:** 🚧 planned. Sweeper milestone for items deferred from
> [M24](26-M24-precompiled-containers.md) Stage 7. Small, independent
> changes; each can land separately as a focused commit if priorities
> shift.

## Scope

Four items, in roughly the order they're useful:

### 1. `junius doctor` (bundle/config diff CLI)

A small new subcommand that, given a `platform.toml` and an image tag
(or `:latest`), reports the diff between the deployment's
`[plugins].enabled` and the image's bundled set **before** the
multi-hundred-MB pull. Lives behind the `develop` and `source-build`
gates (it consults `plugins/*/plugin.toml` in a checkout) **or** reads
the `junius.bundled_plugins` OCI label from a `docker pull --quiet`
when source isn't local.

- Files: new `tools/junius/src/commands/doctor.rs`; `Doctor` variant
  on `Command` (no feature gate — useful in both build modes).
- Verification: a deployment with `enabled = ["events"]` against a
  monorepo bundling `["events", "admin"]` reports "extra in bundle:
  admin" and exits non-zero.

### 2. cosign keyless signing

The M24 GHA workflow currently publishes unsigned images. Adding
cosign signing needs:

- `permissions: id-token: write` on the workflow.
- A `cosign sign` step against the just-pushed digest.
- A verification step in the smoke job (`cosign verify`).
- A note in the precompiled example's README on how to verify a
  pulled tag against the keyless signature.

Worth doing once the repo's OIDC trust is confirmed working end-to-end
on `ghcr.io`.

### 3. linux/arm64 multi-arch

M24 ships `linux/amd64` only. Add arm64 once amd64 is proven stable:

- `platforms: linux/amd64,linux/arm64` on the `docker/build-push-action`
  step.
- Confirm the rust-builder stage cross-compiles cleanly (it should —
  no native deps in the workspace today).
- Doubles CI minutes per push; the trade-off is fine once Apple
  Silicon + cloud arm64 hosts are a real fraction of deployers.

### 4. Nightly precompiled-image E2E

The existing E2E suites (`e2e/` and `e2e/split/`) run against
source-built artifacts. Add a nightly variant that runs the same
specs against the just-published precompiled images. Catches drift
between source-built and image-built behaviour early (the only
intentional difference is bundle-all vs the deployment's `enabled`
list — anything else is a bug).

- `.github/workflows/images-nightly.yml`: schedule = `'0 3 * * *'`.
  Pulls `<variant>:latest`, runs `task ci:e2e` (and `ci:e2e:split`)
  against them with `JUNIUS_E2E_USE_IMAGE=1`.
- Requires a small change to the existing E2E orchestrators to skip
  the `cargo build` / `pnpm build` steps when the image is supplied.

## Out of scope

- Image vulnerability scanning (Trivy/Grype) — file a separate
  follow-up if needed; not bundled here.
- Helm charts / k8s manifests — compose examples only.
- Semver-tagged releases — needs a versioning policy first.

## Why a separate milestone

Each item is roughly a single-commit change with no cross-dependencies
on the others. Bundling them into M24 would have inflated review
surface; landing them as a sweeper milestone keeps the M24 PR focused
on the deployment-path delivery.
