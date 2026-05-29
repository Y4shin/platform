# Local Authentik (dev OIDC provider)

`dev/docker-compose.yml` runs a real [Authentik](https://goauthentik.io/)
(pinned to `2024.12.5`) so local login mirrors production. CI never uses it — the
auth tests seed sessions directly against Postgres.

## First run

```bash
docker compose -f dev/docker-compose.yml up -d
# Authentik takes ~2–3 minutes to become healthy AND to apply the blueprint on
# first boot. Watch progress:
docker compose -f dev/docker-compose.yml logs -f authentik-server
```

Export the secrets the deployment config references:

```bash
export OIDC_CLIENT_SECRET=dev-oidc-secret   # must match blueprints/junius.yaml
export SESSION_KEY=dev-session-key
export ROLE_PW_SECRET=dev-role-pw-secret
```

- Admin UI: <http://localhost:9000/if/admin/> — `akadmin` / `akadmin-dev`
  (from `AUTHENTIK_BOOTSTRAP_*`).
- OIDC issuer (matches `dev/platform.toml`):
  `http://localhost:9000/application/o/junius/`.

### Verify the blueprint applied

```bash
curl -s http://localhost:9000/application/o/junius/.well-known/openid-configuration | head
```

This must return JSON (HTTP 200). If it 404s, the blueprint hasn't applied yet —
give the worker a minute, or `docker compose ... restart authentik-worker`. To
see a validation error, check the instance status:
`docker compose ... exec authentik-postgresql psql -U authentik -d authentik -tAc "SELECT name, status FROM authentik_blueprints_blueprintinstance WHERE name='junius-dev';"`

## Blueprint

`blueprints/junius.yaml` seeds, automatically on boot:

- the `junius` application + OIDC provider (`client_id=platform`,
  `client_secret=dev-oidc-secret`, two redirect URIs — `http://localhost:5173/api/auth/callback`
  for `junius dev` (Vite origin) and `http://localhost:18080/api/auth/callback`
  for the embedded SPA — openid/email/profile scopes, the default self-signed
  signing key);
- a custom **`groups` scope mapping** (M18) that emits the caller's group names
  as a `groups` claim, so the host's OIDC reconciler can map Authentik groups
  onto Junius `(group, role)` pairs;
- two dev groups, `junius-organisers` (alice) and `junius-members` (bob), to
  exercise that mapping out of the box;
- the `alice` / `bob` test users.

It's written for the pinned `2024.12.x` schema. Notes if you bump the image:

- `redirect_uris` is a list of `{matching_mode, url}` objects (not a string).
- the provider requires `invalidation_flow` and `signing_key`.
- `!KeyOf <id>` resolves an entry's `id:` field — the provider entry sets
  `id: junius-provider` so the application can reference it.
- user emails must be valid addresses (Django rejects single-label domains like
  `@local`), so the test users use `@example.com`.

## Set test-user passwords

Blueprints don't carry plaintext passwords. Set one for each (then log in with
the **username** `alice` / `bob`):

- Admin UI → **Directory → Users** → user → *Set password*, or
- `docker compose -f dev/docker-compose.yml exec authentik-server \`
  `ak shell -c "from authentik.core.models import User; u=User.objects.get(username='alice'); u.set_password('alice'); u.save()"`

## Seed groups, roles & permissions (provisioning)

The platform's initial groups/roles/permissions ship as checked-in TOML, not a
hand-run SQL script (M18 replaced the old `dev-seed.sql`). After migrating,
apply the dev provisioning once:

```bash
junius provision apply --config dev/platform.toml
```

This reconciles `dev/provisioning.toml` (the `Organisers` group + `organiser`
role, the `admin` user-role assignment for alice, and the OIDC group mapping)
and is idempotent — re-running is a no-op. `junius dev` / `juniusd` also apply
it automatically on boot (`[provisioning] auto_apply_on_boot`).

To force an OIDC-group re-sync for a user without waiting for their next login
(e.g. after changing their Authentik groups), call the running host's admin
sweep — set `[config] admin_api_token` in `dev/platform.toml` first:

```bash
junius oidc resync --user alice@example.com --config dev/platform.toml
junius oidc resync --all --config dev/platform.toml
```

## Then

Two ways to run, both single-origin (the browser only ever talks to one host;
cookies and the OIDC callback land on that same origin):

**`junius dev` (Vite, hot reload) — the primary local loop:**

```bash
junius dev --config dev/platform.toml
# Runs migrations, starts juniusd (:18080), and Vite (:5173). Vite proxies
# /api → juniusd, and dev/platform.toml's oidc_redirect_url points the callback
# at :5173, so the whole login round-trip stays on :5173.
# Visit http://localhost:5173 → redirected to Authentik → log in as alice →
# back to the app; /api/me returns your user.
```

**Embedded SPA (production-shaped, no Vite):**

```bash
junius migrate up --config dev/platform.toml
junius build --config dev/platform.toml          # embed the SPA into juniusd
JUNIUS_CONFIG=dev/platform.toml ./target/release/juniusd
# Visit http://localhost:18080 → log in → back to the app.
# (juniusd derives the :18080 callback from bind_addr when oidc_redirect_url is
# unset; here dev/platform.toml sets :5173, which is also a registered redirect,
# so the embedded SPA bounces through :5173 on the callback — harmless in dev.
# For a pure :18080 run, comment out oidc_redirect_url in dev/platform.toml.)
```
