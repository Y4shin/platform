# 06 — Plugin Distribution (First-Class Third-Party) & the Asset Problem

> Part of the [Junius → Elixir/Phoenix report](README.md). Forward-looking design, backed by
> targeted research (sources cited inline).

**Decision recap:** third-party plugins must be **first-class**, including plugins that ship
their own **static assets** (JS hooks, CSS, images). This file answers: *are Hex packages
enough, or is a "starter-script / ad-hoc project" step needed?*

## 1. Short answer

**Hex packages are sufficient to _transport_ everything a plugin needs — Elixir code,
migrations, LiveViews, and static assets under `priv/` — but they are _not_ sufficient, by
themselves, to _compose the frontend_ of a host that gains new CSS/JS from plugins chosen at
runtime.** The reason is a hard, framework-level constraint: **Tailwind emits only the classes it
sees by scanning source files at build time, and esbuild bundles at build time**; a Mix release
ships a frozen `priv/static` and never runs the Node/Tailwind/esbuild toolchain at boot. The
moment a plugin contributes new Tailwind classes or new bundled JS, *something must run a build
with that plugin's files present.*

So the requester's **"starter-script / ad-hoc project" intuition is essentially correct** — for a
plugin set known at deploy time, a **deploy-time assembler** is the pragmatic, best-quality
answer. A no-rebuild "add a plugin to an already-shipped host" story is possible only with a
weaker option (self-served precompiled bundles), at the cost of a shared design system.

## 2. What Hex fully solves (transport)

- **`priv/` ships by default.** Hex's default file set includes `priv`, so any JS/CSS/images/
  precompiled bundles under `priv/` are published automatically (re-add `"priv"` if you override
  `:files`). Source: <https://hexdocs.pm/hex/Mix.Tasks.Hex.Build.html>.
- **Size cap:** ~16 MiB compressed / 128 MiB uncompressed (hex_core default; the widely-cited
  "8 MB" is stale). Source: <https://github.com/hexpm/hex_core>.
- **Runtime access to a dep's `priv/`:** `Application.app_dir(:dep, "priv")` / `:code.priv_dir/1`.
- **`Plug.Static` can serve a dependency's `priv/static` directly** — no copy into the host —
  via `from: {:dep_app, "priv/static"}` (the tuple form is required in releases). You can stack
  one `Plug.Static` per plugin under distinct `:at` prefixes. Source:
  <https://hexdocs.pm/plug/Plug.Static.html>.
- **The catch — cache-busting.** `mix phx.digest` operates on a *single* `priv/static`, and the
  endpoint reads *one* `cache_manifest.json`. There's **no built-in way to merge multiple deps'
  digest manifests**, so a dep either digests into the host's static dir at build time or owns
  its own cache-busting. Sources: <https://hexdocs.pm/phoenix/Mix.Tasks.Phx.Digest.html>,
  <https://hexdocs.pm/phoenix/Phoenix.Endpoint.html>.

**Transport is solved.** The hard part is never "ship the file" — it's "get the file's classes/
modules into a coherent bundle."

## 3. Where "just a Hex dep" breaks down (build-time composition)

- **esbuild importing a dep's JS — solved by `NODE_PATH`.** Phoenix's esbuild profile sets
  `NODE_PATH` to include `deps` (and, in Phoenix 1.8 / LiveView 1.1, the `_build` dir), so
  `import` from a dependency resolves without an npm install. The default profile does *not*
  support esbuild plugins. Source: <https://github.com/phoenixframework/esbuild>.
- **Tailwind content globs must include the dep's templates at build time.** Tailwind v4:
  `@source "../deps/<dep>/**/*.*ex";` in `app.css`. **Gotcha:** Tailwind v4 `@source` wildcards
  **respect `.gitignore`**, so a single wildcard over `deps/` silently skips gitignored plugins —
  you must emit **one explicit `@source` line per plugin**. Sources:
  <https://tailwindcss.com/docs/detecting-classes-in-source-files>,
  <https://github.com/phoenixframework/tailwind>.
- **LiveView colocated hooks cross the dependency boundary — but with a caveat.**
  `Phoenix.LiveView.ColocatedHook`/`ColocatedJS` (LiveView 1.1, needs Phoenix 1.8+) extract inline
  `<script :type={...}>` JS at *compile time* into `_build/.../phoenix-colocated/<app>/`, one
  folder per app, importable because `NODE_PATH` includes `_build`. Official guidance is that
  *libraries* should pre-bundle their hooks into a published file rather than rely on host
  pickup. So this helps, but it's still resolved by the **host's build**, not at runtime. Source:
  <https://hexdocs.pm/phoenix_live_view/Phoenix.LiveView.ColocatedJS.html>.
- **Umbrella vs external dep** — an umbrella shares `deps/`/`_build/`, so the same `NODE_PATH`
  and `phoenix-colocated/<app>` layout apply; you widen the Tailwind `@source` globs to sibling
  apps. Same build-time scanning problem, relocated.

**Conclusion:** every mechanism that gets a plugin's JS/CSS into the final bundle (`NODE_PATH`,
Tailwind globs, colocated hooks) is a **build-time** operation run against the host.

## 4. Runtime vs build-time — the unavoidable tension

The BEAM loads compiled *bytecode* at runtime, and loading a plugin's OTP app runs only its
Erlang side — it **never invokes Node/esbuild/Tailwind**, which typically aren't even present in a
production release (Phoenix docs suggest *removing* the esbuild/tailwind deps for a pure-runtime
deploy). A Mix release ships a frozen `priv/static` and loads only the digest manifest at boot.
Sources: <https://hexdocs.pm/phoenix/asset_management.html>,
<https://hexdocs.pm/phoenix/releases.html>.

