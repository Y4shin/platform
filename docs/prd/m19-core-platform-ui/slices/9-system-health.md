---
kind: feature
title: "System health"
slug: system-health
issue: 9
prd: ../prd.md
mode: hitl
---

# Slice #9 — System health

Full design: [`21-M19-platform-ui.md` §Tier 2.D](../../../impl/21-M19-platform-ui.md#tier-2d--system-health-padminhealth).

## What to build

End-to-end (admin plugin): an admin sees a live status panel at `/p/admin/health` — one
green/amber/red row per dependency — polling on mount + every 30s.

- Proto `HealthService.Check(Empty) → HealthReport`, guarded `requires = "admin:health.read"`.
  `HealthReport` carries a `Dep` for postgres / oidc / jobs / email / storage; `Dep` =
  `{ status, detail, optional error }`, status ∈ `ok | degraded | down | not_configured`.
- `src/repo/health.rs` + `src/service/health.rs` factoring the host's boot-time checks into a
  `HostHealth` service queryable on demand. Cheap pings (Postgres `SELECT 1`, S3 `HeadBucket`)
  run each call; OIDC uses the cached last-success timestamp (discovery is best-effort).
- `frontend/src/routes/pages/HealthPage.tsx`: status panel polling on mount + every 30s while
  the page is visible (`document.visibilityState`); no SSE/WebSocket. One row per dependency.
- New permission `admin:health.read`.

## Acceptance criteria

- [ ] With everything up, `/p/admin/health` shows Postgres / OIDC / Jobs / Email / Storage all
      *ok*.
- [ ] Stopping mailpit flips Email to *down* within 30s; restarting it returns Email to *ok*
      on the next poll.
- [ ] All new strings ship through Lingui.
- [ ] Playwright spec under `plugins/admin/frontend/e2e/`.

## Blocked by

- None — can start immediately.
