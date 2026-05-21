# M07 — Repository Pattern + Typed Permissions + `PluginCtx` Derive

## Goal

Plugins query the DB through `#[derive(Repository)]` types whose methods are gated by `Has<X>` permission bounds. `plugin_metadata!()` emits one zero-sized marker type per declared permission. `#[derive(PluginCtx)]` generates the Axum extractor that verifies permissions and instantiates the plugin's repos per request. RPC method permissions declared via `option (platform.requires) = "..."` are enforced server-side by a `connect-rs` interceptor and reflected in handler signatures.

## Why now

M06 gave us a `PluginDb` with no public query API. Without M07's pattern, plugins would either get a raw `PgPool` (breaks the design's guardrail) or be unable to talk to the DB (blocks everything). This milestone is the design's signature pattern landing in code — everything from M08 onward uses it.

## Scope (in)

### `junius-sdk` — typed permission system

```rust
// crates/junius-sdk/src/permissions/mod.rs

pub trait Permission: Send + Sync + 'static {
    const NAME: &'static str;
}

/// Conjunction at the type level.
pub struct And<A, B>(PhantomData<(A, B)>);

mod sealed {
    pub trait Sealed {}
}

/// `P contains permission X` — implemented for X itself and transitively through And.
pub trait Has<X: Permission>: sealed::Sealed {}

impl<X: Permission> sealed::Sealed for X {}
impl<X: Permission> Has<X> for X {}

impl<A, B> sealed::Sealed for And<A, B> {}
impl<X: Permission, B> Has<X> for And<X, B> {}
impl<X: Permission, A: Permission> Has<X> for And<A, X> {}
// Recursive impls for nested And trees:
impl<X: Permission, A, B: Has<X>> Has<X> for And<A, B> where A: Permission {}

/// For runtime introspection used by the FromRequestParts impls.
pub trait PermissionList {
    fn names() -> &'static [&'static str];
}
impl<P: Permission> PermissionList for P { ... }
impl<A: PermissionList, B: PermissionList> PermissionList for And<A, B> { ... }
```

`permissions!()` macro in `junius-sdk-macros`:

```rust
permissions!(SpeakersRead & SpeakersWrite)
// → And<SpeakersRead, SpeakersWrite>

permissions!(SpeakersRead & SpeakersWrite & EventsRead)
// → And<SpeakersRead, And<SpeakersWrite, EventsRead>>
```

### `plugin_metadata!()` extension

Emits a `permissions` module with marker types:

```rust
junius_sdk::plugin_metadata!();

// expanded:
pub static METADATA: PluginMetadata = ...;

pub mod permissions {
    pub struct HelloRead;
    impl ::junius_sdk::Permission for HelloRead {
        const NAME: &'static str = "hello:read";
    }
    pub struct HelloWrite;
    impl ::junius_sdk::Permission for HelloWrite {
        const NAME: &'static str = "hello:write";
    }
}
```

Referencing an undeclared permission is a compile error because the type doesn't exist.

### `#[derive(Repository)]` + `#[impl_repository(...)]`

`#[derive(Repository)]` injects private fields and a constructor:

```rust
#[derive(Repository, Clone)]
pub struct HelloRepo<P = ()> {
    // generated:
    //   db: ScopedDb,
    //   user: Option<User>,
    //   _phantom: PhantomData<P>,
    //
    //   pub fn new(db: &PluginDb, user: Option<User>) -> Self;
    //   (sealed) pub(crate) fn pool(&self) -> &PgPool;
}
```

`ScopedDb` is a `pub(crate)` wrapper around `PgPool` exposed only inside the `junius-sdk` crate; consumers can't construct it without the derive. The `pool()` accessor is gated by a sealed marker trait so it's only callable inside `#[impl_repository(...)]` blocks.

`#[impl_repository(HelloRepo)]` is an attribute macro that:
1. Verifies the impl target matches a `#[derive(Repository)]` struct.
2. Imports the sealed marker, making `self.pool()` callable inside the impl block.
3. Enforces nothing else — the type system does the rest.

Usage:

```rust
use junius_sdk::Has;
use crate::permissions::{HelloRead, HelloWrite};

#[impl_repository(HelloRepo)]
impl<P: Has<HelloRead>> HelloRepo<P> {
    pub async fn list(&self) -> Result<Vec<Greeting>, RepoError> {
        sqlx::query_as!(Greeting, "SELECT * FROM hello.greeting")
            .fetch_all(self.pool())
            .await
            .map_err(Into::into)
    }
}

#[impl_repository(HelloRepo)]
impl<P: Has<HelloRead> + Has<HelloWrite>> HelloRepo<P> {
    pub async fn create(&self, input: NewGreeting) -> Result<Greeting, RepoError> { ... }
}
```

