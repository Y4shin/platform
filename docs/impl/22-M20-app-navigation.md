# 22. M20 — Plugin apps + left-rail picker with sub-nav

> **📦 Migrated to a PRD.** Planning of this (still-unbuilt) milestone moved to the PRD
> workflow — see [`docs/prd/m20-app-navigation/prd.md`](../prd/m20-app-navigation/prd.md) and
> the [MIGRATION.md](../../MIGRATION.md) rollout. This doc stays as the **full design
> reference**; track active work via the PRD's tracking issue
> ([#10](https://github.com/Y4shin/platform/issues/10)), not this doc.

> **Status:** 🚧 planned. Sequenced **after [M19](21-M19-platform-ui.md)** —
> M20 reshapes the nav surface M19 deliberately left as a stepping stone (the
> hand-edited `NavLinks` from M04) and changes the dashboard's tile source
> from "one tile per plugin" to "one tile per app." Depends on M19 for the
> `@junius/design/Menu` (Radix dropdown) primitive, the `lucide-react` icon
> dep, and the M18 `admin` plugin whose operator pages become five sibling
> *apps* under this scheme.

One-line goal: a plugin declares one or more named **apps** in its manifest;
the host renders a **left-rail picker** of apps the viewer can reach, with a
**sub-nav rail** underneath showing where they are inside the active app.

## Why this milestone exists

M19 fills out the host frontend's user/operator surfaces but **does not**
redesign the top nav — that was deferred here because it's a structural
change, not a feature add:

- M04 ships a hardcoded `Home`/`Events` `NavLinks` ([NavLinks.tsx](../../platform/frontend/src/layout/NavLinks.tsx)).
  M19's Tier 1.B was originally going to extend it with a permission-filtered,
  manifest-driven version (one entry per plugin); on reflection, "one entry
  per plugin" is **already the wrong unit** — the M18 admin plugin has five
  logical surfaces (groups, audit, jobs, plugin inventory, health) that the
  rest of the UI treats as peers, and the events plugin has at least two
  (events themselves + calendar feeds) that share a route tree but no nav.
- Flat-listing five admin items as sub-nav under one "Admin" entry would
  bury them; surfacing them as siblings in a header is too crowded. The
  pattern this is asking for is the **left-rail app picker** found in tools
  with comparable surface area (Linear, GitLab, AWS console).
- M13 friction row #11 flagged that plugin sub-route navigation is
  type-erased (`buildRoutes` returns `AnyRoute[]`); [M16 item D](18-M16-authoring-ergonomics.md#d--blessed-public-handler--plugin-navigation-affordances)
  is going to bless `usePluginNavigate`/`PluginLink`. M20 is the place those
  affordances start carrying traffic at the *platform* level (the rail
  renders typed sub-nav links into a plugin's own routes).

## Outcome / acceptance

- A plugin's `plugin.toml` declares `[[plugin.apps]]` entries; each app
  carries a `key`, `display_name`, `icon`, `path`, optional `section`,
  optional `visible_with` permissions list, optional `short_description`,
  and an optional sub-nav table `[[plugin.apps.nav]]`.
- `junius sync` emits `platform/frontend/src/generated/plugin-apps.ts`
  (this **replaces** M19's planned `plugin-nav.ts` — see Downstream
  updates).
- The host frontend grows a **left rail** that:
  - shows the **current app's** icon + name + ▾ chevron as the picker
    trigger;
  - shows the **current app's sub-nav** under the trigger, with the active
    sub-page bar-highlighted;
  - opens a **fly-out picker** on click / ⌘K listing every app the viewer
    can reach, grouped by manifest-declared `section` (default `your_apps`;
    admin opts into `admin`), with the current app checkmarked;
  - closes on outside-click, Esc, or selection.
- Apps with `visible_with` are filtered at render time against the viewer's
  permissions; apps the viewer can't reach are **absent** (no greyed-out
  rows in v1).
- The M19 `<DashboardPage>` tile grid now reads `APPS` instead of
  `PLUGIN_NAV`: a plugin with two apps gets two tiles; the empty-state and
  the `admin_contact_email` link stay as M19 specified.
- Existing plugins (`events`, `hello`, `admin`, plus toy `greetings` /
  `widgets`) declare their `[[plugin.apps]]` blocks and are wired up.
- Playwright (M17) spec covers: opening the picker, ⌘K binding,
  permission-filtered visibility, sub-nav active-item highlight, and a
  full *log-in → pick app → navigate sub-nav → switch app* journey.

## Design

### A — Manifest schema

A new `[[plugin.apps]]` array on `plugin.toml`. The optional `[nav]`
table M19 sketched is **superseded** by this — `[nav]` is removed before
landing; no migration shim, since M19's `[nav]` hasn't shipped yet.

```toml
# plugins/events/plugin.toml

[[plugin.apps]]
key               = "events"
display_name      = "Events"
icon              = "calendar"               # Lucide name (M19 added the dep)
path              = "/p/events"
section           = "your_apps"              # default; "admin" for admin-section apps
visible_with      = ["events:read"]          # any-of; default = any perm this plugin declares
short_description = "Create and share events."

[[plugin.apps.nav]]
label = "Overview" ; path = "/p/events"
[[plugin.apps.nav]]
label = "Calendar" ; path = "/p/events/calendar"
[[plugin.apps.nav]]
label = "Invites"  ; path = "/p/events/invites" ; visible_with = ["events:write"]
[[plugin.apps.nav]]
label = "Feeds"    ; path = "/p/events/feeds"

# A second app from the same plugin — appears as a sibling in the picker,
# not nested under "Events".
[[plugin.apps]]
key               = "events_feeds"
display_name      = "Calendar feeds"
icon              = "rss"
path              = "/p/events/feeds"
visible_with      = ["events:read"]
short_description = "Manage your iCalendar subscriptions."
```

For the M18 admin plugin (each operator surface is its own app):

```toml
[[plugin.apps]]
key = "admin_groups"  ; display_name = "Groups & roles" ; icon = "users"     ; path = "/p/admin/groups"  ; section = "admin" ; visible_with = ["admin:groups.read"]
[[plugin.apps]]
key = "admin_audit"   ; display_name = "Audit log"      ; icon = "file-text" ; path = "/p/admin/audit"   ; section = "admin" ; visible_with = ["admin:audit.read"]
[[plugin.apps]]
key = "admin_jobs"    ; display_name = "Jobs"           ; icon = "cog"       ; path = "/p/admin/jobs"    ; section = "admin" ; visible_with = ["admin:jobs.read"]
[[plugin.apps]]
key = "admin_plugins" ; display_name = "Plugins"        ; icon = "package"   ; path = "/p/admin/plugins" ; section = "admin" ; visible_with = ["admin:plugins.read"]
[[plugin.apps]]
key = "admin_health"  ; display_name = "Health"         ; icon = "heart"     ; path = "/p/admin/health"  ; section = "admin" ; visible_with = ["admin:health.read"]
```

**Manifest validation** ([`junius check`](../../tools/junius/src/commands/check.rs)):
- Each app's `path` must be a prefix of an actual route emitted by the
  plugin's `buildRoutes` (cross-checked against the same compiled metadata
  M12 uses for FE-export rules).
- Each sub-nav `path` must be under the parent app's `path`.
- `key` is unique deployment-wide (the host concatenates apps from all
  enabled plugins; duplicate keys are a sync error).
- `icon`, if set, must be a known Lucide name (the codegen embeds the
  allowlist from the `lucide-react` package version pinned by the host).
- An app whose `visible_with` lists a permission the plugin doesn't
  declare (and isn't an `admin:*` permission from the M18 plugin) is a
  warning — usually a typo.

### B — `junius sync` codegen

```ts
// platform/frontend/src/generated/plugin-apps.ts (generated)

export interface AppNavEntry {
  label: string;
  path: string;
  visibleWith: string[];
}

export interface AppEntry {
  key: string;
  pluginName: string;
  displayName: string;
  icon: string;
  path: string;
  section: string;
  visibleWith: string[];
  shortDescription: string;
  nav: AppNavEntry[];
}

export const APPS: AppEntry[] = [ /* … */ ];

// Stable order; deployments override via [config] app_section_order if needed.
export const SECTION_ORDER: string[] = ['your_apps', 'admin'];

// Optional per-plugin app-key constants the plugin's own FE can import for
// typed `<PluginLink to={apps.events_calendar.path}>` style usage (folds
// into the M16-D nav affordances).
export const APP_KEYS = { events: 'events', events_feeds: 'events_feeds', /* … */ } as const;
```

The generator integrates with the existing `sync` codegen pipeline; the
emitted file is rustfmt-equivalent-clean (Biome-formatted), so
`sync --dry-run` stays drift-free.

### C — Layout + components

[`Shell.tsx`](../../platform/frontend/src/layout/Shell.tsx) becomes
two-column:

```
┌── header ─────────────────────────────────────────────────────────────┐
│ Junius                                       🌐 EN     Alice ▾        │
├── left rail ────────┬── main ─────────────────────────────────────────┤
│ [App picker]    ▾   │                                                 │
│                     │                                                 │
│   [sub-nav rail]    │              <Outlet />                         │
│                     │                                                 │
└─────────────────────┴─────────────────────────────────────────────────┘
```

New components under `platform/frontend/src/layout/`:

- **`LeftRail.tsx`** — orchestrates the picker trigger + sub-nav. Resolves
  the active app via a **longest-prefix match** on `APPS[i].path` against
  the current router location (`useRouterState`). Dashboard (`/`) yields
  *no* active app and renders a synthetic "Dashboard" trigger with empty
  sub-nav.
- **`AppPicker.tsx`** — Radix `DropdownMenu` (M19's primitive). Renders
  apps grouped by `section`, with the section labels (`YOUR APPS`,
  `ADMIN`) drawn from a small i18n catalogue. Current app gets a
  checkmark; selection navigates and closes. ⌘K opens it from anywhere
  (a single global `useEffect` keydown listener attached at `Shell`
  mount).
- **`SubNavRail.tsx`** — renders the active app's `nav`, filtered by the
  per-entry `visibleWith`. Active item gets a left bar (Tailwind:
  `border-l-2 border-accent -ml-px`); inactive items hover with
  `text-fg-2 → text-fg-1`.
- **`ActiveAppContext`** — a thin React context exposing the active
  `AppEntry | null` so the optional `useActiveApp()` SDK hook (Stage 4)
  has a single source of truth.

The header **loses `NavLinks`** entirely; it becomes brand + locale
switcher + user menu (the M19 design, minus the nav).

### D — Dashboard re-source

The M19 `<DashboardPage>` tile grid currently reads `PLUGIN_NAV` (one
tile per plugin). M20 changes it to read `APPS` (one tile per app):

```tsx
// Was (M19):
const visible = PLUGIN_NAV.filter(p => userHasAny(user, p.requiredPermissions));
// Becomes:
const visible = APPS.filter(a => userHasAny(user, a.visibleWith));
```

A plugin with two apps therefore yields two tiles, ordered by
`SECTION_ORDER` then declaration order. The empty-state (zero apps
visible ⇒ `<NoAccessEmptyState>` with `admin_contact_email`) is unchanged.

### E — Active-app awareness for plugin code (optional)

A small SDK addition for the cases where a plugin's own page wants to
know the active app context:

```ts
// packages/sdk/src/registry/useActiveApp.ts
export interface ActiveApp { key: string; displayName: string; path: string; nav: AppNavEntry[] }
export function useActiveApp(): ActiveApp | null;
```

Backed by the `ActiveAppContext` from §C. Useful for an in-page
breadcrumb / a sub-action button that wants to inherit the app's icon —
neither shipped by any current plugin, but cheap to expose now that the
data is centralized.

## Stages

Repo norm: one **unsigned** commit per stage, `task ci` green per
stage, sign+push the batch at the end.

- **Stage 1 — Manifest schema + codegen.** Add `[[plugin.apps]]` to
  `crates/junius-manifest`; `junius sync` emits `plugin-apps.ts`;
  `junius check` enforces the validation rules in §A. *Verify:* a fresh
  `junius sync` produces a `plugin-apps.ts` matching the plugins
  currently enabled in `dev/platform.toml`; introducing a duplicate
  `key` or an unknown icon fails `junius check` with a clear message.
- **Stage 2 — Left rail + picker chrome.** New `<LeftRail>`,
  `<AppPicker>`, `<SubNavRail>` components; `Shell` switches to
  two-column; header drops `NavLinks`; ⌘K binding; `ActiveAppContext`.
  No plugins declare apps yet — the rail shows an empty picker + a
  "no apps available" stub, the dashboard still works (existing
  `<NoAccessEmptyState>` path). *Verify:* the new chrome renders;
  ⌘K opens the (currently empty) picker; Esc closes it; the page
  still navigates to `/`, `/me`, `/403`, etc.
- **Stage 3 — Wire existing plugins.** Declare `[[plugin.apps]]` in
  `events` (2 apps + sub-nav for Events), `admin` (5 apps in the
  `admin` section), `hello` (1 app), `greetings` / `widgets` (1 each
  if still enabled). *Verify:* alice (admin) opens the picker and
  sees `YOUR APPS` (events, calendar feeds, hello, greetings,
  widgets) + `ADMIN` (5); bob (`events:read` only) sees the two
  events apps; the zero-perm user sees nothing.
- **Stage 4 — Dashboard re-source + `useActiveApp`.** Switch the
  `<DashboardPage>` tile source from `PLUGIN_NAV` to `APPS`; ship
  `useActiveApp` from the SDK; document it in the authoring guide
  alongside `usePluginNavigate`/`PluginLink` (the M16-D affordances).
  *Verify:* alice sees the full grid of app tiles on `/`; bob sees
  two; a vitest covers `useActiveApp` returning the right entry
  inside a route under the app's path.
- **Stage 5 — E2E + docs.** Playwright spec under
  `e2e/cross/app-navigation.spec.ts`: log in → assert rail layout →
  open picker → ⌘K → arrow-down + Enter → land on a sub-page →
  sub-nav active item highlighted → switch apps. Authoring-guide
  section on declaring apps. Decision-log entry.

## Out of scope

- **Pinning / starring favourite apps.** The manifest is the single
  source of truth; per-user reordering / pinning is a future polish.
- **Reordering apps from the UI.** Manifest only in v1.
- **User-defined sections.** Only the manifest's declared `section`
  is honoured (default `your_apps`, `admin` for the admin plugin);
  custom sections are allowed but won't appear unless added to
  `SECTION_ORDER`.
- **Nested sub-nav.** Sub-nav is **one level deep**. Plugins that want
  more structure inside an app render it inside their page content
  (tabs, accordions, etc.), not the rail.
- **Mobile drawer.** Below the breakpoint, the left rail just stacks
  above content (Tailwind default); a proper slide-in drawer is later
  polish.
- **Plugin-defined keyboard shortcuts.** Apps don't claim their own
  shortcuts beyond ⌘K (which is reserved for the picker). A real
  command palette (`cmdk`-style) is a much bigger feature, not this
  milestone.
- **Notification badges on apps.** A "3 new audit events" red dot
  requires a count-source contract no plugin exposes; defer until a
  consumer asks for it.
- **Per-app theming.** All apps share the global theme tokens.

## Library choices — confirm with user before starting

| Choice | Proposed default | Rationale | Notes |
|---|---|---|---|
| **Dropdown primitive** | `@radix-ui/react-dropdown-menu` via `@junius/design/Menu` (M19) | Already a dep; keyboard nav + a11y solved | Alternative: `react-aria-components`. Rejected — second a11y library for one screen isn't worth it. |
| **⌘K binding** | Hand-rolled `useEffect` keydown at the `Shell` level | Single global trigger; ~30 LoC | If we later add a real command palette, swap to `cmdk` then. |
| **Active-app resolution** | Longest-prefix match on `APPS[i].path` against `useRouterState().location.pathname` | Trivial; no per-app self-declaration of "am I active" needed | The only gotcha is `/` (Dashboard) — handled as a special case (no active app). |
| **Sub-nav active highlight** | Left bar (`border-l-2 border-accent`) + slight bg tint on hover | Calm; reads at a glance even without color | A trailing dot/chevron is an alternative if accessibility review prefers redundancy. |
| **Icon vocabulary** | Lucide; manifest accepts the icon `kebab-case` name; codegen validates against the pinned package | Tree-shakes; consistent across the host & every plugin | A plugin can ship a custom icon component only by registering it via the M09 component registry (out of scope for v1). |
| **Section labels source** | A small host-side i18n catalogue (`your_apps`, `admin`, …) | Picker labels need translation alongside everything else (M14) | A deployment-level override is possible later. |
| **`section` value contract** | Free-form string; the host renders any unknown section *after* `SECTION_ORDER` entries, sorted alphabetically | Forward-compatible: a future plugin's new section appears, just at the bottom | — |
| **App-tile order on dashboard** | Same as picker: `SECTION_ORDER` first, then declaration order within section | Single ordering rule; predictable | Alternative: per-tile `dashboard_order` integer. Rejected — manifest noise for marginal benefit. |
| **Empty-state behaviour** | If `APPS.filter(...).length === 0` the rail shows a *Dashboard*-only trigger + the M19 `<NoAccessEmptyState>` on the main panel | Avoids a dead left rail | — |
| **`[[plugin.apps]]` required?** | **Yes** for plugins with a frontend; `junius check` errors if a FE plugin has zero apps | Otherwise the plugin is invisible to users — almost always a bug | A backend-only plugin (no `frontend/`) is unaffected. |

## Open questions resolved

- **App vs plugin as the nav unit** → **app**. A plugin can expose 1..N
  apps; each is a peer in the picker.
- **One vs many sections** → manifest-declared `section` string; the
  host ships an order (`your_apps`, then `admin`). Unknown sections
  surface at the bottom, alphabetically — forward-compatible.
- **Sub-nav depth** → **one level**. Deeper structure is in-page
  content (tabs, etc.), not the rail.
- **Picker affordance** → fly-out dropdown anchored to the trigger,
  ⌘K-openable. Not a slide-in panel; not a modal.
- **Dashboard tile source** → `APPS` (not `PLUGIN_NAV`). M19's plugin-
  unit tiling is superseded.

## Downstream doc updates

- [`README.md`](README.md) — add the M20 row.
- [`21-M19-platform-ui.md`](21-M19-platform-ui.md) — **amend Tier 1.B
  and Stage 3**: drop the `plugin-nav.ts` codegen + the
  permission-filtered header `NavLinks`. M19 keeps the hardcoded
  `NavLinks` from M04 until M20 lands; explicit, intentional, called
  out as such. **Dashboard tile source** moves to M20 too (M19 still
  ships the dashboard *page*, but its tile source is `APPS` once M20
  is in; until then, the page can render a placeholder or read the
  current hardcoded plugin list). Also: M19 friction-closure on
  `requirePermissions` is unaffected — it's a route-guard concern, not
  a nav concern.
- [`20-M18-group-role-provisioning.md`](20-M18-group-role-provisioning.md) —
  in the admin plugin's `plugin.toml` example, add the five
  `[[plugin.apps]]` entries (section = `admin`). Note: M18 itself
  doesn't depend on M20 — it can ship with `NavLinks` static; M20
  retrofits the nav.
- [`14-M13-events-plugin.md`](14-M13-events-plugin.md) — friction row
  #11 (typed plugin sub-route nav) closure pointer: "fixed across
  M16-D (typed `PluginLink`) and M20 (left-rail sub-nav consumes the
  same typed paths)."
- [`../plugin-authoring-guide.md`](../plugin-authoring-guide.md) — new
  "declaring apps" section; positions `[[plugin.apps]]` as **the**
  way a plugin appears in the UI; deprecates the M19-draft `[nav]`
  sketch (which never landed).
- [`../design/14-decision-log.md`](../design/14-decision-log.md) — an
  M20 entry: app-as-nav-unit, left-rail + picker, manifest-declared
  sub-nav.

## Verification

```bash
# Bootstrap (after M19).
docker compose -f dev/docker-compose.yml up -d
target/release/junius migrate up      --config dev/platform.toml
target/release/junius provision apply --config dev/platform.toml   # M18
target/release/junius sync            --config dev/platform.toml   # emits plugin-apps.ts
target/release/junius dev             --config dev/platform.toml &

# Stage 1 — manifest + codegen.
cat platform/frontend/src/generated/plugin-apps.ts        # contains events / events_feeds / hello / admin_* / …
target/release/junius check                               # passes
# Introduce a duplicate `key` in plugins/events/plugin.toml → check errors.

# Stage 2/3 — left rail + picker.
# As alice (admin user-role from M18):
#  - Header: brand + locale + user menu (no NavLinks).
#  - Left rail: "🏠 Dashboard ▾" trigger on /.
#  - Click trigger → fly-out lists YOUR APPS (events, calendar feeds, hello, …)
#    and ADMIN (groups, audit, jobs, plugins, health).
#  - Press ⌘K from anywhere → picker opens; arrows + Enter selects → navigate.
#  - Land on /p/events → trigger now reads "📅 Events ▾"; sub-nav shows
#    Overview / Calendar / Invites / Feeds, with Overview bar-highlighted.
#  - Click Calendar → URL becomes /p/events/calendar; Calendar bar-highlighted.
#  - Click trigger → fly-out shows Events ✓; pick Audit log → land on
#    /p/admin/audit; trigger now "📜 Audit log ▾"; sub-nav = Recent /
#    By resource / By actor.

# As bob (events:read only):
#  - Picker shows YOUR APPS = {Events, Calendar feeds}; no ADMIN section.
#  - Navigating to /p/admin/audit directly → /403 (M19's route guard).

# As a fresh OIDC user (no perms):
#  - Picker shows only the synthetic "Dashboard" entry.
#  - Dashboard renders the M19 <NoAccessEmptyState> with admin_contact_email.

# Stage 4 — dashboard re-source.
#  - As alice on /: tile grid lists Events, Calendar feeds, Hello, (admin
#    section header), Groups & roles, Audit log, Jobs, Plugins, Health.
#  - As bob: two tiles (Events, Calendar feeds).

# CI gate.
target/release/junius check
cargo test --workspace
pnpm exec playwright test     # exercises Stage 5 spec
```

End of M20. The host's primary nav is no longer hand-edited; plugins
declare what surfaces they expose; the user always knows which app
they're in and what else is reachable. Future work (pinning,
notification badges, deeper sub-nav, a true command palette) builds on
the manifest schema this milestone established.
