# 11. Backend Plugin Interface

This section defines the Rust API surface a plugin author writes against. It lives in the `junius-sdk` crate ([05-repository-and-deployment-layout.md](05-repository-and-deployment-layout.md) §5.1) and is consumed by every plugin crate. Cross-references: capabilities and DB access in [10-infrastructure-and-data.md](10-infrastructure-and-data.md); manifest schema in [06-plugin-shape.md](06-plugin-shape.md); cross-plugin composition in [08-cross-plugin-composition.md](08-cross-plugin-composition.md).

## 11.1 The `Plugin` trait

Every plugin crate implements `Plugin` once on a top-level struct.

```rust
#[async_trait]
pub trait Plugin: Send + Sync + 'static {
    /// Static metadata, produced by the plugin_metadata! macro from plugin.toml.
    fn metadata(&self) -> &'static PluginMetadata;

    /// Receive the raw, pre-scoped resources for this plugin (Postgres pool
    /// configured for the plugin's role, object storage scoped to the plugin's
    /// bucket prefix, etc.) and return an Axum Router. The plugin sets up its
    /// Axum state internally — typically by passing `resources` directly via
    /// `.with_state(resources)` and using a plugin-defined `#[derive(PluginCtx)]`
    /// context type as the per-request extractor (§11.6).
    /// The host nests the returned router under the plugin's prefixes.
    fn routes(&self, resources: PluginResources) -> Router;

    /// Lifecycle: after migrations, before traffic. Default: no-op.
    async fn on_startup(&self, _resources: &PluginResources) -> Result<(), PluginError> {
        Ok(())
    }

    /// Lifecycle: when the server begins graceful shutdown. Default: no-op.
    async fn on_shutdown(&self, _resources: &PluginResources) -> Result<(), PluginError> {
        Ok(())
    }

    /// Background workers to register with the job queue. Default: none.
    fn jobs(&self) -> Vec<JobHandler> {
        Vec::new()
    }
}
```

**Convention**: every plugin exposes `pub fn new(...) -> Self`. The constructor may take arguments (eager state, host-injected configuration); `junius`-generated glue calls it. Object-safe via `async_trait` so the host can hold `Vec<Box<dyn Plugin>>` — the per-plugin context type is not on the trait, so generics over it don't break object safety.

## 11.2 Metadata via macro (not source-tree codegen)

The plugin's `lib.rs` invokes a proc-macro that reads the crate's `plugin.toml` at compile time and expands to the static metadata plus typed permission markers:

```rust
// plugins/speakers/src/lib.rs

junius_sdk::plugin_metadata!();
// expands to (conceptually):
//   pub static METADATA: PluginMetadata = PluginMetadata { name: "speakers", ... };
//   pub mod permissions {
//       pub struct SpeakersRead;  impl Permission for SpeakersRead  { const NAME: &str = "speakers:read"; }
//       pub struct SpeakersWrite; impl Permission for SpeakersWrite { const NAME: &str = "speakers:write"; }
//       pub struct SpeakersBook;  impl Permission for SpeakersBook  { const NAME: &str = "speakers:book"; }
//   }

pub struct SpeakersPlugin { /* ... */ }

impl SpeakersPlugin {
    pub fn new() -> Self { Self { /* ... */ } }
}

#[async_trait]
impl Plugin for SpeakersPlugin {
    fn metadata(&self) -> &'static PluginMetadata { &METADATA }
    fn routes(&self, resources: PluginResources) -> Router { /* see §11.7 */ }
}
```

**Why a macro, not codegen into source:**
- No generated `.rs` files in plugin source trees.
- `plugin.toml` edits propagate at the next `cargo build` automatically.
- The macro is the single mapping from manifest → type system: permission types declared in the manifest **are exactly** the types available in code. Referencing an undeclared permission is a compile error — no separate `junius check` pass for permission name validity.

The macro lives in a `junius-sdk-macros` proc-macro crate.

**`junius check` enforcement**: scans every plugin's crate root and verifies that `plugin_metadata!()` is invoked exactly once. Missing or duplicate invocations fail with a clear error.

## 11.3 `PluginResources` and `PluginContext<S, P>`

The host hands every plugin a `PluginResources` bundle — the raw, pre-scoped primitives it needs. `PluginContext<S, P>` is a generic type in `junius-sdk` that plugins parameterize with their own per-request state `S` (typically a struct of typed repositories) and a permission witness `P`.

```rust
// In junius-sdk:

