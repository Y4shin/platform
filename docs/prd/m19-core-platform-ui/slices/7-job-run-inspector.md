---
kind: feature
title: "Job-run inspector"
slug: job-run-inspector
issue: 7
prd: ../prd.md
mode: hitl
---

# Slice #7 — Job-run inspector

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

## Test plan

**Test type:** rust-integration (correctness backbone) + e2e (required UI smoke) — the same deliberate split slice #6 landed on; the two layers catch genuinely different failure modes.
**Reasoning:** Keyset cursor pagination over `meta.job_run` (cursor `(enqueued_at, id)`) and status-filter narrowing are backend SQL best proven against real Postgres (testcontainers) — fast feedback while writing the query, and the equal-`enqueued_at` page-seam tie-break is impractical to force through a browser; the AC-mandated Playwright spec then stays a thin smoke over the UI wiring (status filter narrows the table, row click opens the drawer with attempt + captured error, no retry/cancel control) using seeded rows the backend test can't see.

> **Schema reconciliation (implement-time):** the slice prose follows the M19 design wording, but the real M10 table is `meta.job_run` with statuses `enqueued | running | completed | failed` (no `succeeded`/`retrying`), a single `attempt` INT (not `attempt_count`), an `error` column (not `last_error`), and `payload` JSONB (the "job args"). Derive assertions from the **actual** columns; map the prose's `succeeded`→`completed` and `failed/retrying`→`failed` (+ `attempt > 0`).

### Assertions

**rust-integration** (`JobsAdminService.List` / `repo/job.rs` over a seeded `meta.job_run`, real Postgres):
- **Cursor tie-break:** seed multiple rows with *identical* `enqueued_at`; paging with `limit` < total yields every row **exactly once** — no duplicates, no gaps at the page seam (proves the `(enqueued_at, id)` composite cursor, not a naive `enqueued_at <` predicate).
- **Ordering:** newest-first by `(enqueued_at DESC, id DESC)`, stable across pages; the returned cursor round-trips (next call resumes exactly after the last row).
- **Status filter narrows inside the keyset query** (not post-filtered): filtering `status = 'failed'` (and each of `enqueued`/`running`/`completed`) restricts the rows; a status matching nothing returns an empty page with no cursor.
- **`limit` handling:** absent → default 50; > 200 → clamped to 200; respected as the page size.
- **No `OFFSET`/`LIMIT`-offset pagination** — keyset only.
- **Permission guard:** a call lacking `admin:jobs.read` is rejected (PermissionDenied / Err); a caller holding it (builtin `admin` user-role `'*'`) succeeds.
- **Cursor robustness:** a malformed/garbage opaque cursor yields a clean error (InvalidArgument), not a panic or a silent full scan.
- **Failed-run fields surface:** a seeded `failed` row carries its non-null `error` and `attempt` through the response unchanged (the drawer's data source).

**e2e** (Playwright, admin logged in, `meta.job_run` rows seeded via the `db` helper):
- The page at `/p/admin/jobs` renders the table (job_name, status, enqueued/started/finished timestamps).
- Applying a status filter (e.g. `failed`) narrows the visible rows.
- Clicking a row opens a drawer showing `attempt`, the captured `error`, the timestamps, and the serialized `payload`.
- A seeded `failed` row (status `failed`, non-null `error`, `attempt > 0`) shows the captured error + attempt count in its drawer.
- **No retry/cancel control** is present anywhere on the page or in the drawer (read-only viewer).
- All visible strings are Lingui-sourced (no raw literals).

### Test file
- `plugins/admin/tests/jobs_pg.rs` (rust-integration; testcontainers + host migrations, mirrors slice #6's `audit_pg.rs` — reconcile the exact home at implement time if the job read goes through a host capability rather than a plugin-local repo).
- `plugins/admin/frontend/e2e/jobs.spec.ts` (e2e).

### Run command
`task test:rust` (rust-integration) · `task test:e2e` (Playwright)
