---
kind: epic
title: Platform UX foundations
slug: platform-ux-foundations
epic_issue: 31
prds:
  - slug: m19-core-platform-ui
    kind: feature
    issue: 1
    blocked_by: []
  - slug: m20-app-navigation
    kind: feature
    issue: 10
    blocked_by: [m19-core-platform-ui]
status: in-progress
---

# Platform UX foundations

> Retrofitted epic grouping milestones **M19** + **M20**. Each child PRD links to its own
> milestone doc under [`docs/impl/`](../../../impl/) for the full design. This epic captures
> the cross-cutting story the two PRDs share.

## Problem / outcome

The host frontend was an *intentional* skeleton through M04–M18: every feature milestone added
only what it needed. The result is a platform with no front door — `/` renders `null`, there's
no logout/profile/403, a zero-permission OIDC user lands on a dead app, and the operator
surfaces (audit, jobs, plugin registry, health) are invisible outside `psql`. And the nav that
ties it all together is still M04's hardcoded `Home`/`Events` list.

**Outcome:** a host frontend that feels like a finished product — every surface a web app needs
*and* every surface an operator needs, organised under a real navigation model. M19 fills the
surfaces; M20 reshapes how you move between them.

## Constituent plugins & surfaces

- **Host frontend** (`platform/`) — `Shell` chrome, page templates, dashboard, `/me` profile,
  403/500 error pages, the left-rail app picker + typed sub-nav.
- **`admin` plugin** — its five operator surfaces (groups & roles, audit log, jobs, plugin
  inventory, health) become sibling *apps* in the rail.
- **`events` plugin** — its events + calendar-feeds surfaces become apps with shared route-tree
  sub-nav.

## Shared / foundational work

- `@junius/design/Menu` (Radix dropdown) primitive + `lucide-react` icons (introduced in M19,
  consumed by M20's rail).
- The `[[plugin.apps]]` manifest schema + codegen and the `usePluginNavigate`/`PluginLink`
  typed-nav affordances (M20) that every plugin now declares against.

## Per-plugin features

- **M19 — Core platform UIs** (`m19-core-platform-ui`): dashboard, user menu + logout, `/me`
  profile, 403/500 pages, and the Tier-2 operator pages extending the M18 admin plugin.
- **M20 — Plugin apps + left-rail navigation** (`m20-app-navigation`): the manifest `apps`
  model, the rail chrome + picker, wiring existing plugins, and re-sourcing the M19 dashboard
  tiles from the active app.

## Dependency ordering

`m20-app-navigation` is **blocked by** `m19-core-platform-ui`: the rail consumes M19's `Menu`
primitive and re-sources the dashboard tiles M19 establishes.

## Out of scope

Onboarding/empty-state tutorials, notifications, cross-plugin search, theming, avatars — each
deferred in the child PRDs. This epic is the foundational chrome, not the full UX backlog.

## Open questions

None outstanding — resolved per-slice in the M19/M20 milestone docs.

## Decomposition

1. **`m19-core-platform-ui`** (feature, #1) — the every-app + operator surfaces and the shared
   chrome they need.
2. **`m20-app-navigation`** (feature, #10, after #1) — the app-based navigation model that
   organises those surfaces.
