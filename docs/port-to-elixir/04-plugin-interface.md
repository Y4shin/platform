# 04 — The Plugin Interface & How Rust Shapes It

> Part of the [Junius → Elixir/Phoenix report](README.md). As-is analysis, with concrete
> interfaces quoted from source. **This is the most important file for the port** — the plugin
> contract is where the choice of language bites hardest.

## 1. Anatomy of a plugin

A plugin is one directory containing a Rust crate + a frontend package + declarative manifests:

```
plugins/events/
├── plugin.toml          # THE manifest — the single source of truth for composition
├── Cargo.toml           # Rust crate
├── build.rs             # runs connectrpc-build + junius-rpc-meta + junius-i18n-build codegen
├── src/
│   ├── lib.rs           # Plugin impl + RPC handlers + per-request state
│   ├── domain.rs        # domain types
│   ├── repo/            # typed repositories (the only place SQL can run)
│   ├── http.rs, ics.rs  # plain-HTTP handlers
├── proto/events/v1/*.proto   # Connect-RPC service + message definitions
├── migrations/*.up.sql       # up-only SQL migrations
└── frontend/
    ├── package.json     # @junius/plugin-events
    ├── i18n/*.po        # Lingui catalogs
    └── src/index.ts     # public exports: buildRoutes, buildPublicRoutes, components, loadI18n
```

## 2. The manifest (`plugin.toml`)

The manifest is the **only** thing `junius` reads to compose a plugin. Its schema is a strict
serde struct (`crates/manifest/src/plugin.rs`, `#[serde(deny_unknown_fields)]` throughout):

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub plugin: PluginIdentity,                              // name, display_name, manifest_schema, public_routes
    #[serde(default)] pub mount: PluginMount,                // route_prefix / rpc_prefix / http_prefix overrides
    #[serde(default)] pub dependencies: BTreeMap<String, PluginDep>,   // optional, tables, rpc_methods
    #[serde(default)] pub exposes: PluginExposes,            // components{module,description}, tables{schema}
    #[serde(default)] pub permissions: BTreeMap<String, String>,       // "events:read" => "description"
    #[serde(default)] pub requires: PluginRequires,          // capabilities = [...]
    #[serde(default)] pub config: BTreeMap<String, ConfigField>,       // typed config schema
    #[serde(default)] pub secrets: BTreeMap<String, SecretDecl>,
    #[serde(default)] pub storage: PluginStorageDecl,        // logical buckets
}
```

Mount prefixes default from the name (`with_defaults`): `/p/<name>` (frontend routes),
`/rpc/<name>`, `/h/<name>`. A concrete manifest (`plugins/events/plugin.toml`, abbreviated):

```toml
[plugin]
name = "events"
display_name = "Events"
manifest_schema = 1
public_routes = true          # mount buildPublicRoutes at /i/events (login-optional)

[permissions]
"events:read"  = "View events you own, that belong to your groups, or that are public."
"events:write" = "Create, edit, and delete events and configure their invite pages."
"events:share" = "Share a private event with another user or group."

[exposes.components.EventCard]
module = "./lib/EventCard"
description = "Compact event summary card (title, when, where, visibility)."

[exposes.tables.event]
schema = "events"

