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
