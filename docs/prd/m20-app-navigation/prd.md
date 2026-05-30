---
kind: feature
title: Plugin apps + left-rail navigation
slug: m20-app-navigation
milestone: M20
prd_issue: 10
slices: [11, 12, 13, 14]
status: issues-created
---

# Plugin apps + left-rail navigation

> Migrated from the M20 milestone doc
> [`docs/impl/22-M20-app-navigation.md`](../../impl/22-M20-app-navigation.md), which holds the
> **full design** (manifest schema, codegen interfaces, layout sketches, validation rules).
> This PRD is the planning surface; it links back rather than duplicating. Sequenced **after
> M19** — it reshapes the nav surface M19 deliberately left as a stepping stone (the M04
> hardcoded `NavLinks`) and re-sources the M19 dashboard tiles. Depends on M19's
> `@junius/design/Menu` (Radix dropdown) primitive + `lucide-react`, and on the M18 `admin`
> plugin (its five operator surfaces become sibling *apps*).

## Problem / why

M19 fills out the host frontend's surfaces but does **not** redesign the top nav — that was
deferred here because it's a structural change, not a feature add:

- M04 ships a hardcoded `Home`/`Events` `NavLinks`. The obvious "one entry per plugin"
  replacement is **the wrong unit**: the M18 admin plugin has five logical operator surfaces
  the rest of the UI treats as peers, and the events plugin has at least two (events +
  calendar feeds) that share a route tree but no nav.
- Flat-listing five admin items under one "Admin" entry buries them; surfacing them all in a
  header is too crowded. The pattern this calls for is a **left-rail app picker** (Linear,
  GitLab, AWS console).
