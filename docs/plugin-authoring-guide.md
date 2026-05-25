# Plugin authoring guide

This guide walks through building a real Junius plugin end to end, using the
**`events`** plugin (`plugins/events/`) as the worked example. Events exercises
every platform primitive: per-instance ownership, **user *and* group** principals,
public/private visibility, permission-gated RPC, an **unauthenticated** public
surface, background jobs + email, and token-authed feeds.

If you only read one thing: a plugin is a Rust crate (`plugins/<name>/`) plus an
optional frontend package (`plugins/<name>/frontend/`, `@junius/plugin-<name>`).
The Rust side serves HTTP (`/h/<name>`) and Connect-RPC (`/rpc/<name>`); the
frontend contributes routes (`/p/<name>`) and exposed components. A deployment's
`platform.toml` lists which plugins are enabled, and `junius sync` wires them into
the host.

> Conventions used below: run everything inside `nix develop`; the local gate is
> `task ci`. After editing a manifest or proto, re-run `junius sync` (dev:
> `task sync`). The events plugin is the canonical reference — every snippet below
> has a real counterpart under `plugins/events/`.

---

## 1. Scaffold

```bash
cargo run -p junius -- new plugin myplugin            # full-stack scaffold
cargo run -p junius -- new plugin myplugin --backend-only
```

`junius new` is **scaffold-only**: it writes a starter `plugin.toml`, `Cargo.toml`,
`build.rs`, a `proto/` stub, an initial migration, a repo, `src/lib.rs`, and (unless
`--backend-only`) a `frontend/` package. It does **not** run `sync` and does not need
a `platform.toml` — wiring a plugin into a deployment is `sync`'s job.

Then enable it and wire it in:

```bash
# add "myplugin" to [plugins].enabled in your deployment's platform.toml
cargo run -p junius -- sync --config <deployment>/platform.toml
```

`sync` regenerates the composition glue: the host's plugin registry, the RPC
permission table, the FE route tree + component registry, and each plugin's narrow
`@junius/generated/<plugin>/rpc` namespace.

> **Two manual steps `sync` doesn't yet do** (tracked follow-ups): if your plugin
> adds a proto, register its `proto/` path in the workspace `buf.yaml` and run
> `pnpm exec buf generate`; and ensure the host frontend depends on
> `"@junius/plugin-<name>": "workspace:*"` (run `pnpm install`). The `events` plugin
> is already wired, so copying it sidesteps both.

---

## 2. Declare permissions (manifest → Rust + proto)

`plugin.toml` is the single source of truth. Events declares three permissions:

```toml
[permissions]
"events:read"  = "View events you own, that belong to your groups, or that are public."
"events:write" = "Create, edit, and delete events and configure their invite pages."
"events:share" = "Share a private event with another user or group."
```

`plugin_metadata!()` (invoked once in `src/lib.rs`) generates a zero-sized **marker
type** per permission in a `permissions` module: `events:read → permissions::EventsRead`,
etc. You use these two ways:

- **In Rust**, as a compile-time witness: `EventCtx::<junius_sdk::permissions!(EventsRead & EventsWrite)>::from_rpc(&ctx)?`.
- **In proto**, as the runtime gate: `option (platform.v1.requires) = "events:read,events:write";`.

> **Gotcha:** don't `use junius_sdk::permissions` — the name collides with the
> generated `permissions` *module*. Call the macro fully qualified
> (`junius_sdk::permissions!(...)`) and import the markers from `crate::permissions`.

`junius check` validates that every `requires` string and every marker corresponds
to a declared permission.

---

## 3. Repositories: `#[repository]` + `#[impl_repository]`

A repository is the **only** path to your tables. `#[repository]` injects the DB
handle + caller + audit emitter; `#[impl_repository]` blocks attach `Has<P>` bounds
to each method, so a context proven to hold only `events:read` cannot even *name* a
mutating method (a compile-time gate). See `plugins/events/src/repo/event.rs`.

