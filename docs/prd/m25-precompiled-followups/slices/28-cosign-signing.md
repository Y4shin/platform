# Slice #28 — cosign keyless image signing

**PRD:** ../prd.md · **kind:** capability · **mode:** hitl

Full design: [`27-M25-precompiled-followups.md` §2](../../../impl/27-M25-precompiled-followups.md#2-cosign-keyless-signing).

> Land **only once** the repo's `ghcr.io` OIDC trust is confirmed working end-to-end.

## What to build

Sign the M24-published images with cosign keyless (OIDC) and verify in CI.

- `permissions: id-token: write` on the M24 GHA workflow.
- A `cosign sign` step against the just-pushed digest.
- A `cosign verify` step in the smoke job.
- A note in the precompiled example's README on verifying a pulled tag against the keyless
  signature.

**First consumer:** the M24 smoke job's `cosign verify` step against a just-published image.

## Acceptance criteria

- [ ] Published images are signed; `cosign verify` passes in the smoke job.
- [ ] The example README documents how a deployer verifies a pulled tag.

## Blocked by

- None — independent of the other M25 items (but gated on confirmed ghcr OIDC trust).
