---
kind: epic
title: Build & infra
slug: build-and-infra
epic_issue: 33
prds:
  - slug: m22-rustfs-evaluation
    kind: capability
    issue: 21
    blocked_by: []
  - slug: m25-precompiled-followups
    kind: capability
    issue: 26
    blocked_by: []
status: in-progress
---

# Build & infra

> Retrofitted epic grouping milestones **M22** + **M25**. Each child PRD links to its own
> milestone doc under [`docs/impl/`](../../../impl/) for the full design. Both are capabilities
> (no UI) that harden the build/deploy/storage substrate; they're independent of each other and
> of all plugin/SDK code.

## Problem / outcome

The plugin/SDK layers are mature, but the substrate underneath them carries loose ends:

- The dev/CI S3 server is **MinIO** — AGPL-trajectory, ~150–200 MB, non-Rust-native — used
  only for a narrow, commodity slice of S3.
- The M24 precompiled images shipped, but with deferred polish: no pre-pull preflight, unsigned
  images, amd64-only, and no test that catches drift between source-built and image-built
  behaviour.

**Outcome:** a substrate that's lighter, signed, multi-arch, self-checking, and free of an
avoidable AGPL dep — without touching the plugin or SDK abstractions above it.

## Constituent plugins & surfaces

- **Storage server** (dev stack + E2E orchestrator + deployment example) — the only thing M22
  touches; the `ObjectStore` seam at `platform/src/storage/` stays unchanged.
- **`tools/junius`** — a new `doctor` command (M25).
- **GHA release workflow** — cosign signing + linux/arm64 multi-arch (M25).
- **CI** — a nightly precompiled-image E2E (M25).

## Shared / foundational work

None shared between the two PRDs — each is self-contained. They sit together because they're
the same *kind* of work (infra hardening) and share no consumers above the substrate.

## Per-plugin features

Not applicable — both are capabilities. Acceptance is a consumer/infra test, not a UI:

- **M22 — Evaluate RustFS** (`m22-rustfs-evaluation`): a probe spike with two acceptable
  outcomes — **adopt** (swap MinIO out of dev/CI/example) or **not yet** (a written record of
  the specific gap). Proven by M10's existing `ObjectStore` integration tests.
- **M25 — Precompiled-image follow-ups** (`m25-precompiled-followups`): `junius doctor`
  preflight, cosign keyless signing, linux/arm64 multi-arch, and a nightly image E2E — each an
  independent slice.

## Dependency ordering

No cross-PRD dependencies. Within M22, the probe slice gates the adopt-only swap slices (if the
spike concludes "not yet", those close without work). M25's four items are mutually independent.

## Out of scope

Changing the `ObjectStore` abstraction or any plugin storage usage (M22); the original M24
image delivery itself (M25 is only its deferred polish).

## Open questions

The single open question — does RustFS clear our narrow S3 bar? — is the M22 spike's deliverable
and is resolved within that PRD, not here.

## Decomposition

1. **`m22-rustfs-evaluation`** (capability, #21) — spike + conditional swap of the dev/CI/example
   storage server.
2. **`m25-precompiled-followups`** (capability, #26) — doctor, signing, multi-arch, nightly
   image E2E.