```rust
#[repository]
pub struct EventRepo<P = ()>;

#[impl_repository(EventRepo)]
impl<P: Has<EventsRead>> EventRepo<P> {
    pub async fn list(&self) -> Result<Vec<EventView>, RepoError> { /* … */ }
}

#[impl_repository(EventRepo)]
impl<P: Has<EventsRead> + Has<EventsWrite>> EventRepo<P> {
    pub async fn create(&self, input: NewEvent, owner: Principal, authz: &Authz)
        -> Result<EventView, RepoError> { /* … */ }
}
```

Repos use **compile-time `sqlx::query!`/`query_as!`** checked against the committed
`.sqlx/` offline cache. After adding or changing a query you must regenerate it
(see §12). Use **runtime `sqlx::query(...)`** for host-schema reads (it skips the
cache) — that's what the SDK's `Groups`/`Users` directory accessors do.

> **Postgres enums** (e.g. `events.visibility`) map to Rust via
> `#[derive(sqlx::Type)] #[sqlx(type_name = "events.visibility", rename_all = "lowercase")]`.
> With `query!` you must name every column and annotate enum columns
> (`visibility AS "visibility: Visibility"`) — `SELECT e.*` won't carry the override —
> and cast enum *binds* (`$7::events.visibility`).

---

## 4. Record ownership for **user *and* group** principals

The platform's per-instance ACL lives in `platform.resource_principal`. Record it
atomically with the INSERT, in the plugin's own transaction, via `authz.record_owner`:

```rust
let mut tx = self.pool().begin().await?;
let row = sqlx::query!("INSERT INTO events.event (...) VALUES (...) RETURNING ...")
    .fetch_one(&mut *tx).await?;
authz.record_owner(&mut tx, "events:event", row.id, owner).await?; // owner: Principal::User | ::Group
tx.commit().await?;
```

`Principal::User(uid)` or `Principal::Group(gid)` — events lets the creator choose
(a personal event vs a group event). **Who may own on a group's behalf is the
*service's* call**, not the repo's: `EventService::create_event` verifies the caller
belongs to the target group via `resources.groups.is_member(group, caller)` before
passing `Principal::Group`.

> **Group-ownership access rule (important + non-obvious):** a group-owned resource
> is visible to a member only if that member's **role grants the permission** (via
> `platform.role_permission`). Membership alone is not enough — `user_can_access`
> step 2 checks the role's permission set. So "Alice is in the Committee" does *not*
> let her see a Committee event unless her role lists `events:read`.

---

## 5. The `visibility OR user_can_access` read pattern + `viewer_can_*`

Every read filters by the central access function, and surfaces the viewer's
server-computed capabilities (the FE must never recompute access):

```sql
SELECT e.*,
  platform.user_can_access('events:event', e.id, $1, 'events:write') AS "viewer_can_edit!",
  platform.user_can_access('events:event', e.id, $1, 'events:share') AS "viewer_can_share!"
FROM events.event e
WHERE e.visibility = 'public'
   OR platform.user_can_access('events:event', e.id, $1, 'events:read')
```

`$1` is the caller id (`self.user().map(|u| u.id.0)`, `None` for anonymous). Public
rows bypass the ACL; private rows require access. A row the caller can't see is
returned as `NotFound`, never distinguished from a missing one (no existence leak).
`update`/`delete` gate the same way in their `WHERE` clause.

---

## 6. A public, unauthenticated handler

The host session middleware is **pass-through** — it attaches `Extension<User>`
when a session cookie is present and otherwise just continues (it never 401s). So a
public surface is simply a handler that doesn't *require* a caller. There are two
shapes; both let the runtime ACL (not a compile-time witness) decide access:

- **Ungated RPC** (`InviteService.GetInvite/Signup/OptOut`): build the **no-permission
  witness** context, `EventCtx::<()>::from_rpc(&ctx)?`. It resolves the *real*
  (maybe-anonymous) caller without a permission check. Put the public read/write
  methods in a **plain `impl<P>` block** (no `#[impl_repository]`, no `Has` bound) so
  they're callable on `Repo<()>` while still using the macro's `pool()/user()/audit()`.
  See `plugins/events/src/repo/invite.rs::get_page_by_slug`.

- **Unauthenticated HTTP** (the `.ics` endpoints): extract `PluginResources` directly
  and build a caller-less repo — `CalendarRepo::<()>::new(resources.db(), None, resources.audit.clone())`
  — resolving against a *subject* (a feed token's owner, a group), never the caller.
  See `plugins/events/src/http.rs`.

In both cases the SQL `visibility = 'public' OR user_can_access(...)` predicate does
the access control: a public invite is reachable by anyone; a private one 404s
unless a logged-in viewer has `events:read` access.

---

## 7. Frontend routes (+ a login-optional public page)

A plugin's frontend exports `buildRoutes(parent)` (and, for a public surface,
`buildPublicRoutes(parent)`) from `src/index.ts`:

```ts
export function buildRoutes(parent: AnyRoute): AnyRoute[] {
  const getParentRoute = () => parent;
  return [
    createRoute({ getParentRoute, path: '/', component: EventsListPage }),
    createRoute({ getParentRoute, path: '/$eventId', component: EventDetailPage }),
    // …
  ];
}
```

`sync` mounts `buildRoutes` under `/p/<name>` (inside the authed shell) and, for
plugins with `public_routes = true` in `plugin.toml`, `buildPublicRoutes` under
`/i/<name>` **outside** the shell. The public invite page renders for logged-out
visitors; `useUser()` is non-null only when a session happens to be present.

Pages call RPC through the narrow generated namespace + Connect-Query:

```ts
import { useQuery, useMutation } from '@connectrpc/connect-query';
import { rpc } from '@junius/generated/events/rpc';
const { data } = useQuery(rpc.EventService.listEvents, {});
```

> **Navigating to your own sub-routes:** the composed route tree erases plugin
> route types (`buildRoutes` returns `AnyRoute`), so a typed `<Link to="/p/events/$eventId">`
> won't compile. Events uses a one-line `usePluginNavigate` escape hatch
> (`plugins/events/frontend/src/nav.ts`).

> **Don't gate UI on client permissions.** `requirePermissions` is a no-op stub
> today; gate edit/delete affordances on the server-computed `viewerCanEdit` flag
> instead (events does).

---

## 8. The forms library (`@junius/design`)

Non-trivial forms use the zod-validated `Form`/`FormField` wrappers over
react-hook-form. One zod schema drives validation *and* the inferred value type:

```ts
const schema = z.object({ title: z.string().min(1, 'Title is required'), /* … */ });
type FormValues = z.infer<typeof schema>;
const Field = createFormField<FormValues>();   // per-field typed `field.value`

<Form schema={schema} defaultValues={…} onSubmit={…}>
  <Field name="title" label="Title">{(field) => <Input {...field} />}</Field>
  <Field name="schedule" label="When">{(f) => <DateRange value={f.value} onChange={f.onChange} />}</Field>
</Form>
```

Use `createFormField<T>()` (not bare `FormField`) for heterogeneous schemas so each
`name` infers its own `field.value` type. Inputs available: `Input`, `Textarea`,
`Select`, `DateTimeInput`, and the `DateRange` start→end + all-day control. A plugin
authoring zod schemas needs `zod` as its own dependency.

See `plugins/events/frontend/src/routes/pages/EventEditPage.tsx`.

---

## 9. Exposed components (`[exposes.components]`)

Declare reusable components in the manifest; `junius sync` registers them in the
host component registry so other plugins can consume them via
`useComponent('events.EventCard')`:

```toml
[exposes.components.EventCard]
module      = "./lib/EventCard"
description = "Compact event summary card."
```

