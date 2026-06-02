# `plugin.toml` reference

The manifest is the single source of truth for a plugin's identity. This page is
the field-by-field reference; for the narrative, see
[The plugin manifest](../backend/manifest.md).

## `[plugin]`

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `name` | string | yes | Machine name (`^[a-z][a-z0-9_-]*$`). Drives mounts, role, package name. |
| `display_name` | string | yes | Human-facing label. |
| `description` | string | yes | One-line description. |
| `manifest_schema` | integer | yes | Manifest format version (currently `1`). |
| `public_routes` | bool | no (default `false`) | Opt into a login-optional surface at `/i/<name>`. |

## `[permissions]`

A table of `"<name>" = "<description>"`. Convention: `<plugin>:<verb>`.

```toml
[permissions]
"events:read"  = "View events you own, that belong to your groups, or that are public."
"events:write" = "Create, edit, and delete events and configure their invite pages."
"events:share" = "Share a private event with another user or group."
```

Each generates a marker type (`events:read` → `permissions::EventsRead`) and is
referenceable from proto `(platform.v1.requires)`. Appears in the admin
permission picker automatically.

## `[exposes.components.<Name>]`

Register a React component for cross-plugin reuse via `useComponent('<plugin>.<Name>')`.

| Field | Type | Meaning |
| --- | --- | --- |
| `module` | string | Path relative to the frontend `src/` (e.g. `./lib/EventCard`). |
| `description` | string | One-liner shown to integrators. |

Each must be a named export of `frontend/src/index.ts` (`FE.EXPORTS.MATCH_MANIFEST`).

## `[exposes.tables.<name>]`

Expose a table so other plugins' roles get DML on it. Private by default.

| Field | Type | Meaning |
| --- | --- | --- |
| `schema` | string | The Postgres schema the table lives in. |
| `description` | string | What the table holds and its access model. |

## `[requires]`

| Field | Type | Meaning |
| --- | --- | --- |
| `capabilities` | array of string | Host capabilities to unlock (see below). |

### Known capabilities

| Capability | Unlocks |
| --- | --- |
| `email.send` | `resources.email` |
| `job.enqueue` | `resources.jobs` |
| `storage.read` | `resources.storage` read ops (`get`, `download_url`) |
| `storage.write` | `resources.storage` write ops (`put`, `upload_url`, `delete`) |
| `platform.admin` | cross-schema admin API — allow-listed to the `admin` plugin |

## `[requires.secrets]`

Typed secrets the plugin reads at runtime. Declaring a secret generates a typed
accessor and makes it available through `resources.secrets()`. Values are
supplied by the deployment (`env:` / `file:` indirection).

## `[storage.buckets.<name>]`

Declare a logical bucket the plugin reads/writes. Must be mapped to a physical
bucket in the deployment (`STORAGE.BUCKET.UNMAPPED` otherwise). Generates a
`Bucket` enum variant.

| Field | Type | Meaning |
| --- | --- | --- |
| `description` | string | What the bucket holds. |

## What reads the manifest

- **`junius sync`** — generates the plugin registry, RPC permission table,
  frontend route tree, component & i18n registries, and the proto/workspace
  wiring.
- **`junius check`** — validates permission references, exposed-export matching,
  cross-plugin table access, FK cascade rules, bucket mapping, and more.
- **`plugin_metadata!()`** — at compile time, generates the permission markers,
  the `Bucket` enum, secret accessors, and the `METADATA` static.