**Direct answer:** first-class runtime third-party plugins that carry custom CSS/JS
**fundamentally require a per-deployment asset build step** — *unless* you push the build into the
plugin (self-served precompiled bundles, §6) or move CSS compilation to runtime (Beacon, §5). No
configuration lets a stock precompiled host absorb a new plugin's Tailwind classes for free.

## 5. Prior art (everyone hits this wall)

- **Bonfire — the direct precedent, and it's build-time.** Extensions are separate repos pulled
  as deps, assembled by a home-grown **`mess`** tool into "flavours." A hub extension
  (`bonfire_ui_common`) bundles all extensions' JS via esbuild reaching into sibling deps by
  path, and a **codegen Mix task** (`mix bonfire.gen_tailwind_sources`) enumerates every
  extension and **emits one explicit `@source` per extension** into a gitignored file —
  *precisely* to dodge the v4 `.gitignore` wildcard gotcha. Sources:
  <https://docs.bonfirenetworks.org/>, <https://github.com/bonfire-networks/mess>.
- **Beacon CMS — the runtime-compile outlier.** Pages are *data* authored at runtime, so a
  build-time purge can't see them; it compiles Tailwind **at runtime** per site
  (`Beacon.RuntimeCSS.TailwindCompiler`). Only model for adding unknown-at-deploy styles with no
  rebuild. Source: <https://hexdocs.pm/beacon/>.
- **AshAdmin / Phoenix LiveDashboard — precompile & self-serve.** They compile their CSS/JS into a
  committed bundle at package-build time and serve it via their **own** route with a content-hash
  URL + `cache-control: immutable`, bypassing the host's Tailwind build and digest entirely.
  Sources: <https://github.com/ash-project/ash_admin>,
  <https://github.com/phoenixframework/phoenix_live_dashboard>.
- **Backpex / Petal — push globs into the host build.** The host manually adds
  `@source "../deps/<lib>/**/*.*ex"` and merges hooks. Perfect Tailwind sharing, but N manual
  host steps — doesn't scale to *unknown* plugins.

The pattern is unmistakable: every extensible Phoenix system either **aggregates plugin assets in
one build** (Bonfire/Backpex/Petal), **precompiles-and-self-serves per plugin** (AshAdmin/
LiveDashboard), or **compiles at runtime** (Beacon). Nobody auto-discovers plugin *frontend*
assets purely at runtime from a stock release.

## 6. The three viable strategies

### 6.1 Deploy-time assembler (recommended default)

For a plugin set known at deploy time (the realistic single-tenant-release case):

1. Each plugin is a normal **Hex package** (code + migrations + `priv/` assets + colocated JS).
2. A deploy-time script reads the enabled-plugin config and **generates** `mix.exs` deps and —
   critically — the **explicit per-plugin Tailwind `@source` lines** and the **esbuild JS entry
   imports**. **`igniter`** is the purpose-built tool (AST-based `mix.exs`/config patching:
   `Igniter.Project.Deps.add_dep/3`, `mix igniter.new --with phx.new`), or a `mess`-style deps
   file. Sources: <https://github.com/ash-project/igniter>,
   <https://hexdocs.pm/igniter/>.
3. Run **one** `mix assets.deploy` (Tailwind + esbuild + `phx.digest`), then `mix release`.

Result: **one shared design system, deduped/tree-shaken CSS, unified cache-busting** — the best
output, and exactly what Bonfire achieves, just automated. Tradeoff: add/remove a plugin =
rebuild + redeploy; the build host needs the (Node-free) esbuild/tailwind binaries.

### 6.2 Self-served precompiled bundle (escape hatch)

Only if a plugin must be added to an already-shipped host with **no rebuild**: the plugin ships a
precompiled bundle in its own `priv/static` and serves it via its own hash-in-URL route (the
LiveDashboard/AshAdmin pattern). Cost: **duplicated CSS, no shared Tailwind base / design tokens,
possible `@layer` collisions.** This is the price of runtime-addability.

### 6.3 Runtime Tailwind compilation (special case only)

The Beacon model — reserve for content authored at runtime, not for plugin packaging. It's real
infrastructure, not a convenience. (Import-maps/ESM via `phoenix_importmap` can drop the
*bundler* but not the composition problem, and the library is immature.)

## 7. Recommendation

**Adopt both, in priority order:**

1. **Default: the deploy-time assembler (§6.1).** Best quality, shared design system, and it
   matches the app's single-tenant release model. Pair it with a **shared UI OTP app**
   (`junius_ui`, the `@junius/design` analog) that owns the Tailwind base + core components +
   tokens, so plugins carry minimal assets and the design system stays coherent.
2. **Documented option: self-served bundles (§6.2)** for runtime-added / opaque third-party
   plugins whose authors accept visual independence.

**Document plainly** in the plugin-authoring guide: a stock precompiled host **cannot** absorb a
new plugin's Tailwind classes "for free"; either the plugin joins the assembler build (shared
design system) or it self-serves a precompiled bundle (independent styling).

## 8. Bottom line

Hex is **necessary and sufficient for packaging** first-class plugins with assets, but **not
sufficient for frontend composition**. Because Tailwind and esbuild are inherently build-time,
first-class runtime plugins with custom assets need either a **per-deployment build step** (best
result — the assembler) or **per-plugin precompiled self-served bundles** (runtime-addable, at
the cost of a shared design system). This is a real constraint, not an implementation gap — it
falls out of how the Phoenix asset pipeline works.

Continue to [07 — Mapping & tradeoffs](07-mapping-and-tradeoffs.md).