Each declared component **must** be a named export of `frontend/src/index.ts`
(`junius check`'s `FE.EXPORTS.MATCH_MANIFEST` enforces this). Events exposes
`EventCard` + `EventPicker` (`plugins/events/frontend/src/lib/`).

---

## 10. Background jobs + email

A sign-up enqueues a confirmation email through a background job — the cleanest
worked example of jobs + email. Declare the capabilities, define a `Job`, register
a handler, enqueue it:

```toml
[requires]
capabilities = ["email.send", "job.enqueue"]
```

```rust
#[derive(Serialize, Deserialize)]
pub struct SendSignupConfirmation { /* … */ }
impl Job for SendSignupConfirmation { const NAME: &'static str = "events.send_signup_confirmation"; }

async fn handler(job: SendSignupConfirmation, resources: PluginResources) -> Result<(), PluginError> {
    resources.email.send(EmailMessage { to: vec![job.recipient_email], /* … */ }).await
}

// in `impl Plugin`:
fn jobs(&self) -> Vec<JobHandler> {
    vec![JobHandler::new::<SendSignupConfirmation, _, _>(handler)]
}
// in the Signup handler:
let _ = ectx.resources.jobs.enqueue(job).await; // best-effort: don't fail the action
```

`email.send`/`job.enqueue` handles are runtime-gated on the declared capabilities.
Test handlers with no container via `Jobs::disabled(...)` + a capturing `Transport`
(`plugins/events/tests/send_signup_confirmation.rs`).

---

## 11. Token-authed public endpoints (calendar feeds)

The `.ics` subscription feeds are the worked example of a **bearer credential
outside the session**. A feed key is unguessable (256-bit base64url), stored only
as its **SHA-256 hash** (`calendar_token.token_hash` — the secret lives only in the
URL), scoped to one subject, and revocable (`revoked_at`). The unauthenticated
handler hashes the URL key, looks up the active token, and resolves the feed against
the **subject's** live entitlements — so leaving a group or opting out drops events
on the next poll. Minting/publishing a *group* feed additionally requires
`events:write` within that group (checked from the caller's memberships). See
`src/ics.rs`, `src/repo/calendar.rs`, `src/http.rs`.

---

## 12. Regenerate, check, and the CI gate

After changing schema/queries/proto/manifest:

```bash
# 1. proto TS (if you changed a .proto):  pnpm exec buf generate
# 2. composition glue:                    task sync   (junius sync)
# 3. the .sqlx offline cache (if queries changed):
#    spin a Postgres, apply host + plugin migrations, then:
#    DATABASE_URL=… SQLX_OFFLINE=false cargo sqlx prepare --workspace   # commit .sqlx/
# 4. formatting:  cargo fmt · pnpm exec biome check --write <paths> · pnpm exec buf format -w
task ci   # fmt-check · clippy · no-default build · biome ci · buf lint/format · junius check · tests
```

Common `junius check` failures: `PROTO.REQUIRES.UNDECLARED` (a `requires` string
isn't in `[permissions]`), `SQL.PRIVATE_TABLE_ACCESS` (touching another plugin's
unexposed table), `FK.CROSS.CASCADE` (a cross-plugin `ON DELETE CASCADE` — keep
cascades inside your own schema; make cross-schema FKs nullable + non-cascading),
`FE.EXPORTS.MATCH_MANIFEST` (an exposed component isn't exported), and
`STORAGE.BUCKET.UNMAPPED` (a declared bucket isn't mapped in the deployment).

> **`biome format --write` ≠ `biome check --write`.** The CI gate's `biome ci` also
> enforces import sorting (an *assist* action) — run `biome check --write` on
> hand-written TS, not just `format --write`.

---

## Inspecting a plugin

```bash
cargo run -p junius -- plugin info events    # permissions, capabilities, mounts, exposes
cargo run -p junius -- check                 # the full static gate
```

That's the whole loop. Copy `plugins/events/` (or `plugins/hello/` for a minimal
backend) as your starting point, and lean on `task ci` to keep every change honest.
