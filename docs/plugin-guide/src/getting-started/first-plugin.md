# Your first plugin

This chapter takes you from nothing to a running, navigable plugin. We'll create
a plugin called `myplugin`, wire it into the dev deployment, and see it serve a
page and an RPC call. Later chapters replace each placeholder with real
behaviour.

## 1. Scaffold

```bash
cargo run -p junius -- new plugin myplugin            # full-stack scaffold
# or, backend only (no frontend package):
cargo run -p junius -- new plugin myplugin --backend-only
```

The plugin name must match `^[a-z][a-z0-9_-]*$` — lowercase, starting with a
letter.

`junius new` is **scaffold-only**. It writes a starter `plugin.toml`,
`Cargo.toml`, `build.rs`, a `proto/` stub, an initial migration, a repository,
`src/lib.rs`, and (unless `--backend-only`) a `frontend/` package. It does
**not** run `sync`, and it doesn't need a `platform.toml` — wiring a plugin into
a deployment is `sync`'s job.

You now have:

```text
plugins/myplugin/
├── plugin.toml
├── Cargo.toml
├── build.rs
├── migrations/
├── proto/
├── i18n/
└── frontend/
    └── src/
        ├── index.ts
        └── routes/
```

## 2. Enable it in the deployment

A plugin only exists for a deployment once it's listed in that deployment's
`platform.toml`. Add it to the enabled list:

```toml
# dev/platform.toml
[plugins]
enabled = ["events", "admin", "myplugin"]
```

## 3. Wire it in

```bash
task sync     # = cargo run -p junius -- sync --config dev/platform.toml
```

`sync` regenerates all the **composition glue**:

- the host's compiled-in plugin registry,
- the RPC permission table (`RPC_REQUIRES`, from your proto annotations),
- the frontend route tree and the component registry,
- each plugin's narrow generated RPC namespace (`@junius/generated/<name>/rpc`),
- the proto module list in `buf.yaml` (and it runs `buf generate` if that
  changed),
- the `@junius/plugin-<name>` workspace dependency in the host frontend's
  `package.json` (and it runs `pnpm install` if that changed).

> Older docs mention two "manual steps" after `sync` (registering the proto in
> `buf.yaml`, adding the workspace dependency). `sync` now does both — if you're
> following a stale guide that tells you to edit `buf.yaml` by hand, you don't
> need to.

## 4. Run it

```bash
task infra:up     # if the stack isn't already up
task dev
```

Open the printed URL, log in, and navigate to `/p/myplugin`. You'll see the
placeholder page the scaffold generated. The backend is serving an RPC service
and the frontend is calling it.

## 5. The development loop

From here, the inner loop is small:

```text
edit Rust / TS / proto / manifest / migrations
        │
        ├─ changed a manifest or proto?  →  task sync
        ├─ changed a migration?          →  task migrate
        ├─ changed a SQL query?          →  regenerate the .sqlx cache (see Repositories)
        │
        └─ task lint        (fast: clippy, biome, buf, junius check)
           task test:rust   /  task test:js
           task ci          (the whole gate, before you push)
```

`task dev` hot-reloads the frontend on save; backend changes need a restart of
the `dev` task.

## What the scaffold gave you

| File | Purpose | Covered in |
| --- | --- | --- |
| `plugin.toml` | Identity, permissions, capabilities, exposes | [The plugin manifest](../backend/manifest.md) |
| `migrations/` | Your plugin's database schema | [Migrations and the database](../backend/migrations.md) |
| `src/lib.rs` | The `Plugin` impl + RPC services | [Proto and RPC services](../backend/rpc.md) |
| `src/repo/` | Repositories — the only path to your tables | [Repositories](../backend/repositories.md) |
| `proto/` | The API contract | [Proto and RPC services](../backend/rpc.md) |
| `frontend/src/routes/` | Pages and routing | [Routes and pages](../frontend/routes.md) |
| `frontend/i18n/` | Translation catalogs | [Internationalization](../capabilities/i18n.md) |

If you'd rather learn by reading a complete plugin, copy `plugins/events/` as
your starting point — every feature in this book is exercised there. The next
chapters build each piece up from the manifest outward.
