# Local Authentik (dev OIDC provider)

`dev/docker-compose.yml` runs a real [Authentik](https://goauthentik.io/) so
local login mirrors production. CI never uses it — the auth tests seed sessions
directly against Postgres.

## First run

```bash
docker compose -f dev/docker-compose.yml up -d
# Authentik takes ~1–3 minutes to become healthy on first boot.
```

Then export the secrets the deployment config references:

```bash
export OIDC_CLIENT_SECRET=dev-oidc-secret   # must match blueprints/junius.yaml
export SESSION_KEY=dev-session-key
export ROLE_PW_SECRET=dev-role-pw-secret
```

- Admin UI: <http://localhost:9000/if/admin/> — log in as `akadmin` /
  `akadmin-dev` (from `AUTHENTIK_BOOTSTRAP_*`).
- OIDC issuer (matches `dev/platform.toml`):
  `http://localhost:9000/application/o/junius/`.

## Blueprint

`blueprints/junius.yaml` seeds the `junius` application + OIDC provider
(`client_id=platform`, `client_secret=dev-oidc-secret`, redirect
`http://localhost:18080/api/auth/callback`) and the `alice`/`bob` users.

Blueprint schemas drift slightly between Authentik releases (we pin
`ghcr.io/goauthentik/server:2024.12`). If the **worker** logs a blueprint error
on first boot:

1. Check it applied: admin UI → *Customisation → Blueprints* → `junius-dev`.
2. If not, reconcile the model fields against the pinned version's docs and
   re-up the worker.

Set passwords for `alice`/`bob` in the admin UI (or `docker compose exec
authentik-server ak ...`) — blueprints don't carry plaintext passwords.

## Then

```bash
junius dev --config dev/platform.toml
# Visit http://localhost:5173 → redirected to Authentik → log in →
# back to the app; /api/me returns your user.
```
