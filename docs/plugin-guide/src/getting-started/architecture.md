# Architecture in five minutes

Before you write any code, it helps to know how a plugin and the host fit
together. This chapter is the map; the rest of the book fills in the territory.

## Host vs. plugin

```text
platform/                     the HOST — not a plugin
├── src/                      Axum server, the Plugin trait + PluginContext,
│                             capability handles, DB pool, config, telemetry
└── frontend/                 the FE shell (@junius/shell): login, nav, routing

plugins/<name>/               a PLUGIN — one domain use case
├── plugin.toml               the manifest: identity, permissions, capabilities
├── Cargo.toml                the backend crate
├── build.rs                  compile-time codegen (proto → Rust, i18n)
├── src/                      backend: Plugin impl, RPC services, repositories
├── proto/<name>/v1/          the API contract (proto3)
├── migrations/               SQL migrations for the plugin's own schema
└── frontend/                 the frontend package (@junius/plugin-<name>)
    ├── src/                  routes, pages, exposed components
    └── i18n/                 frontend translation catalogs
```

**Auth, users, RBAC, sessions and infrastructure stay in the host** — they are
dependencies of the plugin loader itself. A plugin gets at them through the
`junius-sdk` crate, never by importing from `platform/`.

## The three boundaries

A plugin meets the rest of the system at exactly three seams:

1. **`junius-sdk` (Rust).** Every host capability a plugin can use — the
   database, identity, authorization, audit, email, jobs, storage, telemetry,
   i18n — is a handle on `PluginResources`, handed to your code per request.
   This is the *only* Rust dependency a plugin has on the host.

2. **`plugin.toml` (the manifest).** The single source of truth for the
   plugin's identity, the permissions it defines, the capabilities it requires,
   the components and tables it exposes, and its dependencies. `junius sync`
   and `junius check` both read it; code generation flows from it.

3. **proto (the API contract).** Service and message definitions under
   `proto/<name>/v1/`. `buf generate` turns them into a typed Rust service trait
   (your backend implements it) and a typed TypeScript client (your frontend
   calls it). The proto is also where each RPC method declares the permissions
   it requires.

If you keep those three honest, everything else — wiring, routing, registration
— is generated for you.

## Request lifecycle

When a browser calls one of your plugin's RPC methods:

```text
browser ──POST /rpc/<service>/<method>──▶ host router
                                            │
                            session middleware attaches Extension<User>
                            (pass-through: no session ⇒ no user, never 401)
                                            │
                            RPC guard checks the method's `requires` set
                            against the caller's permissions (pre-dispatch)
                                            │
                            your #[rpc_service] handler runs, with a
                            PluginContext: { state (repositories), user,
                            resources (db, authz, email, jobs, …) }
                                            │
                            repositories run SQL as the plugin's own
                            Postgres role, gated by platform.user_can_access
                                            │
                            response encoded back to the browser
```

Two layers of permission enforcement matter here, and the rest of the book
keeps coming back to them:

- **Runtime gate (the RPC guard).** Before your handler runs, the host checks
  the caller holds every permission the proto method's `(platform.v1.requires)`
  annotation lists. A caller who doesn't is rejected pre-dispatch.
- **Compile-time gate (permission witnesses).** Your handler receives a context
  typed on a *witness* — a zero-sized proof of which permissions were required.
  A repository method that mutates data carries a `Has<EventsWrite>` bound, so a
  context proven to hold only `events:read` **cannot even name** the mutating
  method. The wrong call doesn't fail at runtime; it fails to compile.

Plus a **per-resource ACL**: even with the right permission, a read only returns
rows the caller can actually access (owns, shares, or that are public). That's
the `platform.user_can_access` function you'll see in every query.

## Where the host mounts your plugin

| Surface | Mounted at | Who can reach it |
| --- | --- | --- |
| Connect-RPC services | `/rpc` (one router, keyed by service FQN) | gated per method |
| Plain HTTP routes | `/h/<name>` | as your handlers decide |
| Frontend routes | `/p/<name>` (inside the authed shell) | logged-in users |
| Public frontend routes | `/i/<name>` (outside the shell) | anyone, if `public_routes = true` |

## What the host gives every plugin

Everything below is reachable through `PluginResources` (covered in detail in
the [SDK reference](../reference/sdk.md)):

| Capability | Handle | Gated on |
| --- | --- | --- |
| Database (via repositories) | `db` | plugin-scoped Postgres role |
| Current caller + user lookups | `auth`, `users` | — |
| Groups (membership, roster) | `groups` | — |
| Ownership & sharing | `authz` | ownership checks |
| Audit log | `audit` | — |
| Email | `email` | `email.send` capability |
| Background jobs | `jobs` | `job.enqueue` capability |
| Object storage | `storage` | `storage.read` / `storage.write` |
| Config | `config` | — |
| Secrets | `secrets` | declared in manifest |
| Telemetry | `telemetry` | — |
| i18n | `localizer` | — |

Capability-gated handles (`email`, `jobs`, `storage`, …) only work if your
manifest declares the matching `[requires] capabilities`. Forget the
declaration and the call fails at runtime with a clear error — covered in
[Background jobs and email](../capabilities/jobs-and-email.md).

Now let's get a shell and a running plugin.