[requires]
capabilities = ["email.send", "job.enqueue"]
```

The `admin` plugin declares the trusted capability that only it may hold
(`plugins/admin/plugin.toml`): `capabilities = ["platform.admin"]`. The host's allowlist refuses
to boot if any other plugin declares it.

## 3. The `Plugin` trait

Every plugin implements this once. It is object-safe (via `async_trait`) so the host holds
`Vec<Box<dyn Plugin>>`. Verbatim (`crates/junius-sdk/src/plugin.rs`):

```rust
#[async_trait::async_trait]
pub trait Plugin: Send + Sync + 'static {
    /// `'static` view of the manifest, produced by the `plugin_metadata!()` macro.
    fn metadata(&self) -> &'static PluginMetadata;

    /// The plugin's plain-HTTP routes (unprefixed; host nests under /h/<plugin>).
    fn routes(&self) -> Router;

    /// Register Connect-RPC services into the shared router. Default: none.
    fn register_rpc(&self, router: connectrpc::Router) -> connectrpc::Router { router }

    async fn on_startup(&self, _resources: &PluginResources) -> Result<(), PluginError> { Ok(()) }
    async fn on_shutdown(&self, _resources: &PluginResources) -> Result<(), PluginError> { Ok(()) }

    /// Background-job handlers. Default: none.
    fn jobs(&self) -> Vec<JobHandler> { Vec::new() }

    /// Install the plugin's i18n catalog. Default: no-op.
    fn register_i18n(&self, _builder: &mut LocalizerBuilder) {}
}
```

The events plugin's impl (`plugins/events/src/lib.rs`) is small — it delegates to macro-generated
and hand-written pieces:

```rust
#[async_trait]
impl Plugin for EventsPlugin {
    fn metadata(&self) -> &'static PluginMetadata { &METADATA }
    fn routes(&self) -> Router {
        Router::new().route("/ping", get(|| async { "pong" })).merge(http::routes())
    }
    fn register_rpc(&self, router: connectrpc::Router) -> connectrpc::Router {
        let router = Arc::new(EventRpc).register(router);
        let router = Arc::new(InviteRpc).register(router);
        Arc::new(CalendarRpc).register(router)
    }
    fn jobs(&self) -> Vec<JobHandler> {
        vec![JobHandler::new::<SendSignupConfirmation, _, _>(send_signup_confirmation_handler)]
    }
    fn register_i18n(&self, builder: &mut junius_sdk::LocalizerBuilder) { catalog::register(builder); }
}
```

## 4. Host-provided resources

The host builds one **`PluginResourceCtx`** per plugin at boot and, per request, a
**`PluginResources`** bundle (ctx + the session-resolved `User`). This is the plugin's entire
surface onto host services. Verbatim (`crates/junius-sdk/src/resources.rs`):

```rust
#[derive(Clone)]
pub struct PluginResources {
    pub config: PluginConfig,
    pub telemetry: Telemetry,
    pub(crate) db: PluginDb,            // opaque — no query API in the public type
    pub auth: Auth,                     // caller identity + user-directory lookups
    pub users: Users,
    pub groups: Groups,                 // resolve by name/id, enumerate members
    pub audit: AuditEmitter,
    pub authz: Authz,                   // per-resource ownership + sharing
    pub email: Email,                   // gated on `email.send`
    pub jobs: Jobs,                     // gated on `job.enqueue`
    pub storage: PluginStorage,         // gated on `storage.read`/`storage.write`
    pub platform_admin: PlatformAdminApi, // gated on `platform.admin`
    pub localizer: Localizer,
    pub(crate) secrets: SecretStore,
    pub(crate) capabilities: &'static [&'static str],
}
```

**Capability gating is runtime** and lives right here (`resources.rs`):

```rust
pub(crate) fn require_capability(caps: &[&'static str], needed: &'static str)
    -> Result<(), PluginError>
{
    if caps.contains(&needed) { Ok(()) }
    else { Err(PluginError::CapabilityNotDeclared(needed)) }
}
```

Note the deliberate constructor-less struct (`PluginResourceCtx`, all `pub` fields, no
constructor): adding a host handle becomes a *compile error* at every construction site. That is
a Rust-specific guardrail; the port replaces it with a plain struct + a test.

## 5. The type-level permission system (the deepest Rust coupling)

This is the mechanism with **no Elixir equivalent**, so it's worth understanding precisely.
Verbatim from `crates/junius-sdk/src/permissions.rs`:

```rust
/// A declared permission — one zero-sized marker type per manifest [permissions] key,
/// generated by plugin_metadata!(). Referencing an undeclared permission is a COMPILE ERROR
/// because the marker type doesn't exist.
pub trait Permission: Send + Sync + 'static { const NAME: &'static str; }

/// Type-level conjunction. A witness is a right-nested chain terminated by ():
/// permissions!(A & B)  =>  And<A, And<B, ()>>
pub struct And<A, B>(PhantomData<(A, B)>);

pub struct Here;              // index witness: permission is at the chain head
pub struct There<T>(PhantomData<T>);   // …somewhere in the tail

/// `Self` proves it holds permission X. Idx records WHERE in the chain, so head and tail
/// impls don't overlap (the classic HList-membership encoding). Sealed.
pub trait Has<X, Idx>: sealed::Sealed {}
impl<X: Permission, Tail> Has<X, Here> for And<X, Tail> {}
impl<Head, Tail, X, I> Has<X, There<I>> for And<Head, Tail> where Tail: Has<X, I> {}

