# 05 — Elixir/Phoenix Target Architecture

> Part of the [Junius → Elixir/Phoenix report](README.md). Forward-looking design. Elixir code
> below is **illustrative** — concrete enough to build against, not final API.

The target is a **Phoenix umbrella (or poncho) project**: the host is one OTP application, each
plugin is its own OTP application, and enabled plugins are **discovered and wired at runtime**.
The frontend is **LiveView** throughout — the React SPA and the entire Connect-RPC layer are
dropped. See [README decisions](README.md).

## 0. Top-level shape

```
junius/                          (umbrella or poncho)
├── apps/
│   ├── junius/                  the host: endpoint, auth, RBAC, ACL, plugin behaviour + registry,
│   │                            infra behaviours (Jobs/Mailer/Storage/Telemetry/Oidc)
│   ├── junius_ui/               shared design-system OTP app (Tailwind base + core components)
│   ├── events/                  a plugin OTP app  (Ecto schemas, contexts, LiveViews, jobs)
│   └── admin/                   a plugin OTP app
├── config/                      config incl. the enabled-plugin list
└── mix.exs
```

Third-party plugins are additional OTP apps brought in as **Hex dependencies** (see
[06](06-plugin-distribution-and-assets.md)).

## 1. The `Junius.Plugin` behaviour

The runtime analog of the Rust `Plugin` trait ([04 §3](04-plugin-interface.md)). Where Rust
generated a `Vec<Box<dyn Plugin>>` at compile time, Elixir declares a **behaviour** each plugin
module implements, and the host builds the registry at boot.

```elixir
defmodule Junius.Plugin do
  @moduledoc "Behaviour every plugin's entry module implements."

  @doc "The plugin manifest (parsed from plugin.toml or returned inline). Validated at boot."
  @callback manifest() :: Junius.Manifest.t()

  @doc "Router scope contributed by the plugin. Composed into the host router (see §3)."
  @callback routes(opts :: keyword()) :: Macro.t()

  @doc "Optional login-optional public routes, mounted outside the authed shell."
  @callback public_routes(opts :: keyword()) :: Macro.t()

  @doc "Exposed LiveView/function components: %{\"EventCard\" => Events.Components.EventCard}."
  @callback components() :: %{optional(String.t()) => module()}

  @doc "Cross-plugin public API module, if any (see §7). nil if the plugin exposes nothing."
  @callback public_api() :: module() | nil

  @doc "Child specs to start under the host supervisor (plugin-owned processes)."
  @callback children(ctx :: Junius.PluginContext.t()) :: [Supervisor.child_spec()]

  @doc "Oban worker modules the plugin registers (see §5)."
  @callback jobs() :: [module()]

  @doc "Run once after migrations, before serving. Default no-op."
  @callback on_startup(ctx :: Junius.PluginContext.t()) :: :ok | {:error, term()}

  @optional_callbacks public_routes: 1, components: 0, public_api: 0,
                      children: 1, jobs: 0, on_startup: 1
end
```

A plugin's entry module (`use Junius.Plugin` provides defaults + `@behaviour`):

```elixir
defmodule Events.Plugin do
  use Junius.Plugin

  @impl true
  def manifest, do: Junius.Manifest.load(:events)   # reads apps/events/priv/plugin.toml

  @impl true
  def routes(_opts) do
    quote do
      live "/events", Events.Live.Index, :index
      live "/events/:id", Events.Live.Show, :show
    end
  end

  @impl true
  def components, do: %{"EventCard" => Events.Components.EventCard,
                        "EventPicker" => Events.Components.EventPicker}

  @impl true
  def jobs, do: [Events.Jobs.SendSignupConfirmation]

  @impl true
  def public_api, do: Events.PublicApi
end
```

## 2. The runtime plugin registry

Config lists the enabled plugin apps (mirroring `[plugins].enabled`):

```elixir
# config/runtime.exs
config :junius, :plugins, [Events.Plugin, Admin.Plugin]
```

At boot, `Junius.PluginRegistry` (a `GenServer` or a compiled `:persistent_term`) loads each
module, **validates its manifest**, enforces boot-time invariants, and exposes lookups. This is
where the Rust compile-time guarantees become **boot-time assertions**:

