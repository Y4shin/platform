# Slice #3 — `/me` profile page

**PRD:** ../prd.md · **kind:** feature · **mode:** hitl

Full design: [`21-M19-platform-ui.md` §Tier 1.C](../../../impl/21-M19-platform-ui.md#tier-1c--me-profile-page).

## What to build

End-to-end: a user opens `/me`, sees their identity + groups + roles + sessions, changes
their language, and revokes an old session (forcing that browser to re-login).

- New host route `/me` under `authedLayoutRoute`. Lives entirely in the host frontend (it
  queries `/api/me` + `/api/sessions/*`); no plugin code.
- Extend `GET /api/me` with `oidcSub` + a `sessions` array (id, user_agent, last_seen).
- New `DELETE /api/sessions/<id>` — revokes a session. The **current** session is protected
  from revoke (it signs out via the user menu) to avoid accidental self-lockout.
- `POST /api/me/locale` already exists (M14).
- Page sections:
  - **Identity** — display name, email, OIDC subject (all read-only).
  - **Preferences** — `LocaleSwitcher` re-homed here (saves on change).
  - **Group memberships** — table of group, role, permission count, `managed_by` badge
    (manual / OIDC) per M18.
  - **User-roles** — table of user-role assignments (e.g. `admin` wildcard).
  - **Sessions** — table; current session has no Revoke; others have a Revoke control.

## Acceptance criteria

- [ ] `/me` renders identity (display name, email, OIDC sub) read-only.
- [ ] The locale switcher updates `user.locale` (visible in `/api/me` after `POST /api/me/locale`).
- [ ] The memberships table lists the user's groups with `managed_by` badges.
- [ ] The user-roles section lists assignments (e.g. `admin`).
- [ ] The sessions table lists the current session (no Revoke) plus others; revoking a
      non-current session forces that browser to re-login on its next request.
- [ ] All new strings ship through Lingui.
- [ ] Playwright spec under `e2e/cross/`.

## Blocked by

- #2 — the `LocaleSwitcher` is removed from the header in #2 and re-homed here.

## Test plan

**Test type:** e2e (spine) + rust-integration (backend auth gate)
**Reasoning:** The headline behaviour — revoke a non-current session → that browser is
forced to re-login on its next request — is only honestly provable end-to-end (cookie →
session middleware → DB → re-login), and the acceptance criteria mandate a Playwright spec
under `e2e/cross/`; the current-session self-lockout guard is a pure backend authorization
gate that is far cheaper and more precise to pin as a `rust-integration` test than through a
browser.

### Schema note (precondition for the assertions)

The slice's `sessions[]` shape `(id, user_agent, last_seen)` requires columns that
`platform.session` does **not** have yet (today: `id, user_id, created_at, expires_at,
oidc_tokens`). A new migration adds `user_agent` + `last_seen`, and the session middleware
stamps `last_seen` on each validated request. `oidcSub` lives on `platform.user` but is not
yet surfaced in the `/api/me` `User` struct — the query + struct both extend.

### Assertions

**rust-integration — `GET /api/me` + `DELETE /api/sessions/<id>`** (testcontainers Postgres,
seeded sessions, no live IdP — mirrors `auth_pg.rs`):

- `GET /api/me` returns `oidcSub` (the seeded `platform.user.oidc_sub`) and a `sessions`
  array whose entries carry `id`, `user_agent`, `last_seen`; the entry for the request's own
  session is identifiable (so the FE can suppress its Revoke control).
- `DELETE /api/sessions/<own-non-current-id>` → **204 No Content**; the row is gone
  (`SELECT … WHERE id = $1` returns nothing) → a request bearing that cookie now loads no
  `User` (forced re-login).
- `DELETE /api/sessions/<own-current-id>` → **403 Forbidden**; the current session row
  **survives** (self-lockout guard).
- `DELETE /api/sessions/<other-users-id>` and `DELETE /api/sessions/<nonexistent-id>` →
  **404 Not Found**; no row is touched (don't leak existence of other users' sessions).
- `DELETE /api/sessions/<id>` with **no/invalid session cookie** → **401 Unauthorized**.

**e2e — `/me` profile journey** (Playwright, `loginAs` fixture + `insertSession` seed):

- `/me` renders Identity (display name, email, OIDC sub) read-only.
- Changing the locale switcher persists `user.locale` (re-read via `/api/me` shows the new
  value after `POST /api/me/locale`).
- The memberships table lists the seeded group(s) with role, permission count, and a
  `managed_by` badge (manual / OIDC).
- The user-roles section lists assignments (e.g. `admin`).
- The sessions table lists the current session **without** a Revoke control and at least one
  other session **with** one; clicking Revoke on the other removes its row, and a browser
  carrying that revoked session cookie is bounced to login on its next navigation.
- New strings render under a non-`en` locale (pseudo/de) — i.e. they ship through Lingui.

### Test files

- `platform/tests/sessions_pg.rs` (new; sibling of `platform/tests/auth_pg.rs`)
- `e2e/cross/me-profile.spec.ts` (new)

### Run command

`task test:rust` (rust-integration) · `task test:e2e` (Playwright)
