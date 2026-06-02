# SDK surface

Everything a plugin can do with the host is reached through `junius-sdk`. This
page is a map of that surface — the `Plugin` trait, the per-request context, and
the capability handles. For the canonical definitions, read
`crates/junius-sdk/src/` (and the real usage in `plugins/events/`).

## The `Plugin` trait

Implemented once per plugin. Every method but `metadata` has a default, so a
minimal plugin overrides only what it uses.

| Method | Default | Purpose |
| --- | --- | --- |
| `metadata(&self) -> &'static PluginMetadata` | — (required) | The compiled manifest. Return `&METADATA` from `plugin_metadata!()`. |
| `routes(&self) -> Router` | — | Plain-HTTP routes, mounted at `/h/<name>`. |
| `register_rpc(&self, router) -> Router` | no RPC | Fold your Connect-RPC services into the shared router. |
| `on_startup(&self, &PluginResources)` | no-op | Run once after migrations, before serving. |
| `on_shutdown(&self, &PluginResources)` | no-op | Run once during graceful shutdown. |
| `jobs(&self) -> Vec<JobHandler>` | none | Register background-job handlers. |
| `register_i18n(&self, &mut LocalizerBuilder)` | no-op | Install the plugin's i18n catalog. |

## The per-request context

Your handlers receive a `PluginContext<S, P>`:

| Field | Type | Is |
| --- | --- | --- |
| `state` | `S` | Your repository bundle (a `#[derive(PluginCtx)]` struct), typed on the witness `P`. |
| `user` | `Option<User>` | The authenticated caller, if any. |
| `resources` | `PluginResources` | The capability handles (below). |

`P` is the permission witness; it gates which repository methods `state` can
call. You typically alias this as `MyCtx<P> = PluginContext<MyState<P>, P>`.

## Capability handles on `PluginResources`

| Handle | Type | Does | Gate |
| --- | --- | --- | --- |
| `db` | `PluginDb` | Sealed pool; reached only via repositories. | plugin role |
| `auth` | `Auth` | `current_user()`, `lookup_user()`. | — |
| `users` | `Users` | `lookup(id)` → display projection. | — |
| `groups` | `Groups` | `by_name`, `by_id`, `is_member`, `members`. | — |
| `authz` | `Authz` | `record_owner`, `forget_resource`, `share`, `unshare`. | ownership |
| `audit` | `AuditEmitter` | `emit(kind, actor, resource_kind, resource_id, details)`. | — |
| `email` | `Email` | `send(EmailMessage)`. | `email.send` |
| `jobs` | `Jobs` | `enqueue(job)`. | `job.enqueue` |
| `storage` | `PluginStorage` | `bucket(B)` → `put`/`get`/`delete`/`upload_url`/`download_url`. | `storage.*` |
| `config` | `PluginConfig` | `get(key)` / `get_opt(key)` typed deserialize. | — |
| `secrets` | `SecretStore` | `get(name)` → `SecretString`. | declared |
| `telemetry` | `Telemetry` | spans, counters, histograms (auto-tagged). | — |
| `localizer` | `Localizer` | `for_stored(locale)` / `for_request(...)` → `.t(msg)`. | — |
| `platform_admin` | `PlatformAdminApi` | cross-schema group/role admin. | `platform.admin` |

Capability-gated handles return `CapabilityNotDeclared` if the matching
`[requires] capabilities` entry is missing.

## Identity types

- **`User`** — `id`, `email`, `display_name`, `locale`, `memberships`,
  `user_roles`. Methods: `is_admin()`, `has_permission(p)`,
  `has_permission_in_group(group, p)`.
- **`UserId`**, **`GroupId`**, **`Principal`** (`User` / `Group` / `Public`).

## Macros (from `junius-sdk-macros`, re-exported by the SDK)

| Macro | Kind | Generates |
| --- | --- | --- |
| `plugin_metadata!()` | fn-like | `METADATA`, the `permissions` module, the `Bucket` enum, secret accessors. |
| `#[repository]` | attribute | The repo struct's sealed DB handle + `new(&PluginDb)`. |
| `#[impl_repository(Repo)]` | attribute | Attaches `Has<P>` bounds to a method block. |
| `#[derive(PluginCtx)]` | derive | Per-request resolution of resources + caller + repos, witness-verified. |
| `#[rpc_service(Trait)]` | attribute | The real `impl Trait for Y`, extracting the ctx + verifying the witness. |
| `permissions!(A & B)` | fn-like | A witness type `And<A, And<B, ()>>`. |
| `i18n_catalog!()` | fn-like | The generated `messages` structs + `catalog::register`. |

## Permission types

- **`Permission`** — trait each marker implements (carries `NAME`).
- **`Has<P>`** — bound proving a witness contains permission `P`.
- **`And<A, B>`**, **`()`** — witness composition; `()` is the no-permission
  witness used for public surfaces.

## Error types

| Type | Used by | Notable variants |
| --- | --- | --- |
| `PluginError` | resource handles | `CapabilityNotDeclared`, `Config`, `PermissionDenied`, `Db`, `External` |
| `RepoError` | repositories | `Db`, `NotFound`, `Plugin` |
| `ApiError` | context/RPC layer | `unauthenticated()`, `forbidden(perm)`, `internal(msg)` |

## Build-time crates

Used from a plugin's `build.rs` (the scaffold wires them):

- `connectrpc-build` — proto → Rust service trait + messages.
- `junius-rpc-meta` — `emit_rpc_requires(&PROTO_FILES)` → the per-method witness
  aliases (`__rpc_requires`).
- `junius-i18n-build` — `generate(Options::new("<plugin>"))` → typed message
  structs from the `.po` catalogs.

## The boundary rule

Plugins depend on `junius-sdk` and **never** on `platform/`. If you find yourself
wanting something the SDK doesn't expose, that's a signal to extend the SDK
deliberately (a host change), not to reach around it. Keeping this boundary
intact is what lets the host evolve without breaking every plugin.