/// Raw, pre-scoped resources. The DB pool is configured for the plugin's
/// Postgres role; storage for its bucket prefix; telemetry pre-tagged with
/// `plugin = "<name>"`. Plugins receive this in `Plugin::routes` and lifecycle
/// hooks and pass it through to their own Axum state.
#[derive(Clone)]
pub struct PluginResources {
    pub(crate) db: PluginDb,            // opaque; data access via Repository only
    pub(crate) storage: PluginStorage,  // opaque; file access via Bucket only
    pub jobs: Jobs,
    pub email: Email,
    pub config: PluginConfig,
    pub telemetry: Telemetry,
    pub auth: Auth,
    pub users: Users,
}

/// Generic per-request context. Plugins parameterize S (their own state, often
/// a bundle of typed repositories) and P (permission witness — what permissions
/// the current request has been proven to hold).
#[derive(Clone)]
pub struct PluginContext<S, P = ()>
where
    S: Clone + Send + Sync + 'static,
{
    pub state: S,                    // plugin-defined: repos, caches, audit context, ...
    pub user: Option<User>,
    pub resources: PluginResources,  // host-provided primitives, always available
    _phantom: PhantomData<P>,
}
```

Note what's still hidden:
- **`PluginDb` is opaque** — not in the public API of `junius-sdk` in a queryable form. The only way to use it is via a `#[derive(Repository)]` type that internally owns a `ScopedDb` derived from it (§11.5).
- **`PluginStorage` is opaque** the same way — only usable via a `#[derive(Bucket)]` type.

This means the repository pattern enforcement from §11.5 survives unchanged: `sqlx::query!()` cannot compile outside a repository, regardless of how the plugin constructs its context.

What's freely accessible on `resources`:
- `jobs`, `email`, `config`, `telemetry`, `auth`, `users` — all the non-DB / non-storage handles. Capability gating is **runtime**: calling `resources.email.send(...)` without declaring `email.send` returns `Err(PluginError::CapabilityNotDeclared("email.send"))`.

Type aliases per plugin make handler signatures readable:

```rust
// In each plugin:
pub type SpeakersCtx<P = ()> = PluginContext<SpeakersState<P>, P>;
```

Examples:
- `SpeakersCtx<()>` — no permissions proven; used in lifecycle hooks or system contexts.
- `SpeakersCtx<SpeakersRead>` — read permission proven.
- `SpeakersCtx<permissions!(SpeakersRead & SpeakersWrite)>` — both proven.

`PluginContext` is `Clone` (resources and state are `Arc`-wrapped internally) so plugin services and RPC handlers can take owned copies without lifetime gymnastics.

## 11.4 Typed permission system

```rust
// In junius-sdk:

pub trait Permission: Send + Sync + 'static {
    const NAME: &'static str;
}

/// Conjunction of two permissions.
pub struct And<A, B>(PhantomData<(A, B)>);

/// "P contains permission X" — implemented for X itself and transitively through And.
pub trait Has<X: Permission> {}
impl<X: Permission> Has<X> for X {}
impl<X: Permission, B> Has<X> for And<X, B> {}
impl<X: Permission, A: Permission> Has<X> for And<A, X> {}
// (full transitivity worked out via sealed helper traits in implementation)

/// Trait listing permissions for runtime introspection (used by the extractor).
pub trait PermissionList {
    fn names() -> &'static [&'static str];
}
impl<P: Permission> PermissionList for P {
    fn names() -> &'static [&'static str] { &[P::NAME] }
}
// + impls for And<A, B>, etc.
```

