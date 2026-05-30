# Slice #12 — Left rail + app picker chrome

**PRD:** ../prd.md · **kind:** feature · **mode:** hitl

Full design: [`22-M20-app-navigation.md` §C](../../../impl/22-M20-app-navigation.md#c--layout--components).

## What to build

The host chrome that renders apps — independent of any plugin declaring them yet (with zero
apps the rail shows a Dashboard-only trigger and the page still navigates).

- `Shell` becomes two-column (header row + left rail + main `<Outlet />`); the header **loses
  `NavLinks`** entirely (brand + locale switcher + user menu only).
- `LeftRail` — orchestrates the picker trigger + sub-nav; resolves the active app via
  **longest-prefix match** of `APPS[i].path` against the router location; `/` yields no active
  app (synthetic "Dashboard" trigger, empty sub-nav).
- `AppPicker` — Radix `DropdownMenu` (M19's `@junius/design/Menu`), apps grouped by `section`
  with i18n section labels, current app checkmarked; selection navigates + closes; closes on
  outside-click/Esc. A global ⌘K keydown listener at `Shell` mount opens it.
- `SubNavRail` — renders the active app's `nav`, filtered by per-entry `visible_with`; active
  item gets a left bar.
- `ActiveAppContext` — exposes the active `AppEntry | null` as the single source of truth for
  the later `useActiveApp()` hook.

## Acceptance criteria

- [ ] The new two-column chrome renders; the header no longer shows `NavLinks`.
- [ ] ⌘K opens the picker from anywhere; Esc / outside-click / selection closes it.
- [ ] With no plugins declaring apps, the picker shows a Dashboard-only / "no apps available"
      state and the page still navigates to `/`, `/me`, `/403`.
- [ ] All new strings ship through Lingui.

## Blocked by

- #11 — consumes the generated `APPS` / `SECTION_ORDER` from the codegen slice.
