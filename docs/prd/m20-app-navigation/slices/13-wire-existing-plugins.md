---
kind: feature
title: "Wire existing plugins' `[[plugin.apps]]`"
slug: wire-existing-plugins
issue: 13
prd: ../prd.md
mode: hitl
---

# Slice #13 — Wire existing plugins' `[[plugin.apps]]`

Full design: [`22-M20-app-navigation.md` §A + Stage 3](../../../impl/22-M20-app-navigation.md#stages).

## What to build

Declare `[[plugin.apps]]` in the existing plugins so the rail + picker carry real traffic.

- `events` — two apps (`events` with Overview/Calendar/Invites/Feeds sub-nav; `events_feeds`
  as a sibling), `your_apps` section, `visible_with = ["events:read"]` (Invites gated on
  `events:write`).
- `admin` — five apps in the `admin` section (Groups & roles, Audit log, Jobs, Plugins,
  Health), each gated on its `admin:*.read` permission.
- `hello` — one app; `greetings` / `widgets` — one each if still enabled.

## Acceptance criteria

- [ ] Alice (admin) opens the picker and sees `YOUR APPS` (events, calendar feeds, hello,
      greetings, widgets) + `ADMIN` (groups, audit, jobs, plugins, health).
- [ ] Bob (`events:read` only) sees the two events apps and no ADMIN section.
- [ ] A zero-permission user sees only the synthetic Dashboard entry.
- [ ] On `/p/events`, the sub-nav shows Overview / Calendar / Invites / Feeds with the active
      item bar-highlighted.
- [ ] All new strings ship through Lingui.

## Blocked by

- #11 — needs the manifest schema + validation.
- #12 — needs the rail/picker to render the declared apps.
