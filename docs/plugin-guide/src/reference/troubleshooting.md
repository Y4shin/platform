# Troubleshooting

A symptom-indexed list of the things that bite plugin authors, with the fix and a
pointer to the chapter that explains why.

## Build and codegen

**`junius check` says `PROTO.REQUIRES.UNDECLARED`.**
A proto method's `(platform.v1.requires)` names a permission that isn't in
`[permissions]`. Declare it, or fix the typo. → [Permissions](../backend/permissions.md)

**`RPC.SERVICE.UNIMPLEMENTED`.**
A proto service has no `#[rpc_service]` block. Run
`cargo run -p junius -- rpc scaffold --plugin <name>` to stub it. → [RPC](../backend/rpc.md)

**`RPC.HANDLER.UNGUARDED`.**
There's a bare `impl XService for Y` in your source. Always go through
`#[rpc_service(XService)]` — never write the trait impl by hand. → [RPC](../backend/rpc.md)

**`RPC.WITNESS.MISMATCH`.**
A handler's ctx-parameter alias names a different method/service than the one
it's implementing (e.g. `create_event` carrying `…::ListEvents`). Point the alias
at this method. → [RPC](../backend/rpc.md)

**The repo compiles but the wrong methods are callable.**
You wrapped a generated `__rpc_requires` alias in `permissions!(…)`. Don't — the
alias is already a built witness; wrapping it breaks `Has<X>` resolution. Use the
alias directly. → [RPC](../backend/rpc.md)

**`use junius_sdk::permissions;` won't compile / collides.**
The name clashes with the generated `permissions` *module*. Call the macro fully
qualified (`junius_sdk::permissions!(…)`) and import markers from
`crate::permissions`. → [Permissions](../backend/permissions.md)

## Database and queries

**CI fails on a stale `.sqlx` cache.**
You changed a compile-time query without regenerating. With Postgres up and
migrations applied:
`DATABASE_URL=… SQLX_OFFLINE=false cargo sqlx prepare --workspace`, then commit
`.sqlx/`. → [Repositories](../backend/repositories.md)

**An enum column comes back as the wrong type / a sqlx type error.**
`SELECT e.*` doesn't carry the enum override. Name the column and annotate it
(`visibility AS "visibility: Visibility"`), and cast enum binds
(`$1::events.visibility`). → [Migrations](../backend/migrations.md)

**`SQL.PRIVATE_TABLE_ACCESS`.**
You queried another plugin's table that isn't exposed. Either it's the wrong
table, or that plugin needs to `[exposes.tables]` it. → [Manifest](../backend/manifest.md)

**`FK.CROSS.CASCADE`.**
A foreign key into another schema uses `ON DELETE CASCADE`. Make cross-schema FKs
nullable and non-cascading; keep cascades inside your own schema. → [Migrations](../backend/migrations.md)

**A read returns `NotFound` for a row I can see in the DB.**
The ACL filtered it out — the caller doesn't own/share it and it isn't public.
That's by design (no existence leak). Check ownership was recorded and the
caller's role grants the permission. → [Ownership](../backend/ownership.md)

**A group member can't see a group-owned resource.**
Membership isn't enough — the member's **role** must grant the permission. → [Ownership](../backend/ownership.md)

## Capabilities

**`CapabilityNotDeclared` at runtime.**
You called `resources.email` / `jobs` / `storage` without the matching
`[requires] capabilities` entry. Declare it. → [Manifest](../backend/manifest.md)

**`STORAGE.BUCKET.UNMAPPED`.**
A declared logical bucket isn't mapped to a physical one in the deployment's
`[config.storage.mapping]`. → [Storage](../capabilities/storage.md)

**A job sends email in the wrong language.**
You didn't capture the recipient's locale at enqueue time. Add a
`recipient_locale` field to the payload and resolve via
`localizer.for_stored(...)`. → [Jobs and email](../capabilities/jobs-and-email.md)

## Frontend

**`FE.EXPORTS.MATCH_MANIFEST`.**
An `[exposes.components]` entry isn't a named export of `frontend/src/index.ts`.
Add the export. → [Exposed components](../frontend/exposed-components.md)

**A typed `<Link to="/p/...">` won't compile.**
The composed route tree erases plugin route types. Use `usePluginNavigate` /
`PluginLink` from `@junius/sdk` for intra-plugin navigation. → [Routes](../frontend/routes.md)

**The "Edit" button shows but the server refuses the edit.**
You gated the UI on a client-side guess instead of the server's `viewerCanEdit`
flag. Render affordances from the server-computed flags. → [Ownership](../backend/ownership.md)

**Strings show as plain English under the pseudo-locale.**
They're unwrapped. Wrap every user-facing string in a Lingui macro (imported from
`@lingui/react/macro` directly). → [Internationalization](../capabilities/i18n.md)

**A Lingui macro isn't being transformed.**
The babel plugin only transforms macros imported from `@lingui/react/macro` — not
re-exports. Import directly. → [Internationalization](../capabilities/i18n.md)

## CI surprises

**Passes locally, fails CI on biome.**
`biome format --write` doesn't sort imports; `biome ci` checks that it's sorted.
Run `biome check --write` on hand-written TS. → [CI gate](../quality/ci-gate.md)

**E2E passes embedded, fails split.**
A server-side `window`/`document` access or a hydration mismatch. → [SSR](../quality/ssr.md)

**`task ci` is green but the PR is red.**
Local green is necessary, not sufficient — remote is the source of truth. Wait on
`gh pr checks --watch`. → [CI gate](../quality/ci-gate.md)

## Still stuck?

- Inspect the plugin: `cargo run -p junius -- plugin info <name>`.
- Read the worked example: every pattern in this book has a real counterpart in
  `plugins/events/`.
- Cross-check the deeper design docs under [`design/`](../../../design/) and the
  per-milestone notes under [`impl/`](../../../impl/). When the code and the book
  disagree, the code wins — please fix the book.
