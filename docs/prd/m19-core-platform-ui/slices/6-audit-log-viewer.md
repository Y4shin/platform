---
kind: feature
title: "Audit log viewer"
slug: audit-log-viewer
issue: 6
prd: ../prd.md
mode: hitl
---

# Slice #6 — Audit log viewer

Full design: [`21-M19-platform-ui.md` §Tier 2.A](../../../impl/21-M19-platform-ui.md#tier-2a--audit-log-viewer-padminaudit).

## What to build

End-to-end (admin plugin): an admin browses `platform.audit_event` at `/p/admin/audit` with
filters, pagination, and a JSON details drawer. Reads existing data — **no new tables**.

- Proto `AuditService.List(ListAuditRequest) → ListAuditResponse`, guarded
  `requires = "admin:audit.read"`. Request filters: `actor_user_id`, `resource_kind`,
  `event_kind`, `starts_at`, `ends_at` (RFC3339), `cursor`, `limit` (default 50, max 200).
- `src/repo/audit.rs` + `src/service/audit.rs` reading `platform.audit_event`; **cursor-based
  pagination**, opaque cursor = `(occurred_at, id)`.
- `frontend/src/routes/pages/AuditPage.tsx`: filter bar + table (when, actor, event_kind,
  resource_kind → resource_id); clicking a row opens a drawer with the full JSON `details`.
- New permission `admin:audit.read` added to the M18 `admin` plugin's manifest, granted by the
  builtin `admin` user-role's `'*'`.

## Acceptance criteria

- [ ] Every M18 Stage-2 mutation (create group, add role, assign permission, add member)
      appears in `/p/admin/audit` with the actor + resource.
- [ ] Filters narrow the table (e.g. `event_kind = 'admin:membership.add'`).
- [ ] Clicking a row opens a drawer showing the JSON `details`.
- [ ] Pagination is cursor-based (no `OFFSET`/`LIMIT`).
- [ ] All new strings ship through Lingui.
- [ ] Playwright spec under `plugins/admin/frontend/e2e/`.

## Blocked by

- None — can start immediately (the M18 `admin` plugin is shipped).

## Test plan

**Test type:** rust-integration (correctness backbone) + e2e (required UI smoke) — a deliberate split; the two layers catch genuinely different failure modes.
**Reasoning:** Keyset cursor pagination and filter narrowing are backend SQL logic best proven against real Postgres (testcontainers) — fast feedback while writing the query, and the equal-`occurred_at` tie-break is impractical to exercise through a browser; the AC-mandated Playwright spec then stays a thin smoke over the UI wiring (filter narrows the visible table, row click opens the drawer with JSON `details`) that the backend test cannot see.

### Assertions

**rust-integration** (`AuditService.List` / `repo/audit.rs` over a seeded `platform.audit_event`, real Postgres):
- **Cursor tie-break:** seed multiple rows with *identical* `occurred_at`; paging through with `limit` < total yields every row **exactly once** — no duplicates, no gaps at the page seam (proves the `(occurred_at, id)` composite cursor, not a naive `occurred_at <` predicate).
- **Ordering:** results are newest-first by `(occurred_at DESC, id DESC)` and stable across pages; the returned cursor round-trips (next call resumes exactly after the last row).
- **Each filter narrows correctly** *inside* the keyset query (not post-filtered): `actor_user_id`, `resource_kind`, `event_kind`, and the `starts_at`/`ends_at` (RFC3339) date range each restrict the rows; combined filters AND together; a filter that matches nothing returns an empty page with no cursor.
- **`limit` handling:** absent → default 50; > 200 → clamped to 200; respected as the page size.
- **No `OFFSET`/`LIMIT`-offset pagination** — pagination is keyset only.
- **Permission guard:** a call lacking `admin:audit.read` is rejected (PermissionDenied / Err), a call holding it (builtin `admin` user-role `'*'`) succeeds.
- **Cursor robustness:** a malformed/garbage opaque cursor yields a clean error (InvalidArgument), not a panic or a silent full scan.

**e2e** (Playwright, admin logged in, audit rows seeded via `db`):
- The page at `/p/admin/audit` renders the table columns (when, actor, event_kind, resource_kind → resource_id).
- Applying a filter (e.g. `event_kind = 'admin:membership.add'`) narrows the visible rows.
- Clicking a row opens a drawer showing the full JSON `details`.
- All visible strings are Lingui-sourced (no raw literals).

### Test file
- `plugins/admin/tests/audit_pg.rs` (rust-integration; testcontainers + host migrations, mirrors the `platform/tests/admin_*_pg.rs` pattern — reconcile the exact home at implement time if the audit read goes through a host capability rather than a plugin-local repo).
- `plugins/admin/frontend/e2e/audit.spec.ts` (e2e).

### Run command
`task test:rust` (rust-integration) · `task test:e2e` (Playwright)