```elixir
defmodule Junius.PluginRegistry do
  use GenServer

  def start_link(_), do: GenServer.start_link(__MODULE__, plugins(), name: __MODULE__)

  @impl true
  def init(modules) do
    plugins = Enum.map(modules, &load_and_validate!/1)
    enforce_trusted_capabilities!(plugins)   # only `admin` may declare platform.admin
    :persistent_term.put({__MODULE__, :plugins}, plugins)
    {:ok, plugins}
  end

  defp load_and_validate!(mod) do
    manifest = mod.manifest()
    :ok = Junius.Manifest.validate!(manifest)         # undeclared perms/caps/deps => raise, fail boot
    :ok = Junius.Manifest.validate_dependencies!(manifest, all_manifests())
    %{module: mod, manifest: manifest}
  end

  def all, do: :persistent_term.get({__MODULE__, :plugins})
  def component(key), do: # look up "<plugin>.<Name>" across registered components
end
```

Boot-time validation replaces `plugin_metadata!()`'s compile errors: a plugin referencing an
undeclared permission, or a consumer depending on a disabled plugin, **fails the boot**, not the
compile.

## 3. Routing & LiveView composition

Each plugin contributes a router scope; the host composes them. Because plugin route lists are
known at build time in the umbrella/assembler model ([06](06-plugin-distribution-and-assets.md)),
a **router macro** is the simplest approach:

```elixir
defmodule JuniusWeb.Router do
  use JuniusWeb, :router
  import Junius.Router, only: [plugin_scopes: 0, public_plugin_scopes: 0]

  pipeline :browser do
    plug :fetch_session
    plug Junius.Plug.LoadUser          # session cookie -> current_user (see §4)
    # …
  end

  scope "/", JuniusWeb do
    pipe_through :browser
    live_session :authed, on_mount: [{Junius.Auth, :require_authenticated}] do
      unquote(plugin_scopes())          # expands each enabled plugin's routes/1 under /<name>
    end
    live_session :public, on_mount: [{Junius.Auth, :mount_current_user}] do
      unquote(public_plugin_scopes())   # public_routes/1 under /i/<name>
    end
  end
end
```

For the pure-runtime, no-rebuild case (adding a plugin without recompiling the host), a **runtime
route table** driven by the registry is possible but heavier; it is only needed alongside the
self-served-asset escape hatch in [06](06-plugin-distribution-and-assets.md). Default to the
macro.

**Collaborative surfaces** (a decision — the live conference manager: live votes, speaker lists)
are ordinary LiveViews subscribing to **Phoenix.PubSub** topics. This is a genuine
simplification over the Rust RPC/polling model and a core reason LiveView fits this app.

## 4. Auth & the `User`

Port the identity model unchanged ([03 §4](03-architecture.md)): OIDC → server sessions in
Postgres → a `User` struct in the LiveView socket / conn assigns.

```elixir
defmodule Junius.Accounts.User do
  defstruct [:id, :email, :display_name, :locale, memberships: [], user_roles: []]

  @admin_wildcard "*"

  def admin?(%__MODULE__{user_roles: roles}),
    do: Enum.any?(roles, &(@admin_wildcard in &1.permissions))

  @doc "Held via any user-role or any group membership. Admin passes everything. (Layer 1.)"
  def has_permission?(user, perm) do
    admin?(user) or
      Enum.any?(user.user_roles, &(perm in &1.permissions)) or
      Enum.any?(user.memberships, &(perm in &1.permissions))
  end

  def has_permission_in_group?(user, group_id, perm) do
    admin?(user) or
      Enum.any?(user.memberships, &(&1.group_id == group_id and perm in &1.permissions))
  end
end
```

This is a near-verbatim port of `User::has_permission` / `is_admin` / `has_permission_in_group`
from `crates/junius-sdk/src/auth.rs` ([04 §5, §9](04-plugin-interface.md)).

- **OIDC** via `assent` or `ueberauth_oidcc` against Authentik; discovery-based so it stays
  provider-agnostic. OIDC `groups` claim reconciled on login (as today).
