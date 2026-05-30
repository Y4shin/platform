# Slice #6 — Audit log viewer

**PRD:** ../prd.md · **kind:** feature · **mode:** hitl

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