The `permissions!` macro builds the type expression:

```rust
permissions!(SpeakersRead & SpeakersWrite)
// expands to: And<SpeakersRead, SpeakersWrite>

permissions!(SpeakersRead & SpeakersWrite & EventsRead)
// expands to: And<SpeakersRead, And<SpeakersWrite, EventsRead>>
```

Permission markers come from each plugin's `plugin_metadata!()` expansion (§11.2): one zero-sized struct per declared permission. **Referencing an undeclared permission is a compile error** because the type doesn't exist.

OR-style combinators (`Or<A, B>` + `HasAny<X>`) deferred until a real handler needs them.

## 11.5 Repositories — `junius-sdk`-enforced, permission-gated

Data access **must** go through a repository. `junius-sdk` enforces this by hiding the raw `sqlx::PgPool` type — it's not in `junius-sdk`'s public API, so `sqlx::query!()` etc. cannot compile outside a repository's impl.

```rust
// plugins/speakers/src/repo.rs

use junius_sdk::Repository;

#[derive(Repository, Clone)]
pub struct SpeakerRepo<P = ()> {
    // The derive macro injects:
    //   - a private `db: ScopedDb` field
    //   - a private `user: Option<User>` field
    //   - the PhantomData<P>
    //   - a `pub fn new(db: &PluginDb, user: Option<User>) -> Self` constructor
    //     (called by the plugin's #[derive(PluginCtx)] machinery — §11.6)
    //   - sealed accessors usable only inside #[impl_repository] blocks below
}

#[impl_repository(SpeakerRepo)]     // attribute macro that enables sqlx access within
impl<P: Has<SpeakersRead>> SpeakerRepo<P> {
    pub async fn get(&self, id: SpeakerId) -> Result<Speaker, RepoError> {
        // Inside this impl block, `self.pool()` is available (provided by the macro).
        // Outside such blocks, the pool is unreachable.
        sqlx::query_as!(Speaker, "SELECT * FROM speakers.speaker WHERE id = $1", id)
            .fetch_one(self.pool())
            .await
            .map_err(Into::into)
    }

    pub async fn list(&self) -> Result<Vec<Speaker>, RepoError> { /* ... */ }
}

#[impl_repository(SpeakerRepo)]
impl<P: Has<SpeakersRead> + Has<SpeakersWrite>> SpeakerRepo<P> {
    pub async fn create(&self, input: NewSpeaker) -> Result<Speaker, RepoError> { /* ... */ }
    pub async fn update(&self, id: SpeakerId, patch: SpeakerPatch) -> Result<Speaker, RepoError> { /* ... */ }
}

#[impl_repository(SpeakerRepo)]
impl<P: Has<SpeakersRead> + Has<SpeakersBook>> SpeakerRepo<P> {
    pub async fn book(&self, id: SpeakerId, slot: BookingSlot) -> Result<Booking, RepoError> { /* ... */ }
}
```

Usage from a route handler — repos are pre-instantiated in the per-plugin context (§11.6), so handlers don't construct them:

```rust
async fn get_speaker(
    ctx: SpeakersCtx<SpeakersRead>,
    Path(id): Path<SpeakerId>,
) -> Result<Json<Speaker>, ApiError> {
    let s = ctx.state.speakers.get(id).await?;
    Ok(Json(s))
}
```

**Properties:**

- **Repositories are mandatory.** `PluginResources` exposes no queryable `PluginDb` API; the only public type that can run SQL is one annotated with `#[derive(Repository)]`. No escape hatch for ad-hoc queries — new queries are new repository methods. Deliberate guardrail.
- **Permission types flow through `Has<X>` bounds** — same machinery used by the per-plugin extractor (§11.6). A handler that extracted `SpeakersCtx<SpeakersRead>` cannot call `ctx.state.speakers.create(...)` because `SpeakersWrite` isn't proven. Compile-time guarantee.
- **Platform-sdk supplies the primitives** (`PluginDb`, `ScopedDb`, `Repository` derive, `#[impl_repository(...)]`, `Has<X>`, `Permission`); plugins compose them. The derive + attribute macros are the only authorized path to a `sqlx` executor.
- **Cross-plugin reads** ([10-infrastructure-and-data.md](10-infrastructure-and-data.md) §10.3) still go through a repository — typically a consumer plugin defines a read-only view repo wrapping the dep's tables, with its own permission gates.