- **Sessions** server-side in `platform.session`; the cookie carries only the session id;
  encrypted OIDC tokens via **Cloak** (or `:crypto` AEAD).
- A `Junius.Plug.LoadUser` plug (controllers) and a `Junius.Auth` `on_mount` (LiveViews) resolve
  the session to a `User` and put it in assigns — the analog of the Rust session middleware.

## 5. Authorization — the two layers

**Layer 1 (capability gate)** becomes a plug + a LiveView `on_mount` hook, replacing the
`Has<X>` type witnesses and the RPC guard:

```elixir
defmodule Junius.Auth do
  import Phoenix.LiveView, only: [attach_hook: 4]

  # LiveView: require a permission before mount/handle_params.
  def on_mount({:require_permission, perm}, _params, session, socket) do
    socket = mount_current_user(session, socket)
    if Junius.Accounts.User.has_permission?(socket.assigns.current_user, perm),
      do: {:cont, socket},
      else: {:halt, Phoenix.LiveView.redirect(socket, to: "/403?perm=#{perm}")}
  end

  # Controller plug variant for /h-style endpoints and uploads.
  def require_permission(conn, perm), do: # 403 unless has_permission?/2
end
```

A plugin route declares its requirement in Elixir (the analog of the proto `requires`
annotation), e.g. `on_mount: [{Junius.Auth, {:require_permission, "events:write"}}]`.

**Layer 2 (resource scope)** is **ported verbatim** — `platform.user_can_access` and the
`record_owner` / `forget_resource` SECURITY DEFINER functions are recreated as Ecto migrations
([03 §5](03-architecture.md)). Every plugin query joins the function. A `Junius.Authz` context
wraps the definer calls:

```elixir
defmodule Junius.Authz do
  @doc "Record ownership atomically with a resource insert (calls platform.record_owner)."
  def record_owner(repo, kind, id, {:user, uid}), do: repo.query!("SELECT platform.record_owner($1,$2,$3,NULL)", [kind, id, uid])
  def record_owner(repo, kind, id, {:group, gid}), do: repo.query!("SELECT platform.record_owner($1,$2,NULL,$3)", [kind, id, gid])
  def forget_resource(repo, kind, id), do: repo.query!("SELECT platform.forget_resource($1,$2)", [kind, id])
  def share(...), do: # owner-checked + audited, on the platform repo
end
```

## 6. Data access — per-plugin Ecto repo on a per-plugin role

Keep the DB-level isolation ([README decision](README.md)). Each plugin has its **own
`Ecto.Repo`** authenticating as its Postgres role (`role_events`), so a plugin's queries are
constrained by grants at the database, not only by convention.

```elixir
defmodule Events.Repo do
  use Ecto.Repo, otp_app: :events, adapter: Ecto.Adapters.Postgres
end
```

```elixir
# config/runtime.exs — role + derived password, mirroring the Rust role_password_secret scheme
config :events, Events.Repo,
  username: "role_events",
  password: Junius.Roles.derive_password(role_secret, "role_events"),
  database: System.fetch_env!("DATABASE"),
  hostname: System.fetch_env!("PGHOST"),
  after_connect: {Postgrex, :query!, ["SET search_path = events, public", []]}
```

- Migrations still run as a **privileged migrator role** (`platform_migrator`), not the plugin
  role — the plugin role has no DDL rights. The host orchestrates ordering (host-first, then
  plugins by `@requires`), a small port of the Rust migration runner. (Whether to keep the
  `@requires` DAG or lean on Ecto's per-repo `migrations/` is an open question — see
  [08](08-sequencing-and-open-questions.md).)
- SQL confinement is **convention**, not a compiler guarantee: data access lives in the plugin's
  Ecto context modules; Credo rules + code review keep raw SQL out of LiveViews. Every read joins
  `user_can_access` and returns `viewer_can_*` fields, exactly as the Rust repos do
  ([03 §5](03-architecture.md)).

## 7. Cross-plugin access — context-function APIs (a decision)

Replace the Rust SQL-level `[exposes.tables]` sharing ([03 §7](03-architecture.md)) with
**explicit context-function APIs**. A plugin exposes a public module; it does **not** expose its
tables — and because the consumer's Postgres role has no grants on the provider's schema, the
boundary is also **DB-enforced** (strictly safer than the Rust model).

