---
kind: capability
title: "`junius doctor` (bundle/config diff CLI)"
slug: junius-doctor
issue: 27
prd: ../prd.md
mode: afk
---

# Slice #27 — `junius doctor` (bundle/config diff CLI)

Full design: [`27-M25-precompiled-followups.md` §1](../../../impl/27-M25-precompiled-followups.md#1-junius-doctor-bundleconfig-diff-cli).

## What to build

A new `junius doctor` subcommand that diffs a deployment's `[plugins].enabled` against an
image's bundled plugin set **before** the multi-hundred-MB pull.

- A `Doctor` variant on `Command` + `tools/junius/src/commands/doctor.rs`. **No feature gate**
  — useful in both build modes.
- Given a `platform.toml` + an image tag (or `:latest`), resolve the bundled set from
  `plugins/*/plugin.toml` in a checkout (behind the `develop` / `source-build` gates) **or**
  from the `junius.bundled_plugins` OCI label via `docker pull --quiet` when source isn't
  local; report the diff and exit non-zero on mismatch.

**First consumer:** a deployment fixture with `enabled = ["events"]` against a monorepo
bundling `["events", "admin"]`.

## Acceptance criteria

- [ ] `junius doctor` against the fixture reports "extra in bundle: admin" and exits non-zero.
- [ ] A matching deployment exits zero.
- [ ] Works both from a source checkout and from the OCI label path (no local source).

## Blocked by

- None — can start immediately.
