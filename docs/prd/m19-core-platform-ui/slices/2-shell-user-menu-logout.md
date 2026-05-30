# Slice #2 — Shell + user menu + logout

**PRD:** ../prd.md · **kind:** feature · **mode:** hitl

Full design: [`21-M19-platform-ui.md` §Tier 1.A](../../../impl/21-M19-platform-ui.md#tier-1a--shell-chrome-user-menu--logout).

## What to build

End-to-end: a signed-in user opens a header dropdown showing their name, sees their email +
a Profile link, and can sign out back to the Authentik login page.

- `@junius/design/Menu` — a reusable dropdown primitive wrapping
  `@radix-ui/react-dropdown-menu` (already a transitive dep; unstyled + a11y-correct). First
  host-side menu primitive, reusable for plugin row-action menus later.
- `<UserMenu>` in the host `Shell`, replacing the static display-name `<span>`: trigger =
  `displayName` + chevron; content = an email label, a Profile link to `/me`, a separator,
  and a **Sign out** item.
- `signOut` helper in `@junius/sdk/auth` (next to `goToLogin`): `POST /api/auth/logout` with
  credentials, then redirect to `/api/auth/login` for a fresh session.
- Move `LocaleSwitcher` **out** of the header (it re-homes onto `/me` in slice #3); the
  header becomes brand + nav + user menu.
- `lucide-react` icons for the chevron + sign-out glyph.

## Acceptance criteria

- [ ] The header shows a dropdown with the user's display name + chevron.
- [ ] Opening it reveals the user's email, a Profile link (to `/me`), and Sign out.
- [ ] Sign out calls `POST /api/auth/logout` and lands on the Authentik login page.
- [ ] `LocaleSwitcher` no longer renders in the header.
- [ ] All new strings ship through the M14 i18n seam (Lingui).
- [ ] Playwright spec under `e2e/cross/` covers login → open menu → sign out.

## Blocked by

- None — can start immediately.

## Test plan

**Test type:** e2e (Playwright) + frontend-unit (vitest) — the repo convention pairs every
`e2e/cross/*.spec.ts` with a deterministic vitest counterpart (cf. `locale.spec.ts` ↔
`I18nProvider.test.tsx`).
**Reasoning:** the load-bearing behaviour is an integration (clear session → leave the authed
app), which only an e2e proves honestly; the `signOut` request-shaping is the one piece with
real risk that's too slow to iterate on in e2e, so it gets a fast unit counterpart.

### e2e — `login → open menu → sign out` (the required round-trip)
- After `loginAs`, the header shows the user's **display name + chevron** (trigger visible).
- Opening the menu reveals the **email**, a **Profile link to `/me`**, and a **Sign out** item.
- The **`LocaleSwitcher` no longer renders in the header**.
- Clicking **Sign out** ends the session: assert we're **bounced out of the authed SPA** (URL
  leaves the app / a follow-up `page.goto('/')` lands on the login flow) — **not** an assertion
  against Authentik's own login DOM (session-seed fixtures deliberately avoid the external IdP).

### frontend-unit — the `signOut` helper (deterministic counterpart)
- `POST`s `/api/auth/logout` **with credentials** (`credentials: 'include'`).
- Then redirects to `/api/auth/login` for a fresh session.
- Error case: a failed logout `fetch` still redirects (no silent dead-end leaving the user
  stuck in a half-authed state) — confirm the intended behaviour during implementation.

*Not separately tested:* `<UserMenu>` render and the `@junius/design/Menu` primitive — the
e2e's open-menu assertions cover the contents, and the primitive is a thin Radix wrapper
(testing it would test the library).

### Test files
- `e2e/cross/user-menu.spec.ts` (gated behind `JUNIUS_E2E`, session-seed login via `loginAs`)
- `packages/sdk/src/auth/signOut.test.ts` (vitest, jsdom — mock `fetch` + `window.location`)

### Run command
`task test:e2e` (round-trip) · `task test:js` (the `signOut` vitest)