```elixir
defmodule Events.PublicApi do
  @moduledoc "Cross-plugin API. Every function takes the actor and re-applies BOTH authz layers."

  @doc "Events the actor may read. Applies capability + user_can_access; never unscoped."
  def list_events_for(%Junius.Accounts.User{} = actor) do
    Junius.Authz.require_permission!(actor, "events:read")   # Layer 1
    Events.Events.list_accessible(actor)                     # Layer 2: query joins user_can_access
  end
end
```

Rules:
- Every exposed function **takes the calling actor** and re-applies Layer 1 + Layer 2 internally.
  A cross-plugin call must never bypass the ACL.
- **Declared dependencies** in the manifest gate which plugins may call which APIs; the registry
  validates this at boot and calls from undeclared consumers are rejected/logged.
- **Optional deps degrade gracefully**: if the provider isn't enabled, `public_api()` is absent;
  consumers guard via `Junius.PluginRegistry`.
- Cross-plugin UI = the exposed function API (data) + the exposed component registry (rendering,
  via `Junius.PluginRegistry.component/1`).

## 8. Swappable infrastructure behaviours

Mirror the Rust "vendor crates behind traits" approach ([04 §4](04-plugin-interface.md)). Each
host service is a **behaviour**; the implementation is chosen by config; plugins call the
behaviour, never the vendor library. All handles are **capability-gated** (see §9).

```elixir
defmodule Junius.Jobs do
  @callback enqueue(job :: struct(), opts :: keyword()) :: {:ok, term()} | {:error, term()}
  @callback enqueue_at(job :: struct(), DateTime.t(), keyword()) :: {:ok, term()} | {:error, term()}

  # Facade used by plugins; dispatches to the configured impl and checks the capability first.
  def enqueue(plugin, job, opts \\ []) do
    with :ok <- Junius.Capabilities.require(plugin, "job.enqueue"),
         do: impl().enqueue(job, opts)
  end
  defp impl, do: Application.fetch_env!(:junius, :jobs_backend)   # default: Junius.Jobs.Oban
end
```

| Behaviour | Default impl | Capability gate |
|---|---|---|
| `Junius.Jobs` | **Oban** (`Junius.Jobs.Oban`) | `job.enqueue` |
| `Junius.Mailer` | **Swoosh** (SMTP) | `email.send` |
| `Junius.Storage` | **ExAws.S3** (MinIO) | `storage.read` / `storage.write` |
| `Junius.Telemetry` | **OpenTelemetry** → LGTM | — |
| `Junius.Oidc` | **assent/oidcc** (Authentik) | — (host-owned) |

**Oban** (a decision) subsumes the Rust RabbitMQ worker/DLQ/`meta.job_run` machinery: it provides
durable Postgres-backed jobs, retries, and a dashboard out of the box. Keeping it behind
`Junius.Jobs` means a future RabbitMQ/Broadway backend is a config swap.

## 9. Runtime capability enforcement & trust

Port the manifest `[requires.capabilities]` gate ([04 §4](04-plugin-interface.md)) as a runtime
check, from day one (because third-party plugins are first-class):

```elixir
defmodule Junius.Capabilities do
  def require(plugin, cap) do
    if cap in Junius.PluginRegistry.capabilities(plugin), do: :ok,
      else: {:error, {:capability_not_declared, cap}}
  end
end
```

Keep the **trusted-capability allowlist**: only the `admin` plugin may declare `platform.admin`;
the registry raises at boot otherwise (the port of `enforce_trusted_capabilities`).

**Honest caveat (document it):** on the BEAM all plugins share one VM. Capability enforcement
gates the *host-provided handles* — it is a **guardrail + audit surface + defense-in-depth**, not
a hard sandbox. A malicious plugin in the same VM can call arbitrary modules. True isolation
needs separate OS processes/nodes (out of scope for v1). See [07](07-mapping-and-tradeoffs.md).

## 10. Supervision tree (sketch)