- M13 friction #11 flagged that plugin sub-route nav is type-erased; M16-D blesses
  `usePluginNavigate`/`PluginLink`. M20 is where those affordances start carrying traffic at
  the platform level (the rail renders typed sub-nav into a plugin's own routes).

## User stories

- As a plugin author I declare one or more named **apps** in `plugin.toml`
  (`[[plugin.apps]]`: key, display_name, icon, path, optional section, optional `visible_with`
  permissions, optional short_description, optional `[[plugin.apps.nav]]` sub-nav). A
  backend-only plugin declares none; a frontend plugin with zero apps is a `junius check`
  error.
- As a user I see a **left rail**: the current app's icon + name + chevron as a picker
  trigger, the current app's sub-nav beneath it (active sub-page bar-highlighted), and a
  **fly-out picker** (click or ⌘K) listing every app I can reach, grouped by `section`
  (`your_apps`, `admin`), with the current app checkmarked. It closes on outside-click, Esc,
  or selection.
- As a user I only see apps I can reach — `visible_with` is filtered at render against my
  permissions; unreachable apps are **absent** (no greyed-out rows in v1).
- As a user the dashboard at `/` shows one **tile per app** (re-sourced from `APPS`), so a
  plugin with two apps gets two tiles; the M19 empty-state + `admin_contact_email` link stay.
- As a plugin author I can optionally call `useActiveApp()` from my own page to read the
  active app context (key, display name, path, sub-nav).

## End-to-end behaviour

A plugin declares `[[plugin.apps]]` in its manifest; `junius sync` emits
`platform/frontend/src/generated/plugin-apps.ts` (an `APPS` array + `SECTION_ORDER` +
`APP_KEYS`), and `junius check` validates the manifest (app `path` is a route prefix, sub-nav
paths nest under it, keys are unique deployment-wide, icons are known Lucide names). The host
`Shell` becomes two-column: the header loses `NavLinks` (brand + locale + user menu only), and
a left rail renders the picker trigger + sub-nav. Alice (admin) opens the picker and sees
`YOUR APPS` (events, calendar feeds, hello, …) + `ADMIN` (groups, audit, jobs, plugins,
health); ⌘K opens it from anywhere; arrow + Enter navigates. Landing on `/p/events` the
trigger reads "Events ▾" and the sub-nav shows Overview / Calendar / Invites / Feeds with the
active item bar-highlighted. Bob (`events:read`) sees only the two events apps and no ADMIN
section; navigating directly to `/p/admin/audit` hits M19's `/403`. A zero-perm user sees only
a synthetic "Dashboard" entry and the M19 empty state. The dashboard at `/` lists one tile per
visible app.

## Layers touched

Full design per section in
[`22-M20-app-navigation.md` §Design](../../impl/22-M20-app-navigation.md#design).

- **Manifest + codegen** (`crates/junius-manifest`, `tools/junius` sync + check): the
  `[[plugin.apps]]` schema (supersedes M19's never-shipped `[nav]` sketch); `plugin-apps.ts`
  codegen; `junius check` validation rules.
- **Host layout** (`platform/frontend/src/layout/`): `LeftRail`, `AppPicker` (Radix dropdown,
  ⌘K), `SubNavRail`, `ActiveAppContext`; `Shell` → two-column; header drops `NavLinks`.
- **Dashboard re-source** (`<DashboardPage>`): tile source `PLUGIN_NAV` → `APPS`.
- **SDK** (`packages/sdk/src/registry/`): optional `useActiveApp()` hook backed by
  `ActiveAppContext`.
- **Plugin manifests**: `events` (2 apps + sub-nav), `admin` (5 apps, `admin` section),
  `hello`, `greetings`/`widgets` (1 each if enabled).
- **Tests/docs**: Playwright spec `e2e/cross/app-navigation.spec.ts`; authoring-guide
  "declaring apps" section; decision-log entry (the latter two via `finalize-prd`).

## Out of scope

- **Pinning / starring / per-user reordering** of apps — manifest is the single source of
  truth in v1.
- **Reordering apps from the UI** — manifest only.
- **User-defined sections** — only manifest-declared `section` is honoured; unknown sections
  surface at the bottom alphabetically.
- **Nested sub-nav** — one level deep; deeper structure is in-page content (tabs, etc.).
- **Mobile slide-in drawer** — the rail just stacks above content below the breakpoint in v1.
- **Plugin-defined keyboard shortcuts / a true command palette** — only ⌘K (reserved for the
  picker); a `cmdk`-style palette is a much bigger feature.
- **Notification badges on apps** — needs a count-source contract no plugin exposes.
- **Per-app theming** — all apps share the global theme tokens.

## Decisions

Carried from the milestone's
[*Library choices*](../../impl/22-M20-app-navigation.md#library-choices--confirm-with-user-before-starting)
table — **confirm before starting**:

- **Dropdown primitive:** `@radix-ui/react-dropdown-menu` via M19's `@junius/design/Menu`.
- **⌘K binding:** hand-rolled `useEffect` keydown at `Shell` level (~30 LoC); swap to `cmdk`
  only if a real palette is added later.
- **Active-app resolution:** longest-prefix match of `APPS[i].path` against the router
  location; `/` (Dashboard) is the special no-active-app case.
- **Sub-nav active highlight:** left bar (`border-l-2 border-accent`) + hover bg tint.
- **Icon vocabulary:** Lucide; manifest takes the kebab-case name; codegen validates against
  the pinned package's allowlist.
- **Section labels:** a small host-side i18n catalogue (`your_apps`, `admin`, …).
- **`section` contract:** free-form string; unknown sections render after `SECTION_ORDER`,
  sorted alphabetically (forward-compatible).
- **Dashboard tile order:** same as picker — `SECTION_ORDER` then declaration order.
- **Empty state:** zero visible apps ⇒ a Dashboard-only trigger + M19's `<NoAccessEmptyState>`.
- **`[[plugin.apps]]` required** for any plugin with a `frontend/`; `junius check` errors on a
  FE plugin with zero apps. Backend-only plugins are unaffected.

## Open questions

Resolved at the design level (see
[§Open questions resolved](../../impl/22-M20-app-navigation.md#open-questions-resolved)): the
nav unit is **app** (not plugin); `section` is a manifest string with a host-shipped order;
sub-nav is **one level**; the picker is a ⌘K-openable fly-out dropdown (not a panel/modal); the
dashboard tile source is `APPS`. No open blockers for slicing.

## Implementation notes
<!-- appended by implement-issue as slices land; empty for now -->
