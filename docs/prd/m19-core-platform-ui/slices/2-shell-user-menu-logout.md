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