Raw `sqlx::query*` calls outside an `#[impl_repository]` block won't compile because `PluginDb`'s `PgPool` accessor is sealed.

### `#[derive(PluginCtx)]`

Generates the Axum extractor.

```rust
use junius_sdk::PluginContext;
use crate::permissions;

#[derive(Clone, PluginCtx)]
pub struct HelloState<P = ()> {
    #[repo] pub greetings: HelloRepo<P>,
}

pub type HelloCtx<P = ()> = PluginContext<HelloState<P>, P>;
```

Expansion (conceptually):

```rust
#[async_trait]
impl<P: PermissionList + 'static>
    FromRequestParts<PluginResources> for HelloCtx<P>
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
        let state = HelloState {
            greetings: HelloRepo::new(&resources.db, Some(user.clone())),
        };
        Ok(PluginContext { state, user: Some(user), resources: resources.clone(), _phantom: PhantomData })
    }
}
```

Non-`#[repo]` fields can be supplied via a `#[from_request]` hook on the struct (deferred — M07 supports only repo fields).

### Buf custom-options plugin for `(platform.requires)`

`option (platform.requires) = "..."` was declared in M05 but ignored. M07 introduces:

1. A `protoc-gen-platform-requires` plugin (Rust binary, lives under `tools/protoc-gen-platform-requires/`) added to `buf.gen.yaml`. It outputs:
   - Rust: a `pub static` table mapping `(service, method)` → `&[&'static str]` (permission names).
   - TypeScript: an export alongside each method descriptor, e.g. `HelloService_Greet.requires = ['hello:read'] as const`.
2. A Connect-RS interceptor in `platform/auth/`: on every Connect request, looks up the static table, checks the user holds every listed permission, returns `permission_denied` if not.
3. Codegen produces method signatures using the plugin's `<PluginName>Ctx<P>` type with `P` derived from the requires list — so handler bodies have the same compile-time guarantee as HTTP handlers.

Plugin name is known at codegen time (the proto file lives under `plugins/<name>/proto/`), so the codegen synthesises:

```rust
async fn greet(
    &self,
    ctx: HelloCtx<permissions!(HelloRead)>,        // derived from `requires = "hello:read"`
    req: GreetRequest,
) -> Result<GreetResponse, ConnectError>;
```

For methods with empty `requires`, `P = ()` and no permissions are checked.

### Hello plugin updates

`plugins/hello/plugin.toml`:

```toml
[plugin]
name = "hello"
display_name = "Hello"
manifest_schema = 1

[permissions]
"hello:read"  = "Read greetings."
"hello:write" = "Create or modify greetings."
```

`plugins/hello/migrations/0001_greeting.up.sql`:

```sql
CREATE TABLE hello.greeting (
    id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name    TEXT NOT NULL,
    body    TEXT NOT NULL,
    created TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

`plugins/hello/src/repo.rs`:

```rust
use junius_sdk::Repository;
use crate::permissions::{HelloRead, HelloWrite};

#[derive(Repository, Clone)]
pub struct HelloRepo<P = ()> {}

#[impl_repository(HelloRepo)]
impl<P: Has<HelloRead>> HelloRepo<P> {
    pub async fn list(&self) -> Result<Vec<Greeting>, RepoError> { ... }
    pub async fn get(&self, id: GreetingId) -> Result<Option<Greeting>, RepoError> { ... }
}

#[impl_repository(HelloRepo)]
impl<P: Has<HelloRead> + Has<HelloWrite>> HelloRepo<P> {
    pub async fn create(&self, input: NewGreeting) -> Result<Greeting, RepoError> { ... }
}
```

`plugins/hello/src/lib.rs`:

```rust
#[derive(Clone, PluginCtx)]
pub struct HelloState<P = ()> {
    #[repo] pub greetings: HelloRepo<P>,
}
pub type HelloCtx<P = ()> = PluginContext<HelloState<P>, P>;
```

`plugins/hello/proto/hello/v1/hello.proto` extended:

```proto
service HelloService {
  rpc Greet (GreetRequest) returns (GreetResponse) {
    option (platform.requires) = "hello:read";
  }
  rpc ListGreetings (ListGreetingsRequest) returns (ListGreetingsResponse) {
    option (platform.requires) = "hello:read";
  }
  rpc CreateGreeting (CreateGreetingRequest) returns (CreateGreetingResponse) {
    option (platform.requires) = "hello:read,hello:write";
  }
}
```

Handler signatures (generated):

```rust
async fn create_greeting(
    &self,
    ctx: HelloCtx<permissions!(HelloRead & HelloWrite)>,
    req: CreateGreetingRequest,
) -> Result<CreateGreetingResponse, ConnectError> {
    let g = ctx.state.greetings.create(req.into()).await?;
    Ok(g.into())
}
```

### `junius check` extensions

- Scan every plugin source tree for `use sqlx::PgPool` or `sqlx::query*!` outside `#[impl_repository(...)]` blocks. Pattern is regex-based; conservative (false positives reported as warnings).
- Verify exactly one `plugin_metadata!()` invocation per plugin crate (parse the crate root).
- Validate every `option (platform.requires) = "name"` references a permission declared in the relevant plugin's manifest.

