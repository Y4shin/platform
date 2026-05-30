---
paths:
  - "**/*.rs"
---

# Rust conventions

- **Lint clean under `-D warnings`.** CI runs `cargo clippy --workspace --all-targets -- -D warnings`.
- **Format with `cargo fmt`** (or `task fmt`). Shared dependency versions go in the workspace
  root `[workspace.dependencies]`, not per-crate.

## Banned APIs (clippy.toml)

`std::env::var{,_os}`, `std::env::vars{,_os}`, `std::env::set_var`, `std::env::remove_var` are
disallowed — deployment config must flow through `platform::config::HostConfig` (a single
declarative source); mutating the environment is forbidden. To use a banned API anyway, add the
allow at the **narrowest** scope with a reason:

```rust
#[allow(clippy::disallowed_methods, reason = "<why this exception is OK>")]
```

## SQLx offline cache

`sqlx::query!` macros check against the committed `.sqlx/` cache with `SQLX_OFFLINE=true`. When
you add or change a SQL query, regenerate the cache (needs the dev DB + `DATABASE_URL`):

```bash
cargo sqlx prepare --workspace
```

`task sqlx:check` (`cargo sqlx prepare --workspace --check`) verifies it's fresh; CI fails on a
stale cache.

## Plugin / host boundary

Plugins (`plugins/*`) depend on **`junius-sdk`**, never on `platform/` directly. New
plugin-facing API is added to `junius-sdk` (+ `junius-sdk-macros` for derive/attribute macros),
keeping host internals (auth, users, RBAC, DB pool) from leaking. See
[docs/design/11-backend-plugin-interface.md](../../docs/design/11-backend-plugin-interface.md).
