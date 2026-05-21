# M02 — Platform Core (Host) Bootstrap

> **Status:** ✅ Implemented — see commit `M02: platform host bootstrap + plugin_metadata! macro` in `git log`.

## Goal

The platform host binary boots Axum, holds an (empty) plugin registry, and serves `200 OK` on `/`. The `Plugin` trait and the `plugin_metadata!` proc-macro exist in `junius-sdk`, ready for M03's first plugin to implement.

## Why now

With `junius` and manifests in place (M01), the next foundation piece is the runtime that plugins plug into. Building it without plugins means we can test the boot path, the registry plumbing, and the SDK surface independently — M03 then validates the whole stack with the first real plugin.

## Scope (in)

### `crates/junius-sdk`

The public API surface every plugin imports. At M02, this is intentionally narrow — DB, storage, jobs, email, auth handles arrive in later milestones (M06–M10) and slot into `PluginResources` as fields. Per [../design/11-backend-plugin-interface.md](../design/11-backend-plugin-interface.md) §11.1–§11.3.

```rust
// crates/junius-sdk/src/lib.rs

pub mod plugin;
pub mod metadata;
pub mod resources;
pub mod error;
pub mod telemetry;
pub mod config;

pub use junius_sdk_macros::plugin_metadata;

// Re-export the trait and types
pub use plugin::Plugin;
pub use metadata::PluginMetadata;
pub use resources::PluginResources;
pub use error::PluginError;
```

`plugin.rs`:

```rust
#[async_trait::async_trait]
pub trait Plugin: Send + Sync + 'static {
    fn metadata(&self) -> &'static PluginMetadata;
    fn routes(&self, resources: PluginResources) -> axum::Router;

    async fn on_startup(&self, _resources: &PluginResources) -> Result<(), PluginError> {
        Ok(())
    }
    async fn on_shutdown(&self, _resources: &PluginResources) -> Result<(), PluginError> {
        Ok(())
    }

    // jobs() lands in M10
}
```

`metadata.rs`:

```rust
pub struct PluginMetadata {
    pub name: &'static str,
    pub display_name: &'static str,
    pub description: Option<&'static str>,
    pub manifest_schema: u32,
    pub mount: MountPoints,
    pub dependencies: &'static [DependencyDecl],
    pub exposed_components: &'static [ExposedComponentDecl],
    pub exposed_tables: &'static [ExposedTableDecl],
    pub permissions: &'static [PermissionDecl],
    pub capabilities: &'static [&'static str],
}

pub struct MountPoints {
    pub route_prefix: &'static str,
    pub rpc_prefix: &'static str,
    pub http_prefix: &'static str,
}
// ... DependencyDecl, ExposedComponentDecl, ExposedTableDecl, PermissionDecl
```

`resources.rs` (M02 stub — fields added by later milestones):

```rust
#[derive(Clone)]
pub struct PluginResources {
    pub config: PluginConfig,
    pub telemetry: Telemetry,
    // DB, storage, jobs, email, auth arrive in M06–M10
}
```

`config.rs`:

```rust
#[derive(Clone)]
pub struct PluginConfig {
    inner: Arc<PluginConfigInner>,
}

impl PluginConfig {
    pub fn get<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<T, PluginError> {
        // looks up `[plugins.<name>]` from the loaded platform.toml
    }
}
```

`telemetry.rs`:

```rust
#[derive(Clone)]
pub struct Telemetry {
    plugin_name: &'static str,
}

impl Telemetry {
    pub fn tracer(&self) -> /* opaque wrapper around tracing::Span */ { ... }
    pub fn counter(&self, name: &str) -> Counter { ... }
}
// At M02 this is a thin wrapper over `tracing`; OTel exporter wiring lands in M10.
```