### Audit emission

Repo write methods record an audit event via `resources.audit.emit(...)`. The helper macro `audit_write!` (in `junius-sdk-macros`) makes this terse:

```rust
let g = ...;
audit_write!(self, "hello:greeting.create", g.id, json!({}));
```

Expansion uses `self.user.id` and the plugin's name (from `PluginMetadata`).

## Scope (out)

- No OR-style permission combinators (`Or<A,B>` / `HasAny<X>`). Deferred until a real handler needs them.
- No automatic ownership recording. `#[derive(Repository)]` doesn't touch `platform.resource_principal` — that's M08's `authz` API.
- No `#[derive(Bucket)]` for object storage. M10.
- No `#[from_request]` hook for non-repo state fields. M07 supports only repo fields.

## Library choices — confirm with user before starting

| Choice | Proposed default | Rationale | Downstream milestones to update if changed |
|---|---|---|---|
| **Proc-macro deps** | Already pinned in M02 (`syn 2 + quote + proc-macro2`) | — | — |
| **Compile-fail testing** | `trybuild` | Standard way to assert "this code should NOT compile" | — |
| **`.sqlx/` commit policy** | Yes — committed; CI runs `cargo sqlx prepare --check` | Locked by design ([../design/10-infrastructure-and-data.md](../design/10-infrastructure-and-data.md) §10.2) | M12 (extends to a `junius check` rule) |
| **Buf plugin runtime** | Buf-managed remote plugins for `protoc-gen-es` + `protoc-gen-connect-es`; **local** plugin for `protoc-gen-platform-requires` | The custom plugin is project-specific; remote plugins for the standard ones avoid maintenance | — |
| **`protoc-gen-platform-requires` language** | Rust (uses `prost-types` to parse descriptors) | Reuses dependencies already in the workspace | If we add a non-Rust output, switch to Go for descriptor handling |

## Open questions resolved

- **Trust model** (revisited from M06): with M07's compile-time permission system landing, it's worth restating: capability enforcement is *audit-only* (declared in manifests, checked by `junius check`), but **permission enforcement is compile-time** for repos and **runtime** for RPC interceptors. Third-party plugin support would later layer runtime capability enforcement on top.

## Verification

```bash
# Sync + generate
target/release/junius sync --config dev/platform.toml
target/release/junius migrate up --config dev/platform.toml

# Compile-fail test: a handler taking HelloCtx<HelloRead> cannot call .create()
cargo test -p hello-plugin --test compile_fail
# → trybuild reports the expected error: "method `create` not found ..."

# Functional permission check
# (Set up: alice has hello:read but not hello:write; bob has both.)

# Direct RPC test as alice:
curl -fsS -b alice-cookies.txt -X POST \
  http://127.0.0.1:8080/rpc/hello.v1.HelloService/ListGreetings \
  -H 'Content-Type: application/json' -d '{}'
# → 200

curl -i -b alice-cookies.txt -X POST \
  http://127.0.0.1:8080/rpc/hello.v1.HelloService/CreateGreeting \
  -H 'Content-Type: application/json' -d '{"name":"x","body":"y"}'
# → 403 with permission_denied (interceptor caught it before the handler ran)

# As bob:
curl -fsS -b bob-cookies.txt -X POST \
  http://127.0.0.1:8080/rpc/hello.v1.HelloService/CreateGreeting \
  -H 'Content-Type: application/json' -d '{"name":"x","body":"y"}'
# → 200; row visible in hello.greeting; audit_event row appended.

# junius check rules
target/release/junius check
# → exits 0

# Force a violation
echo "use sqlx::PgPool;" >> plugins/hello/src/lib.rs
target/release/junius check
# → reports the disallowed import outside #[impl_repository]
git checkout plugins/hello/src/lib.rs

# Proto annotation typo
sed -i 's/hello:read/hello:rade/' plugins/hello/proto/hello/v1/hello.proto
target/release/junius check
# → "permission 'hello:rade' referenced in proto but not declared in manifest"
git checkout plugins/hello/proto/hello/v1/hello.proto
```

End of M07: data access is permission-gated end-to-end at compile time (repos) and runtime (RPC). Every later plugin uses this pattern.