**`junius check` additionally verifies**:
- Every plugin crate declaring `db.read` or `db.write` capability has at least one `#[derive(Repository)]` struct.
- No `use sqlx::PgPool` or `use sqlx::query` outside `#[impl_repository(...)]` blocks (regex-scan; redundant with the type-hiding but produces clearer errors).

Object storage follows the same pattern: a `Bucket` derive for typed file access; raw `S3Client` not exposed.

## 11.6 Per-plugin state & extractor via `#[derive(PluginCtx)]`

Each plugin defines its own state type — typically a bundle of its typed repositories. A `#[derive(PluginCtx)]` macro generates the Axum extractor, including the permission check and per-request repository construction.

```rust
// plugins/speakers/src/lib.rs

use junius_sdk::PluginCtx;

/// Per-request state for the speakers plugin. The PluginCtx derive generates
/// the Axum extractor for SpeakersCtx<P> (= PluginContext<SpeakersState<P>, P>).
#[derive(Clone, PluginCtx)]
pub struct SpeakersState<P = ()> {
    #[repo] pub speakers: SpeakerRepo<P>,
    #[repo] pub bookings: BookingRepo<P>,
    // Additional plugin-specific per-request fields can go here. The macro
    // leaves non-#[repo] fields to be constructed via a #[from_request] hook.
}

pub type SpeakersCtx<P = ()> = junius_sdk::PluginContext<SpeakersState<P>, P>;
```

The derive expands (conceptually) to:

```rust
// generated by #[derive(PluginCtx)]
#[async_trait]
impl<P: PermissionList + 'static>
    FromRequestParts<PluginResources> for SpeakersCtx<P>
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, resources: &PluginResources)
        -> Result<Self, ApiError>
    {
        let user = extract_user(parts).await?;
        for perm in P::names() {
            if !user.has_permission(perm) {
                return Err(ApiError::forbidden(perm));
            }
        }
        let state = SpeakersState {
            speakers: SpeakerRepo::new(&resources.db, Some(user.clone())),
            bookings: BookingRepo::new(&resources.db, Some(user.clone())),
        };
        Ok(PluginContext {
            state,
            user: Some(user),
            resources: resources.clone(),
            _phantom: PhantomData,
        })
    }
}
```

The macro handles three responsibilities in one place: permission verification, repository instantiation with the request's user, and context assembly. Plugin authors write the struct; the macro writes the boilerplate.

Usage:

```rust
async fn create_speaker(
    ctx: SpeakersCtx<permissions!(SpeakersRead & SpeakersWrite)>,
    Json(input): Json<NewSpeaker>,
) -> Result<Json<Speaker>, ApiError> {
    let s = ctx.state.speakers.create(input).await?;   // compiles: SpeakersWrite proven by P
    Ok(Json(s))
}
```