```
Junius.Application
├── Junius.Repo                     (platform.* — privileged/app pool)
├── Junius.PluginRegistry           (loads + validates manifests at boot)
├── Oban                            (jobs)
├── Phoenix.PubSub                  (collaborative surfaces)
├── {Events.Repo, Admin.Repo, …}    (one per plugin, on its Postgres role)
├── plugin children (from each plugin's children/1)
└── JuniusWeb.Endpoint
```

## 11. Worked example — a minimal plugin end to end

A tiny `notes` plugin, showing the whole contract: manifest, migration, schema, context (with
both authz layers), a job, a LiveView with a permission gate, and the entry module.

```toml
# apps/notes/priv/plugin.toml
[plugin]
name = "notes"
display_name = "Notes"
manifest_schema = 1
[permissions]
"notes:read"  = "View notes you own or that are shared with you."
"notes:write" = "Create and edit notes."
[requires]
capabilities = []
```

```elixir
# apps/notes/priv/repo/migrations/0001_create_note.exs  (run as platform_migrator)
defmodule Notes.Repo.Migrations.CreateNote do
  use Ecto.Migration
  def change do
    execute "CREATE SCHEMA IF NOT EXISTS notes", "DROP SCHEMA notes"
    create table("note", prefix: "notes", primary_key: false) do
      add :id, :uuid, primary_key: true, default: fragment("gen_random_uuid()")
      add :title, :text, null: false
      add :body, :text
      timestamps(type: :utc_datetime_usec)
    end
    # grants for role_notes emitted by the host runner from [exposes.tables]/[permissions]
  end
end
```

```elixir
# apps/notes/lib/notes/note.ex
defmodule Notes.Note do
  use Ecto.Schema
  @schema_prefix "notes"
  @primary_key {:id, :binary_id, autogenerate: false}
  schema "note" do
    field :title, :string
    field :body, :string
    field :viewer_can_edit, :boolean, virtual: true
    timestamps(type: :utc_datetime_usec)
  end
end
```

```elixir
# apps/notes/lib/notes/notes.ex  — the context: BOTH authz layers
defmodule Notes.Notes do
  import Ecto.Query
  alias Notes.{Repo, Note}

  @kind "notes:note"

  def list_accessible(%Junius.Accounts.User{id: uid}) do
    # Layer 2: only rows user_can_access; viewer_can_edit computed by the ACL.
    Repo.all(
      from n in Note,
        where: fragment("platform.user_can_access(?, ?, ?, 'notes:read')", @kind, n.id, ^uid),
        select: %{n | viewer_can_edit:
          fragment("platform.user_can_access(?, ?, ?, 'notes:write')", @kind, n.id, ^uid)}
    )
  end

  def create(%Junius.Accounts.User{id: uid} = actor, attrs) do
    Junius.Authz.require_permission!(actor, "notes:write")     # Layer 1
    Repo.transaction(fn ->
      note = Repo.insert!(Note.changeset(%Note{}, attrs))
      Junius.Authz.record_owner(Repo, @kind, note.id, {:user, uid})  # ownership
      note
    end)
  end
end
```

```elixir
# apps/notes/lib/notes_web/live/index.ex
defmodule Notes.Live.Index do
  use JuniusWeb, :live_view
  on_mount {Junius.Auth, {:require_permission, "notes:read"}}

  def mount(_params, _session, socket) do
    notes = Notes.Notes.list_accessible(socket.assigns.current_user)
    {:ok, assign(socket, notes: notes)}
  end
  # render/1 uses Junius.UI components; "New" button shown only when has_permission?(user, "notes:write")
end
```

```elixir
# apps/notes/lib/notes/plugin.ex
defmodule Notes.Plugin do
  use Junius.Plugin
  @impl true
  def manifest, do: Junius.Manifest.load(:notes)
  @impl true
  def routes(_), do: quote(do: live("/notes", Notes.Live.Index, :index))
end
```

Enable it by adding `Notes.Plugin` to `config :junius, :plugins`. At boot the registry validates
the manifest; `notes:read`/`notes:write` are checked at runtime; the ACL enforces resource scope
in SQL. **No compile-time witnesses, no proto, no codegen step** — the whole Rust type-machinery
chain collapses into runtime checks + one boot-time validation.

Continue to [06 — Plugin distribution & assets](06-plugin-distribution-and-assets.md).