/// The RUNTIME half: enumerate a witness's names for the extractor's caller check.
pub trait PermissionList { fn names() -> Vec<&'static str>; }
impl PermissionList for () { fn names() -> Vec<&'static str> { Vec::new() } }
impl<Head: Permission, Tail: PermissionList> PermissionList for And<Head, Tail> {
    fn names() -> Vec<&'static str> { let mut n = Tail::names(); n.insert(0, Head::NAME); n }
}
```

**Read this twice — it is the crux of the port.** There are two consumers of a permission
witness `P`:

1. **Compile-time gating** via `Has<X>`. A repository method written
   `impl<P: Has<EventsWrite>> EventRepo<P>` can be *named* only when `P` contains `EventsWrite`.
   A context proven to hold only `events:read` literally cannot call a mutating method — it's a
   type error, caught by the compiler.
2. **Runtime checking** via `PermissionList::names()`, which the request extractor iterates to
   verify the caller actually holds each permission.

The port keeps **(2)** and drops **(1)** — there is no way to make "unauthorized code won't
compile" work in Elixir. See §9.

## 6. Repositories — SQL confinement + permission-gated methods

Data access **must** go through a repository. `PluginDb` exposes no executor, so `sqlx::query!`
cannot compile outside a `#[repository]` type's macro-generated impl blocks. Permission gating
rides on the `Has<X>` bound. From `plugins/events/src/repo/event.rs`:

```rust
#[repository]
pub struct EventRepo<P = ()>;

#[impl_repository(EventRepo)]
impl<P: Has<EventsRead>> EventRepo<P> {
    pub async fn list(&self) -> Result<Vec<EventView>, RepoError> { /* SELECT … user_can_access … */ }
    pub async fn get(&self, id: EventId) -> Result<EventView, RepoError> { /* … */ }
}

#[impl_repository(EventRepo)]
impl<P: Has<EventsRead> + Has<EventsWrite>> EventRepo<P> {
    pub async fn create(&self, input: NewEvent, owner: Principal, authz: &Authz)
        -> Result<EventView, RepoError>
    {
        let mut tx = self.pool().begin().await?;
        let r = sqlx::query!(r#"INSERT INTO events.event (…) VALUES (…) RETURNING …"#, /* … */)
            .fetch_one(&mut *tx).await?;
        authz.record_owner(&mut tx, "events:event", r.id, owner).await?;   // Layer-2 ownership
        tx.commit().await?;
        self.audit().emit("events:event.create", /* … */).await?;
        Ok(/* EventView with viewer_can_edit: true */)
    }
    pub async fn update(&self, id: EventId, input: EventUpdate) -> Result<EventView, RepoError> {
        // UPDATE … WHERE user_can_access('events:event', e.id, $9, 'events:write')  — Layer 2
    }
}
```

The `create`/`update` methods are on the `Has<EventsWrite>` impl block, so a `read`-only context
cannot even name them. Every read/write also joins `platform.user_can_access` — the two layers
work together.

## 7. Per-request state & context assembly

Each plugin defines a state struct of its repositories, tagged on `P`, and derives the extractor.
From `plugins/events/src/lib.rs`:

```rust
#[derive(PluginCtx)]
pub struct EventState<P = ()> {
    #[repo] pub events:   EventRepo<P>,
    #[repo] pub invites:  InviteRepo<P>,
    #[repo] pub signups:  SignupRepo<P>,
    #[repo] pub calendar: CalendarRepo<P>,
}
pub type EventCtx<P = ()> = PluginContext<EventState<P>, P>;
```

`PluginContext<S, P>` is the value a handler receives (`crates/junius-sdk/src/context.rs`):

```rust
pub struct PluginContext<S, P = ()> {
    pub state: S,                    // the repositories
    pub user: Option<User>,
    pub resources: PluginResources,
    _phantom: PhantomData<P>,
}
```

It is built by a blanket `FromRequestParts` impl (HTTP) or `from_rpc` (Connect); both run
`check_perms` against `P::names()` — the runtime half of the permission system
(`crates/junius-sdk/src/context.rs`):

```rust
pub(crate) fn check_perms(user: Option<&User>, names: &[&str]) -> Result<(), ApiError> {
    for &name in names {
        match user {
            Some(u) if u.has_permission(name) => {}
            Some(_) => return Err(ApiError::forbidden(name)),
            None    => return Err(ApiError::unauthenticated()),
        }
    }
    Ok(())
}
```

`system_context::<S, P>(resources)` builds a caller-less, privileged context for background jobs.

## 8. RPC handlers — proto annotation → witness → macro-rewritten trait impl

Permission requirements are declared **in the proto**, as a method option
(`proto/platform/v1/annotations.proto`):

```proto
extend google.protobuf.MethodOptions {
  optional string requires = 60001;   // e.g. "events:read,events:write"
}
```

At build time, `junius-rpc-meta` turns each method's `(platform.v1.requires)` into a per-method
**witness type alias** under `crate::__rpc_requires::<service>::<Method>`. The `#[rpc_service]`
macro then rewrites an inherent impl into the Connect trait impl, injecting `from_rpc` +
permission resolution. From `plugins/events/src/lib.rs`:

```rust
#[junius_sdk::rpc_service(EventService)]
impl EventRpc {
    async fn list_events(
        &self,
        ectx: EventCtx<crate::__rpc_requires::event_service::ListEvents>,  // witness from proto
        _request: OwnedListEventsRequestView,
    ) -> ServiceResult<impl Encodable<pb::ListEventsResponse>> {
        let events = ectx.state.events.list().await?;
        Ok(Response::new(pb::ListEventsResponse { events: /* … */, ..Default::default() }))
    }
    // create_event, get_event, … each name their own __rpc_requires alias
}
```

So there are **three** places the same permission is threaded, all from one proto annotation: the
host guard (`RPC_REQUIRES`, pre-dispatch), the handler's witness type (compile-time), and the
runtime `from_rpc` check. An **ungated** method (no annotation) gets the alias `()`, so anonymous
callers reach it and the SQL ACL alone decides — this is how the public invite/ICS surfaces work.

## 9. How each mechanism is tied to Rust — and its runtime counterpart

This table is the single most important input to [05](05-elixir-target-architecture.md).

| Mechanism | Rust implementation | Compile-time guarantee | Elixir counterpart |
|---|---|---|---|
| **Permissions** | `Has<X>` HList witnesses (`permissions.rs`) | A read-only ctx *cannot name* a write method | **Runtime check** — `check_perms` / `User.has_permission?` already exist as the runtime half |
| **Manifest → code** | `plugin_metadata!()` emits marker types, typed `Config`, `Secrets`, `buckets` enum, `static METADATA` | Undeclared permission/secret/bucket = compile error | **Runtime manifest parse + boot-time validation** (fail fast) |
| **RPC permission** | proto `requires` → witness alias → `#[rpc_service]` rewrite + host guard | Handler witness pinned to proto set | **Gone** — no proto layer; a plug / LiveView `on_mount` reads the declared permission and checks at runtime |
| **SQL confinement** | opaque `PluginDb`; SQL only inside `#[repository]` macros | `sqlx::query!` can't compile elsewhere | **Convention + Ecto contexts + Credo/tests** (not a compiler guarantee) |
| **Resource assembly** | constructor-less `PluginResourceCtx`, blanket `FromRequestParts` | Missing host handle = compile error | Plain struct + a test |
| **Plugin registry** | `Vec<Box<dyn Plugin>>`, generated at build time | — | **Behaviour + runtime registry** (ports cleanly) |

**The key insight, stated plainly:** the codebase already carries the *runtime half* of every
compile-time mechanism — `PermissionList::names()` + `check_perms`, `User::has_permission`, the
runtime `require_capability` gate, and the SQL `user_can_access` ACL. The Rust type system is a
*second, static* enforcement layer stacked on top. **The Elixir port keeps the runtime layer and
drops the static layer**, trading a compile-time guarantee for large simplicity and re-adding
safety via runtime checks, boot-time manifest validation, and tests/Dialyzer/Credo.

## 10. What ports cleanly vs. what is Rust-specific

**Ports cleanly (language-neutral):**
- The `plugin.toml` schema and its validation semantics (`crates/manifest`).
- The permission/capability *model* (strings, groups, roles, the two layers).
- The migration model (up-only, `@requires` DAG, per-plugin schema/role, manifest-driven grants).
- The host SQL functions (`user_can_access`, `record_owner`, `forget_resource`) — verbatim.
- The host-services model (db / storage / jobs / email / secrets / audit / i18n / users / groups
  / authz as injected, capability-gated handles).
- The frontend composition *idea* (each plugin contributes routes + components).

**Rust-specific (replaced with runtime mechanisms):**
- The type-level permission witnesses and `Has<X>` gating.
- Compile-time manifest→type generation.
- The proto/Connect RPC pipeline and its codegen.
- SQL confinement by type hiding.
- Compile-time static plugin linking (`junius sync` → `plugins.rs`).

Continue to [05 — Elixir target architecture](05-elixir-target-architecture.md).
