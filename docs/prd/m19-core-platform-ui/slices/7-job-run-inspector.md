# Slice #7 — Job-run inspector

**PRD:** ../prd.md · **kind:** feature · **mode:** hitl

Full design: [`21-M19-platform-ui.md` §Tier 2.B](../../../impl/21-M19-platform-ui.md#tier-2b--job-run-inspector-padminjobs).

## What to build

End-to-end (admin plugin): an admin browses `platform.job_run` (M10) at `/p/admin/jobs` with
status filters, a per-row drawer, and the captured error. **Read-only** — no retry/cancel in
v1. Reads existing data — no new tables.

- Proto `JobsAdminService` reading `platform.job_run`, guarded `requires = "admin:jobs.read"`;
  status filters (queued / running / succeeded / failed / retrying). Cursor pagination.
- `src/repo/job.rs` + `src/service/jobs.rs`.
- `frontend/src/routes/pages/JobsPage.tsx`: filter + table + per-row drawer showing attempts
  (`attempt_count`, `last_error`), enqueued/started/finished timestamps, and the serialized
  job args.
- New permission `admin:jobs.read` (granted by the `admin` user-role).
  (`admin:jobs.write` is reserved for a future hands-on operator UI — out of scope.)

## Acceptance criteria

- [ ] A `SendSignupConfirmation` job (from an events signup) appears in `/p/admin/jobs` as
      *succeeded*.
- [ ] Stopping mailpit and signing up again surfaces a *failed* / *retrying* run; the drawer
      shows the captured SMTP error and the attempt count.
- [ ] No retry/cancel control is present (read-only viewer).
- [ ] All new strings ship through Lingui.
- [ ] Playwright spec under `plugins/admin/frontend/e2e/`.

## Blocked by

- #6 — reuses the cursor-paginated filter + table + row-drawer scaffold from the audit page.
