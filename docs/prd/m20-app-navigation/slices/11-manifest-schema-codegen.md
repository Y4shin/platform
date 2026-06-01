---
kind: feature
title: "Manifest schema + `plugin-apps.ts` codegen + `junius check` validation"
slug: manifest-schema-codegen
issue: 11
prd: ../prd.md
mode: hitl
---

# Slice #11 — Manifest schema + `plugin-apps.ts` codegen + `junius check` validation

Full design: [`22-M20-app-navigation.md` §A–B](../../../impl/22-M20-app-navigation.md#a--manifest-schema).

## What to build

The data foundation for app-based nav: a manifest schema, generated TS, and validation —
end-to-end from `plugin.toml` to a typed `APPS` array the host can render.

- `[[plugin.apps]]` array on `plugin.toml` in `crates/junius-manifest`: `key`, `display_name`,
  `icon` (Lucide kebab name), `path`, optional `section` (default `your_apps`), optional
  `visible_with` (any-of permissions; default = any perm the plugin declares), optional
  `short_description`, and an optional `[[plugin.apps.nav]]` sub-nav table (`label`, `path`,
  optional `visible_with`). Supersedes M19's never-shipped `[nav]` sketch (no shim).
- `junius sync` emits `platform/frontend/src/generated/plugin-apps.ts`: `AppEntry` /
  `AppNavEntry` interfaces, the `APPS` array, `SECTION_ORDER` (`['your_apps','admin']`), and
  `APP_KEYS`. Biome-formatted so `sync --dry-run` stays drift-free.
- `junius check` validation: each app `path` is a prefix of a real route from the plugin's
  `buildRoutes`; each sub-nav `path` nests under its app's `path`; `key` is unique
  deployment-wide; `icon` is a known Lucide name (allowlist embedded from the pinned package);
  a `visible_with` permission the plugin doesn't declare (and isn't `admin:*`) is a warning.
  A plugin with a `frontend/` but zero apps is an error.

## Acceptance criteria

- [ ] A fresh `junius sync` produces a `plugin-apps.ts` matching the plugins enabled in
      `dev/platform.toml`.
- [ ] `sync --dry-run` reports no drift after a sync.
- [ ] A duplicate `key` fails `junius check` with a clear message.
- [ ] An unknown icon name fails `junius check`.
- [ ] A frontend plugin declaring zero apps fails `junius check`.

## Blocked by

- None — can start immediately.