A handler extracting `SpeakersCtx<SpeakersRead>` cannot call `ctx.state.speakers.create(...)`. The type system rejects it because `SpeakerRepo<SpeakersRead>` has no `create` method (it's only on `impl<P: Has<SpeakersWrite>> SpeakerRepo<P>`).

**Custom per-request state**: if a plugin needs more than repositories (e.g. a request-scoped cache, an audit-trail builder), it can add non-`#[repo]` fields and provide a `#[from_request]` hook that the derive calls. For v1 we expect ~all plugins to be repo-only; the hook lives in `#[derive(PluginCtx)]` as a deferred extension point (§11.12).

## 11.7 RPC + HTTP route registration

`Plugin::routes(&self, resources: PluginResources) -> Router` returns an **unprefixed** Axum router. The plugin attaches `resources` as Axum state (which the `#[derive(PluginCtx)]` extractor reads from). The host nests the returned router under the plugin's prefixes (RPC under `/rpc/<plugin>`, non-RPC HTTP under `/h/<plugin>`).

For Connect-RPC, plugins implement buf-generated service traits. The service implementation holds the resources (or a derived state) and the auto-generated `connect_rs::router_for` helper wires it up:

```rust
struct SpeakerServiceImpl {
    resources: PluginResources,
}

#[async_trait]
impl SpeakerService for SpeakerServiceImpl {
    async fn get_speaker(
        &self,
        ctx: SpeakersCtx<SpeakersRead>,        // extracted from the request — same machinery as HTTP
        req: GetSpeakerRequest,
    ) -> Result<Speaker, ConnectError> {
        let s = ctx.state.speakers.get(req.id.into()).await?;
        Ok(s.into())
    }
    /* ... */
}

fn routes(&self, resources: PluginResources) -> Router {
    let svc = SpeakerServiceImpl { resources: resources.clone() };
    Router::new()
        .merge(connect_rs::router_for(svc))      // RPC routes under /rpc/speakers (after host nesting)
        .route("/upload", post(upload_handler))  // non-RPC under /h/speakers
        .with_state(resources)                   // Axum state for HTTP extractors
}
```

Non-RPC handlers extract `SpeakersCtx<P>` exactly the same way as RPC handlers.

## 11.8 Permissions on RPC methods (proto-declared, codegen-enforced)

Permission requirements for RPC methods are declared as method options in `.proto` files:

```proto
import "platform/v1/annotations.proto";

service SpeakerService {
  rpc GetSpeaker (GetSpeakerRequest) returns (Speaker) {
    option (platform.requires) = "speakers:read";
  }
  rpc CreateSpeaker (CreateSpeakerRequest) returns (Speaker) {
    option (platform.requires) = "speakers:read,speakers:write";
  }
}
```

What this delivers:
- **Server-side enforcement**: the host's `rpc_guard` middleware reads each method's annotation from the generated `RPC_REQUIRES` table (codegened by `junius sync`) and rejects requests whose user lacks the listed permissions, **before** the handler runs. Plugin authors write no Rust permission code for the RPC surface.
- **Type-safe handler bodies (M15)**: the `#[junius_sdk::rpc_service(<ServiceTrait>)]` attribute macro rewrites an inherent `impl <RpcStruct> { … }` into the `connectrpc` trait impl. The ctx parameter's witness alias path (`crate::__rpc_requires::<service>::<Method>`) is emitted by each plugin's `build.rs` via `junius_rpc_meta::emit_rpc_requires` from the same proto annotation that drives `RPC_REQUIRES`, so the type-level expansion of the permission list and the pre-dispatch table can't drift. Repository gates from §11.5 apply transparently inside RPC handlers — the witness is built once from the proto.
- **Frontend awareness**: the same annotations are surfaced in the TS client, letting the FE gate UI elements (hide a "Create" button if the user lacks `speakers:write`).
- **Authoring & drift gates (M15)**:
  - **Manifest cross-check** — `junius check`'s `PROTO.REQUIRES.UNDECLARED` verifies every permission referenced from a proto appears in the plugin's `[permissions]`.
  - **`RPC.HANDLER.UNGUARDED`** — a bare `impl <X>Service for <Y>` in source skips the macro and is rejected.
  - **`RPC.SERVICE.UNIMPLEMENTED`** — every proto service must have a matching `#[rpc_service]` block.
  - **`RPC.WITNESS.MISMATCH`** — the ctx parameter's alias path must name its own method/service (purely syntactic).
  - `junius rpc scaffold --plugin <name>` inserts `todo!()` stubs for every proto method missing from its impl, idempotent.

## 11.9 Background jobs

```rust
fn jobs(&self) -> Vec<JobHandler> {
    vec![
        JobHandler::new::<SendBookingConfirmation>("speakers.send_booking_confirmation"),
        JobHandler::new::<SyncExternalSpeakers>("speakers.sync_external"),
    ]
}
```

`JobHandler` wraps a typed job (input struct + async handler). Plugins enqueue:

```rust
resources.jobs.enqueue(SendBookingConfirmation { booking_id }).await?;
```

Job names are namespaced `<plugin>.<job_name>` to prevent collisions. Jobs run with a synthetic system identity. If a job needs to invoke permission-gated repository methods, the platform supplies a `system_context::<SpeakersState<P>>(resources)` helper that constructs a privileged `SpeakersCtx<P>` with all permissions granted (bypassing the request-scoped permission check). Mechanism detail deferred to implementation.

## 11.10 Errors

```rust
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("capability not declared: {0}")]
    CapabilityNotDeclared(&'static str),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    External(#[from] anyhow::Error),
}
```

Plugin authors typically use `anyhow::Result` internally and convert at the boundary. RPC handlers convert to `ConnectError`; HTTP handlers convert to `ApiError` mapping to RFC 7807 problem details.

## 11.11 Host startup / shutdown sequence

1. **Migrations** — `junius migrate up` applies host + all plugin migrations ([10-infrastructure-and-data.md](10-infrastructure-and-data.md) §10.5).
2. **Resources construction** — host builds one `PluginResources` per plugin (scoped DB pool, storage prefix, telemetry pre-tagged with `plugin = "<name>"`, etc.). Per-request `PluginContext<S, P>` instances are built later by each plugin's `#[derive(PluginCtx)]` extractor.
3. **`on_startup`** — for each plugin in `plugins_generated` order: `plugin.on_startup(&resources).await?`. Fail-fast on error; host aborts.
4. **Job registration** — for each plugin: register `plugin.jobs()` with the queue, supplying the plugin's `PluginResources` so handlers can construct `system_context()` contexts when invoked.
5. **Route registration** — for each plugin: `plugin.routes(resources)` returns a Router (the plugin has internally `.with_state(resources)`-attached); host nests it under the plugin's prefixes (`/rpc/<plugin>`, `/h/<plugin>`, `/p/<plugin>`).
6. **Serve** — server starts accepting traffic.
7. **Graceful shutdown** — stop accepting → drain in-flight → for each plugin in reverse order: `plugin.on_shutdown(&resources).await` (errors logged, don't abort shutdown).

## 11.12 Implementation details still to work out

These are sequenced after this section lands; they don't block the design.

- Exact proc-macro implementation for `plugin_metadata!()`.
- Exact derive + attribute-macro implementation for `#[derive(Repository)]` and `#[impl_repository(...)]`.
- Exact derive implementation for `#[derive(PluginCtx)]` including the `#[repo]` field marker, the generated `FromRequestParts` impl, and the `#[from_request]` hook for non-repo fields.
- Full transitivity of `Has<X>` impls through nested `And` (sealed helper traits).
- `Bucket` derive for object storage — parallel to `Repository`.
- Job system identity model (`system_context::<S>(resources)` helper).
- ~~The buf custom-options plugin needed for `option (platform.requires)` to round-trip through Rust + TS codegen, including synthesizing the plugin's `<PluginName>Ctx<P>` type in handler signatures.~~ ✅ **Delivered by M15** ([`17-M15-rpc-service-macro.md`](../impl/17-M15-rpc-service-macro.md)): the `junius_rpc_meta` crate scans protos and codegens a per-method `__rpc_requires` alias from each `(platform.v1.requires)`; the `#[rpc_service]` macro uses the author's written ctx type verbatim — no buf plugin / `connectrpc-build` fork needed.
- ~~`connect_rs` interceptor wiring for proto-declared permission enforcement.~~ ✅ **Delivered by M06/M07**: host `rpc_guard` middleware reads from the `junius sync`-generated `RPC_REQUIRES` table.
- OR-style permission combinators (`Or<A, B>` + `HasAny<X>`).