`error.rs`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("capability not declared: {0}")]
    CapabilityNotDeclared(&'static str),

    #[error("config error: {0}")]
    Config(String),

    #[error(transparent)]
    External(#[from] anyhow::Error),
}
```

### `crates/junius-sdk-macros`

```rust
// crates/junius-sdk-macros/src/lib.rs
use proc_macro::TokenStream;

#[proc_macro]
pub fn plugin_metadata(_input: TokenStream) -> TokenStream {
    // 1. Locate the calling crate's plugin.toml via CARGO_MANIFEST_DIR.
    // 2. Parse it (reuse the same serde structs from tools/junius by pulling
    //    them into a small `junius-manifest` library — extract to crates/manifest/
    //    so both junius and the macro consume one schema).
    // 3. Emit:
    //      pub static METADATA: junius_sdk::PluginMetadata = PluginMetadata { ... };
    //    Permission marker types (the `pub mod permissions { ... }` block) arrive in M07.
}
```

To share the manifest parser between `junius` and `junius-sdk-macros`, M02 introduces a new workspace crate:

```
crates/manifest/                       # added in M02
├── Cargo.toml
└── src/
    └── lib.rs                         # re-export of types moved from tools/junius/manifest/
```

Both `junius` and `junius-sdk-macros` depend on it. This avoids drift between "what junius thinks the manifest looks like" and "what the macro emits".

### `platform/` host binary

```
platform/
├── Cargo.toml
└── src/
    ├── main.rs                        # parses CLI, loads platform.toml, calls server::run()
    ├── server.rs                      # builds Axum router, mounts plugins, runs the server
    ├── boot.rs                        # plugin registry assembly + lifecycle orchestration
    ├── config.rs                      # platform.toml loader (uses crates/manifest)
    ├── plugin_registry.rs             # Vec<Box<dyn Plugin>> + iteration helpers
    └── generated/
        └── plugins.rs                 # `pub fn plugins() -> Vec<Box<dyn Plugin>> { vec![] }`
                                        # ← this file is overwritten by `junius sync` in M03
```

`generated/plugins.rs` is committed initially as an empty vec; `junius sync` (M03) takes over. M02 includes a header comment marking it as generated.

`server.rs` does:
1. Build the base Axum router (just a `/` route returning `"hello, platform"` for M02).
2. For each plugin from `plugin_registry::all()` (empty at M02, populated at M03+):
   - Build that plugin's `PluginResources`.
   - Call `plugin.on_startup(&resources).await`.
   - Mount `plugin.routes(resources)` under the plugin's mount prefixes.
3. Bind to `BIND_ADDR` (env, default `127.0.0.1:8080`).
4. `tokio::select!` between `axum::serve(...)` and a Ctrl-C signal; on shutdown, iterate plugins in reverse and call `on_shutdown`.

CI extension: `cargo test --workspace` runs (integration test in `platform/tests/boot.rs` asserts a fresh boot serves 200 on `/`).

## Scope (out)

- No plugins. Registry is empty.
- No DB, storage, jobs, email. `PluginResources` has only `config` + `telemetry`.
- No frontend. The `/` route just returns text.
- No proc-macro permission emission. `plugin_metadata!` emits only the `METADATA` constant; the `permissions` module appears in M07.
- No `rust-embed`. The binary doesn't serve a frontend until M04.

## Library choices — confirm with user before starting

| Choice | Proposed default | Rationale | Downstream milestones to update if changed |
|---|---|---|---|
| **HTTP framework** | **`axum`** | Locked by design ([../design/03-backend.md](../design/03-backend.md) §3.1) | n/a |
| **Async runtime** | **`tokio`** with `full` features | Axum's native runtime; standard | Every backend milestone uses tokio primitives |
| **Async-trait machinery** | **`async-trait`** crate | Stable object-safe async trait methods aren't out yet; necessary for `Box<dyn Plugin>` | Every async trait in `junius-sdk`. Switch to native when Rust supports object-safe async-fn-in-trait |
| **Proc-macro deps** | `syn 2 + quote + proc-macro2` | Standard Rust proc-macro stack | M07 (Repository / PluginCtx derives) |
| **Error type (SDK)** | **`thiserror`** | Idiomatic for library-style errors with `From` impls | All future SDK error types |
| **Error type (plugins)** | **`anyhow`** internally; convert at the boundary to `PluginError` / `ApiError` | Concise plugin code; explicit boundary errors | All plugin code from M03 on |
| **Tracing** | **`tracing`** + **`tracing-subscriber`** with env-filter | Standard observable Rust apps; OTel layered on in M10 | M10 (OTel exporter), every milestone that emits spans |
| **Workspace crate for manifest schema** | New `crates/manifest/` crate, depended on by both `junius` and `junius-sdk-macros` | Single source of truth; prevents drift | M07 (extends with permission emit), M11 (extends with `[source]` resolution) |

## Open questions resolved

None blocking M02.

## Verification

```bash
# Build the host
cargo build -p platform

# Boot the host
cargo run -p platform &
PID=$!

# Verify it responds
curl -fsS http://127.0.0.1:8080/  # → "hello, platform"

# Verify graceful shutdown
kill -INT $PID
wait $PID                          # exits 0

# Verify integration test
cargo test -p platform
# `platform/tests/boot.rs` asserts: 200 on /, then graceful shutdown completes

# Verify the macro builds end-to-end through a fixture crate
cargo test -p junius-sdk-macros
# (uses trybuild + a fixture crate under junius-sdk-macros/tests/fixtures/
#  with a minimal plugin.toml; asserts `plugin_metadata!()` expands and the
#  resulting METADATA constant is reachable.)

# Verify docs generate
cargo doc -p junius-sdk
# Output should document the Plugin trait, PluginMetadata, PluginResources,
# PluginConfig, Telemetry, PluginError.
```

When all checks pass, the runtime substrate is in place. M03 adds the first plugin.
