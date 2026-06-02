# The `junius` CLI

The `junius` management CLI drives the plugin lifecycle. Inside the dev shell run
it via `cargo run -p junius -- <cmd>`; most commands have a `task` shortcut that
passes `--config dev/platform.toml` for you.

```bash
cargo run -p junius -- <command> [args]
```

## Commands

| Command | Purpose |
| --- | --- |
| `new` | Scaffold plugins, migrations, components, RPC services, permissions. |
| `sync` | Regenerate composition glue from the deployment config. |
| `check` | Validate manifests against the schema + semantic rules. |
| `migrate` | Apply migrations; emit per-plugin Postgres role grants. |
| `dev` | One-command dev loop (migrate + host + Vite). |
| `build` | Build a deployment binary (source build). |
| `plugin` | Inspect / enable / disable plugins. |
| `i18n` | Validate and extract per-plugin i18n catalogs. |
| `rpc` | `#[rpc_service]` codemods (e.g. scaffold missing handlers). |
| `provision` | Apply the deployment's declarative provisioning. |
| `oidc` | OIDC group operations against a running `juniusd`. |
| `cache` | Manage the `junius` source cache. |

## Common invocations

### Scaffolding

```bash
cargo run -p junius -- new plugin myplugin                 # full-stack
cargo run -p junius -- new plugin myplugin --backend-only  # no frontend package
cargo run -p junius -- new migration myplugin add_thing    # next-numbered migration pair
```

`new plugin` validates the name against `^[a-z][a-z0-9_-]*$`. It's scaffold-only
— it does not run `sync`.

### Wiring and running

```bash
cargo run -p junius -- sync    --config dev/platform.toml   # task sync
cargo run -p junius -- migrate up --config dev/platform.toml # task migrate
cargo run -p junius -- migrate status --config dev/platform.toml
cargo run -p junius -- dev     --config dev/platform.toml   # task dev
cargo run -p junius -- build   --config dev/platform.toml   # task build
```

### Validation and inspection

```bash
cargo run -p junius -- check   --config dev/platform.toml   # full static gate
cargo run -p junius -- check   --plugin myplugin            # one plugin's manifest
cargo run -p junius -- plugin list  --config dev/platform.toml
cargo run -p junius -- plugin info  events                  # permissions, mounts, exposes
```

### RPC codemods

```bash
cargo run -p junius -- rpc scaffold --plugin myplugin       # stub unimplemented handlers
```

Inserts correctly-shaped `todo!()` stubs for every proto method without a
`#[rpc_service]` handler — the fix for `RPC.SERVICE.UNIMPLEMENTED`.

### i18n

```bash
cargo run -p junius -- i18n extract                              # update catalogs from source
cargo run -p junius -- i18n check --config dev/platform.toml     # validate all catalogs
cargo run -p junius -- i18n check --config dev/platform.toml --check-drift  # CI mode
```

### Provisioning

```bash
cargo run -p junius -- provision apply --config dev/platform.toml
```

Reconciles groups, roles, permissions, role assignments and OIDC mappings toward
the deployment's `[provisioning]` declaration. The block is hashed, so an apply
short-circuits when nothing changed.

## What `sync` generates

A single `sync` keeps all of these in step with the manifests + protos of the
enabled plugins:

- the host's compiled-in plugin registry (`platform/src/generated/plugins.rs`),
- the RPC permission table (`RPC_REQUIRES`),
- the frontend route tree (`platform/frontend/src/generated/routes.ts`) and the
  component & i18n catalog registries,
- each plugin's narrow generated RPC namespace (`@junius/generated/<name>/rpc`),
- the proto module list in `buf.yaml` (running `buf generate` if it changed),
- the `@junius/plugin-<name>` dependency in the host frontend's `package.json`
  (running `pnpm install` if it changed).

Re-run it after any change to a `plugin.toml`, a `.proto`, or the deployment's
enabled-plugins list.
