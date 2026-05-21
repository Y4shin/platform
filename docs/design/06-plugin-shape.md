# 6. Plugin Shape

A plugin is a single directory containing:

```
plugins/speakers/
├── plugin.toml           # Manifest: name, route prefix, deps, exposed components
├── Cargo.toml            # Rust crate
├── src/                  # Rust backend (lib.rs exposes register())
├── proto/                # Connect-RPC .proto files
└── frontend/
    ├── package.json      # @junius/plugin-speakers
    ├── routes/           # TanStack Router subtree (mounted under /p/speakers)
    ├── lib/              # Public components for other plugins
    ├── rpc/              # Generated Connect-RPC client (build artifact)
    └── index.ts          # Public exports
```

## 6.1 Manifest

Each plugin has a `plugin.toml` at its root. This file is the **only** thing `junius` reads to decide how to compose a plugin into the platform — everything else (`Cargo.toml`, `package.json`, `buf.yaml`, generated code) derives from it.

**Format**: TOML. Schema defined as a Rust struct in `junius`, deserialized via serde. The `manifest_schema` field is the only versioning concept retained (it lets the manifest format evolve without breaking older plugins).

```toml
# plugins/speakers/plugin.toml

# --- Identity -------------------------------------------------------------
[plugin]
name = "speakers"                     # unique platform-wide
display_name = "Speakers"
description = "Manage public speakers and their booking history."
manifest_schema = 1                   # manifest format version

# --- Mount points (auto-derived from name if omitted) ---------------------
[mount]
route_prefix = "/p/speakers"          # SPA routes
rpc_prefix   = "/rpc/speakers"        # Connect-RPC
http_prefix  = "/h/speakers"          # non-RPC HTTP (uploads, OAuth callbacks)

# --- Inter-plugin dependencies --------------------------------------------
# All plugins live in this monorepo (directly or via git submodule); the
# workspace tree IS the version. There is no semver and no resolver.
# `junius check` only validates that required deps are enabled and that
# imports/usages match declarations.

[dependencies.identity]
optional = false

[dependencies.venues]
optional = true                       # graceful degradation via typed registry

# --- What this plugin exposes to other plugins ----------------------------
[exposes.components.SpeakerCard]
module      = "./frontend/lib/SpeakerCard"
description = "Compact speaker summary card."

[exposes.components.SpeakerPicker]
module      = "./frontend/lib/SpeakerPicker"
description = "Searchable speaker selector."

# --- Permissions defined by this plugin -----------------------------------
[permissions]
"speakers:read"  = "View speakers and their booking history."
"speakers:write" = "Create, edit, and delete speakers."
"speakers:book"  = "Book a speaker for an event."

# --- Platform capabilities required (audit-only in v1) --------------------
[requires]
capabilities = ["db.read", "db.write", "storage.write", "email.send"]
```

### Design choices locked in

- **Manifest is the single source of truth for plugin-level metadata.** Language-native files (`Cargo.toml`, `package.json`) own language-native concerns (Rust deps, npm deps); the manifest only declares **inter-plugin** relationships and platform-facing facts.
- **Plugin layout is fixed by convention** (see §6). No `[paths]` block; every plugin uses the same directory structure. Plugins that don't need a section (no RPC, no migrations, etc.) simply omit the directory.
- **Plugin enablement lives in the deployment's `platform.toml`** ([05-repository-and-deployment-layout.md](05-repository-and-deployment-layout.md)), not in each plugin's manifest. A plugin doesn't decide whether it's enabled — the deployment does. The source monorepo has no opinion on which plugins ship in any given binary.
- **Permissions are declared in the manifest, not in Rust code.** Makes them auditable without running code, drives the platform's RBAC tables, and lets `junius docs generate` enumerate them statically. Plugins still *enforce* permissions in code; the manifest only *declares* them.
- **Capabilities are audit-only in v1.** The `[requires.capabilities]` list is informational; `junius plugin info` displays it, admins can review before enabling. Runtime enforcement (refusing to give a `db` handle to a plugin that didn't declare `db.write`) is deferred until/unless we ever accept untrusted third-party plugins.
- **No per-plugin versioning, no dependency version constraints.** Single-version monorepo (including git submodules) makes semver ceremony rather than mechanism. `manifest_schema` is the only versioning concept retained. The platform's deployment artifact has its own version (from build metadata), but plugins do not. If a plugin is ever published externally, versioning becomes a deliberate addition at that point.
- **Cross-repo plugins use git submodules.** A submodule shows up under `plugins/<name>/` like any in-tree plugin; `junius` treats it identically. The submodule's pinned revision in the parent repo's git index is the implicit version.
- **`Cargo.toml` and `package.json` use workspace protocols** for cross-plugin deps (`path = "../<plugin>"`, `workspace:*`). Their own `version` fields are sentinels (e.g. `0.0.0`) — never consumed by a resolver. `junius sync` writes these inside marker comments; plugin authors don't touch them.
- **TOML, not YAML/JSON.** Matches Cargo, easy to read, no YAML footguns.
- **Mount paths default from `plugin.name`** but can be overridden — useful for renames and migrations.
