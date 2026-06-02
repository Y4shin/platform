# The plugin manifest

`plugin.toml` is the **single source of truth** for your plugin's identity. The
host's code generation (`junius sync`) and its static checks (`junius check`)
both read it, so most "wiring" is just declaring the right thing here and
letting the tooling follow.

Here is the `events` manifest in full (`plugins/events/plugin.toml`):

```toml
[plugin]
name = "events"
display_name = "Events"
description = "Events (user- or group-owned, private or public) with sign-up invite pages and iCalendar export."
manifest_schema = 1
# Opt in to a public (login-optional) frontend surface. `junius sync` mounts
# this plugin's `buildPublicRoutes` at `/i/events` outside the authed shell.
public_routes = true

[permissions]
"events:read" = "View events you own, that belong to your groups, or that are public."
"events:write" = "Create, edit, and delete events and configure their invite pages."
"events:share" = "Share a private event with another user or group."

[exposes.components.EventCard]
module = "./lib/EventCard"
description = "Compact event summary card (title, when, where, visibility)."

[exposes.components.EventPicker]
module = "./lib/EventPicker"
description = "Searchable event selector for cross-plugin reuse."

[exposes.tables.event]
schema = "events"
description = "Events (public ones are world-readable; private ones gated by ownership)."

[requires]
capabilities = ["email.send", "job.enqueue"]
```

Let's walk through each block. (A field-by-field schema lives in the
[`plugin.toml` reference](../reference/manifest.md).)

## `[plugin]` — identity

| Field | Meaning |
| --- | --- |
| `name` | The stable machine name. Drives mount points (`/p/<name>`, `/h/<name>`), the Postgres role (`role_<name>`), and the frontend package name (`@junius/plugin-<name>`). |
| `display_name` | Human-facing label shown in navigation. |
| `description` | One line; shown in admin/inspection surfaces. |
| `manifest_schema` | The manifest format version (currently `1`). |
| `public_routes` | `true` opts the plugin into a login-optional surface at `/i/<name>`. Omit it (defaults to `false`) if every page requires a session. |

## `[permissions]` — what your plugin gates on

Each entry is `"<name>" = "<human description>"`. The convention is
`<plugin>:<verb>` (`events:read`, `events:write`, `events:share`). These three
strings are the **vocabulary** the rest of your plugin gates on, and they have
reach beyond the code:

- `plugin_metadata!()` generates a zero-sized **marker type** per permission
  (`events:read` → `permissions::EventsRead`) for compile-time checks.
- proto methods reference them by string in `option (platform.v1.requires)` for
  the runtime RPC gate.
- They appear automatically in the **admin plugin's permission picker**, so an
  operator can grant them to a group-role or user-role. There's no second
  catalogue to maintain.

`junius check` validates that every `requires` string and every marker
corresponds to a permission declared here. See
[Permissions and access control](./permissions.md) for how the markers are used.

## `[exposes.components.*]` — React components for other plugins

Declaring a component here registers it in the host component registry so other
plugins can consume it via `useComponent('<plugin>.<name>')`:

```toml
[exposes.components.EventCard]
module = "./lib/EventCard"   # relative to this plugin's frontend `src/`
description = "Compact event summary card."
```

Each declared component **must** be a named export of `frontend/src/index.ts` —
`junius check`'s `FE.EXPORTS.MATCH_MANIFEST` rule enforces it. See
[Exposed components](../frontend/exposed-components.md).

## `[exposes.tables.*]` — database tables for other plugins

By default, a plugin's tables are **private** — no other plugin's Postgres role
can touch them, and `junius check` will flag a cross-plugin query
(`SQL.PRIVATE_TABLE_ACCESS`). Declaring a table here grants the appropriate
role DML on it and marks it part of your plugin's public surface:

```toml
[exposes.tables.event]
schema = "events"
description = "Events (public ones are world-readable; private ones gated by ownership)."
```

Expose only what other plugins genuinely need. `events` exposes `event` but
keeps its `invite`, `signup` and `calendar_token` tables private.

## `[requires]` — host capabilities

Capabilities are infrastructure your plugin needs the host to provide. Declaring
them here is what *unlocks* the matching handle on `PluginResources`:

```toml
[requires]
capabilities = ["email.send", "job.enqueue"]
```

The common ones:

| Capability | Unlocks | Chapter |
| --- | --- | --- |
| `email.send` | `resources.email.send(...)` | [Jobs and email](../capabilities/jobs-and-email.md) |
| `job.enqueue` | `resources.jobs.enqueue(...)` | [Jobs and email](../capabilities/jobs-and-email.md) |
| `storage.read` / `storage.write` | `resources.storage.bucket(...)` | [Object storage](../capabilities/storage.md) |
| `platform.admin` | cross-schema admin API (allow-listed; admin plugin only) | — |

Call a capability-gated handle without declaring it and you get a runtime
`CapabilityNotDeclared` error — not a panic, but a clean failure. Declare what
you use, and nothing more.

## Other blocks you may meet

The manifest also supports:

- `[requires.secrets]` — typed secrets your plugin reads at runtime (see
  [SDK surface](../reference/sdk.md#secrets)).
- `[storage.buckets.*]` — logical buckets your plugin reads/writes (see
  [Object storage](../capabilities/storage.md)).
- `[requires.dependencies]` / cross-plugin deps — when you consume another
  plugin's exposed components or tables.

These are covered where they're used. The full grammar is in the
[manifest reference](../reference/manifest.md).

With the manifest declared, the next step is the database your plugin owns.
