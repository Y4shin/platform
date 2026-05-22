# M05 — Handoff Document

A previous Claude session was implementing **M05 — Connect-RPC End-to-End**
for the Junius platform. The work is ~85% complete but **not committed yet**.
This file is a self-contained handoff so a new agent can pick up exactly
where the previous one stopped.

When M05 is finished and committed, delete this file.

---

## Project context (1-minute orientation)

- **Project**: Junius. A plugin-driven monolith for political work
  (Rust + Axum backend, React + TanStack + Vite frontend, all composed by
  a CLI named `junius`). See [docs/design/](docs/design/) for the
  architecture and [docs/impl/](docs/impl/) for the milestone-by-milestone
  build plan.
- **Repo state before M05**: M00 → M04 done, committed, pushed.
  `git log --oneline -5` shows the chain. The platform serves an embedded
  React SPA, the `hello` plugin contributes an HTTP `/h/hello/ping` route
  and a frontend page, and `junius sync` wires everything together.
- **M05 milestone doc**: [docs/impl/06-M05-connect-rpc.md](docs/impl/06-M05-connect-rpc.md).
  Adds Connect-RPC end-to-end: proto → Rust server stubs → TS client →
  Connect-Query in the FE. End of M05 = **v0 cut**: every later milestone
  adds depth, not new shapes.

## Implementation plan (what's being built)

> Note: the previous session's working notes lived in
> `~/.claude/plans/i-want-you-to-structured-matsumoto.md`, but that file
> is **not** accessible to a new agent. Everything material from it is
> reproduced in this handoff.

Key decisions confirmed with the user before implementation began:

1. **URL scheme**: flat `/rpc/<pkg>.<Service>/<Method>` (no per-plugin path
   prefix — the proto package name does the scoping). Connect-Web baseUrl
   is `/rpc`. This deviates slightly from the M05 doc's `/rpc/<plugin>/`
   wording — should be reflected in the M05 doc's status block when it's
   marked done.
2. **Connect protocol layer**: lives inside `crates/junius-sdk` as a new
   `rpc` module. Hand-rolled (~150 LoC); the only crates.io options for
   Connect-RPC Rust servers are too early/niche to depend on.
3. **Codecs**: both `application/proto` (binary, Connect-Web's browser
   default) and `application/json` (curl-friendly). Plain serde derives
   via `prost-build`'s `type_attribute` — strict proto3-JSON spec
   compliance deferred.
4. **Buf delivery**: via `@bufbuild/buf` npm package (root devDependency).
   `pnpm exec buf` is the canonical invocation. `nix develop` also still
   provides `pkgs.buf` as a backstop.
5. **Rust proto codegen**: `prost-build` invoked from each plugin's
   `build.rs`. Output lands in `OUT_DIR`. `protoc` added to the Nix
   flake (it's a runtime dep of `prost-build` 0.14).
6. **TS proto codegen**: `@bufbuild/protoc-gen-es` v2 emits both message
   types and service descriptors in one pass. The older
   `@connectrpc/protoc-gen-connect-es` is no longer needed.
7. **Plugin trait**: gains a `fn rpc_routes(...) -> Router` default
   method. Plugins override when they expose RPCs. Host nests
   `plugin.routes()` under `/h/<plugin>` and `plugin.rpc_routes()` under
   `/rpc` (flat).

### Why we hand-rolled the Connect-RPC server (don't undo this)

The previous agent surveyed the crates.io ecosystem before deciding to
hand-roll. The only candidates were:

- `connect-rpc = "0.1.0"` — early, default-feature is `reqwest`
  (client-focused), no production usage.
- `scion-sdk-axum-connect-rpc = "0.5.2"` — niche maintainer (Anapaya, a
  SCION networking project), minimal API surface, version <1.0.

Both carry meaningful churn risk for a project that's planning to layer
M07 (permission interceptor), M09 (narrow per-plugin namespaces), and
M10 (telemetry hooks) on top of the RPC layer. The Connect unary HTTP
protocol is small enough that we own ~200 LoC of protocol code rather
than pin a fragile dependency. If a more robust Connect server crate
emerges later (e.g. from the Connect project itself), the
`junius_sdk::rpc` module's surface is small enough to swap behind.

The TS side has no such concern — `@connectrpc/connect-web` and
`@connectrpc/connect-query` are first-party Connect packages and the
Connect spec's reference client implementations.

## What's done (commit-ready, but uncommitted)

All quality gates green in the current uncommitted state:
- `cargo fmt --check` ✓
- `cargo clippy --workspace --all-targets -- -D warnings` ✓
- `cargo clippy -p platform --features embed-frontend --all-targets -- -D warnings` ✓
- `cargo test --workspace` → **99 Rust tests pass** (up from 76 in M04)
- `pnpm exec buf lint` ✓
- `pnpm exec buf format --diff` ✓
- `pnpm run typecheck` ✓
- `pnpm --filter @junius/shell build` ✓ (Vite production build)
- `pnpm test` (vitest in @junius/sdk) ✓
- `pnpm exec biome ci` — **not yet re-run**, see TODO below

### Workspace dependencies

- **Rust** (`Cargo.toml` root): added `prost = "0.14"`, `prost-build =
  "0.14"`, `bytes = "1"` to `[workspace.dependencies]`.
- **npm root** (`package.json`): added `@bufbuild/buf` and
  `@bufbuild/protoc-gen-es` (both `^1/^2` latest) as devDependencies.
- **`platform/frontend/package.json`**: added `@bufbuild/protobuf`,
  `@connectrpc/connect`, `@connectrpc/connect-query`,
  `@connectrpc/connect-web`, `@junius/generated`.
- **`plugins/hello/frontend/package.json`**: added `@bufbuild/protobuf`,
  `@connectrpc/connect-query`, `@junius/generated`.
- **`packages/generated/package.json`**: real `exports` map now —
  `.`, `./*/rpc → ./src/plugins/*/rpc.ts`, `./proto/* →
  ./src/proto/*`. Added `@bufbuild/protobuf` as peer + devDep.
- **`crates/junius-sdk/Cargo.toml`**: added `prost`, `bytes`, `http`,
  `serde_json`. dev-deps: `tokio`, `tower` for the in-process tests.
- **`plugins/hello/Cargo.toml`**: deps gained `prost`, `serde`; new
  `[build-dependencies] prost-build`; dev-deps gained `serde_json`.
- **`platform/Cargo.toml`**: dev-deps gained `prost`, `serde_json`.

`pnpm install` has been run and built scripts (`esbuild`, `@bufbuild/buf`)
approved via `pnpm-workspace.yaml`'s `allowBuilds:` block.

### Nix flake

- `flake.nix` adds `pkgs.protobuf` to the dev shell so `prost-build`
  finds `protoc`. The shellHook also prints `protoc --version`.

### `crates/junius-sdk/src/rpc/`

New module — the Connect-RPC unary protocol implementation. Three files:

- `error.rs` — `RpcCode` enum (full Connect status set), `RpcError`
  struct, `IntoResponse` impl that emits `{"code":"snake_case",
  "message":"..."}` with the right HTTP status. 3 unit tests.
- `codec.rs` — `Codec` enum (`Proto` | `Json`), `from_headers` parses
  `Content-Type` (defaults to `Proto`), `decode_request` / `encode_response`
  for round-tripping prost messages. 6 unit tests covering each
  content-type variant + unknown-type rejection.
- `builder.rs` — `ServiceBuilder::new(type_name).unary(method, handler).into_router()`.
  Each `.unary` registers a POST route at `/<type_name>/<method>`. Generic
  dispatcher reads the codec from headers, decodes into the prost+serde
  message, awaits the handler, encodes the response, sets the response
  `Content-Type` to match. 6 integration tests via `tower::ServiceExt::oneshot`
  with hand-rolled `prost::Message` types (avoids dragging prost-build
  into junius-sdk's test build).
- `mod.rs` — re-exports `RpcResult`, `RpcError`, `RpcCode`, `ServiceBuilder`.

`crates/junius-sdk/src/lib.rs` adds `pub mod rpc;` and the existing
`pub use` block.

### Plugin trait

`crates/junius-sdk/src/plugin.rs` gains:

```rust
fn rpc_routes(&self, _resources: PluginResources) -> Router {
    Router::new()
}
```

Default no-op. Existing M04 code path unchanged.

### Host wiring

`platform/src/server.rs::build_app` now also merges every plugin's
`rpc_routes()` and nests the result under `/rpc`. The base SPA fallback
still owns paths not matched by `/h/*`, `/rpc/*`, or `/api/*`.

### Proto + buf workspace

- Removed `buf.work.yaml` (v1 layout).
- New `buf.yaml` at workspace root (v2 layout) declares two modules:
  `proto` and `plugins/hello/proto`. Lint = STANDARD; breaking = FILE.
- New `buf.gen.yaml`: single plugin `local: ["pnpm", "exec",
  "protoc-gen-es"]`, output to `packages/generated/src/proto`, opts
  `target=ts` + `import_extension=js`. No `inputs:` block — `buf
  generate` defaults to every workspace module.
- New `proto/platform/v1/annotations.proto`: declares
  `extend google.protobuf.MethodOptions { optional string requires = 60001; }`.
  Used by M07's interceptor, declared here so the field number stays
  stable.
- New `plugins/hello/proto/hello/v1/hello.proto`: defines
  `service HelloService { rpc Greet(GreetRequest) returns (GreetResponse); }`
  plus the two scalar-only message types.

### Hello plugin RPC

- New `plugins/hello/build.rs`: invokes `prost-build` with
  `.type_attribute(".", "#[derive(serde::Serialize,
  serde::Deserialize)]")` so the same prost messages can be encoded both
  as protobuf binary and JSON.
- `plugins/hello/src/lib.rs`:
  - `mod pb { include!(concat!(env!("OUT_DIR"), "/hello.v1.rs")); }`
    pulls in the prost-build output. Module renamed from `gen` to `pb`
    because `gen` is reserved in Rust 2024 edition.
  - `pub use pb::{GreetRequest, GreetResponse}` — exposed so integration
    tests can construct messages.
  - `async fn HelloPlugin::greet(req)` — pure business logic. Empty
    `name` defaults to `"world"`. Decorated with
    `#[allow(clippy::unused_async, reason = "...")]` because the body
    doesn't await but the handler signature requires `async`.
  - `Plugin::rpc_routes()` impl returns
    `ServiceBuilder::new("hello.v1.HelloService").unary("Greet",
    Self::greet).into_router()`.

### junius sync extension

`tools/junius/src/commands/sync.rs::run` writes one additional file per
enabled plugin: `packages/generated/src/plugins/<name>/rpc.ts` — a barrel
that re-exports `'../../proto/<name>/v1/<name>_pb.js'`. M09 narrows the
barrel based on `[dependencies.<dep>].rpc_methods`; for M05 it
`export *`s the whole service.

The `render_rpc_barrel` helper sits right above `render_component_registry_ts`.

`junius sync` does **not** yet invoke `pnpm exec buf generate`
automatically. The plan called for it but it's deferred — see
"What's left" below. `pnpm exec buf generate` produces
`packages/generated/src/proto/hello/v1/hello_pb.ts` and is currently
expected to be run manually before `pnpm install` / `vite build`.

### Frontend

- `platform/frontend/src/main.tsx`: wrapped existing providers with
  `TransportProvider transport={createConnectTransport({ baseUrl:
  '/rpc' })}`.
- `plugins/hello/frontend/src/routes/pages/HelloPage.tsx`: replaced
  `fetch('/h/hello/ping')` with `useQuery(HelloService.method.greet,
  { name })` imported from `@junius/generated/hello/rpc`. The /ping HTTP
  route is still served by the backend — it's just no longer called by
  the FE.

### Tests added

- `plugins/hello/tests/hello_rpc.rs` — 6 tests:
  binary round-trip, JSON round-trip, empty-name defaults to "world",
  unknown method 404s, HTTP /ping still works, etc.
- `platform/tests/boot_with_hello.rs` — extended with 3 new tests:
  `rpc_route_mounted_under_slash_rpc`, `rpc_route_via_real_bind_and_reqwest_json`,
  `missing_plugin_rpc_route_404s`.
- `tools/junius/tests/sync.rs` — extended `sync_with_hello_writes_expected_files`
  with a new snapshot assertion on the rpc.ts barrel.
- New insta snapshot file:
  `tools/junius/tests/snapshots/sync__rpc_barrel_hello.snap`.

### Documented end-to-end smoke (works locally now)

```bash
# Apply sync (writes the barrel + everything else):
target/debug/junius sync --config dev/platform.toml

# Build the host + hello plugin (prost-build runs automatically):
cargo build -p platform

# Start the host:
target/debug/juniusd &

# JSON RPC works:
curl -fsS -X POST http://127.0.0.1:18080/rpc/hello.v1.HelloService/Greet \
  -H 'Content-Type: application/json' \
  -d '{"name":"alice"}'
# → {"message":"Hello, alice!"}

# Backend HTTP also still works:
curl -fsS http://127.0.0.1:18080/h/hello/ping
# → pong
```

For the FE: `pnpm install` + `pnpm exec buf generate` + `pnpm --filter
@junius/shell build` produces a working SPA bundle.
`target/debug/junius build --config dev/platform.toml` then
`target/debug/juniusd` serves the embedded SPA at `/p/hello` and the
page successfully calls Greet via Connect-Query.

## What's left to finish M05

### 1. Decision: should `junius sync` auto-run `buf generate`?

The plan called for `junius sync` to invoke `pnpm exec buf generate` so
the developer doesn't have to remember a separate step. **Currently
this is not implemented.** Reasons to defer:

- Inside the test tempdirs (`tools/junius/tests/sync.rs`) `pnpm` isn't
  on PATH and there's no `buf.yaml`, so the sync would either fail or
  need a `--skip-buf-generate` flag.
- It changes sync into a network/install-touching command in some
  contexts (first-time `@bufbuild/buf` download).

A pragmatic compromise: have sync detect `pnpm-workspace.yaml` at the
cwd and only invoke buf when present. The next agent should decide.
For now the workflow is: developers run `pnpm exec buf generate`
manually whenever they change a `.proto` file, the same way they'd
run `cargo` after editing Rust.

### 2. Biome lint pass on the new TS

`pnpm exec biome ci` hasn't been re-run after the FE edits. The new
files are `platform/frontend/src/main.tsx` (modified) and
`plugins/hello/frontend/src/routes/pages/HelloPage.tsx` (rewritten).
Biome may flag formatting; run `pnpm exec biome check --write .` if so.

### 3. CI extension

`.github/workflows/ci.yml` should gain two new steps inside the existing
`ci` job, ideally just after the existing biome step:

```yaml
- name: Buf — lint
  run: nix develop --command pnpm exec buf lint

- name: Buf — format check
  run: nix develop --command pnpm exec buf format --diff
```

`buf breaking --against '.git#branch=main'` is **deferred to M06** — we
don't yet have a stable proto on main to compare against.

### 4. Mark M05 status in the docs

Three files need updating (see how M04's status updates were done — git
log for `483224f` / `9e1963a`):

- `docs/impl/06-M05-connect-rpc.md`: change
  `> **Status:** 🚧 Planned.` to `> **Status:** ✅ Implemented` near
  the top. Add a note explaining the URL-scheme deviation: the doc
  proposes `/rpc/<plugin>/<service>/<method>` but the impl uses flat
  `/rpc/<service>/<method>` (proto package names provide plugin
  scoping). The deviation was confirmed with the user.
- `docs/impl/README.md`: change the M05 row from `🚧` to `✅` in the
  milestone index table.
- `docs/impl/15-open-questions-resolution.md`: the "v0 milestone
  definition" row should be marked as **reached** at end of M05. Add a
  short note that M05 closed out the v0 cut.

### 5. Commit + push

Commit message convention follows the existing chain. Suggested:

```
M05: Connect-RPC + buf workspace + v0 cut reached

junius_sdk::rpc — hand-rolled Connect HTTP unary protocol layer
...
```

See M04's commit (`9e1963a`) for the format. Then `git push origin main`.

## Gotchas encountered (so you don't trip over the same ones)

These came up during the previous session and the fix is already in the
codebase. Noted so a new agent doesn't burn time re-discovering them.

- **`gen` is a reserved keyword in Rust 2024 edition.** The conventional
  Rust idiom of `mod gen { include!(...); }` for prost-build output
  doesn't compile. We renamed the module to `pb` in
  `plugins/hello/src/lib.rs`. Any future plugin doing the same
  prost-build dance should use `pb` (or `proto`) too.
- **`prost-build` 0.14 requires `protoc` on PATH.** Solved by adding
  `pkgs.protobuf` to `flake.nix`. Don't try to swap to
  `protoc-bin-vendored` — adds a build-time download step.
- **`pnpm 11` requires explicit approval for install scripts.** The
  `@bufbuild/buf` package downloads the right buf binary in a
  `postinstall` script. Already approved in `pnpm-workspace.yaml`
  (`allowBuilds: '@bufbuild/buf': true`). If you add another npm dep
  with install scripts, you'll need `pnpm approve-builds --all`.
- **`buf` v2 vs v1 layout.** The workspace was on v1 (`buf.work.yaml` +
  per-module `buf.yaml`). M05 moved to v2 (single root `buf.yaml` with
  `modules:` list). `buf.work.yaml` was deleted. Don't recreate it.
- **`buf.gen.yaml` `inputs:` block must use `module:` not `path:`.**
  The simplest valid config drops the `inputs:` block entirely — `buf
  generate` defaults to "every module declared in buf.yaml". That's
  what we did.
- **Connect-Web URL convention is `<baseUrl>/<typeName>/<method>`** —
  no per-plugin prefix. The host nests `rpc_routes()` under `/rpc` flat,
  and proto package names (`hello.v1`, `speakers.v1`, …) do the plugin
  scoping. Don't try to add `/rpc/<plugin>/` — it works against the
  Connect-Web client's natural URL builder and would require either
  per-plugin transports (many `createConnectTransport` calls) or
  custom URL munging.
- **prost-build's `type_attribute` adds attributes to ALL types** —
  `type_attribute(".", "...")` matches every type via the proto path
  selector `.`. For scalar-only messages (our hello plugin) that's fine.
  When a future plugin uses `bytes`, `uint64`, or well-known types,
  serde derives might not match Connect-Web's strict proto3-JSON
  output; the fix is either narrower `type_attribute` selectors or
  switching to `prost-wkt`.
- **Connect-Web's default browser codec is binary protobuf.** Even
  though both codecs work server-side, the FE will send `Content-Type:
  application/proto`. `curl` testing benefits from explicit
  `application/json`. Don't assume JSON-only.
- **Test crates that need `serde_json` must list it explicitly.** It's
  a workspace dep but not transitively inherited. The previous session
  hit this when extending `plugins/hello/tests/hello_rpc.rs` and
  `platform/tests/boot_with_hello.rs` — both got `serde_json` added to
  their `[dev-dependencies]`.

## Risks for the remaining work

- **Auto-running `buf generate` from `junius sync`** (item 1 in
  "What's left"). The tempdir sync tests at `tools/junius/tests/sync.rs`
  don't have `pnpm`, `@bufbuild/buf`, or any `buf.yaml` — they assert on
  the Rust-side outputs only. If you wire sync to invoke buf, you'll
  need to:
  - Detect whether buf is reachable (try `pnpm exec buf --version` ↔
    check for `node_modules/.bin/buf`, or look for the binary on PATH).
  - Skip with a warning when missing, so the test tempdirs still pass.
  - Add a `--require-codegen` flag for CI-style strict mode (M09 polish).
  Alternatively: defer the auto-invocation entirely until M09 when
  cross-plugin sync logic lands. Document the manual `pnpm exec buf
  generate` step in the plugin-authoring guide instead.
- **Biome may flag the new TS.** `pnpm exec biome ci` hasn't been
  re-run since `main.tsx` and `HelloPage.tsx` were edited. If it
  complains, `pnpm exec biome check --write .` auto-fixes formatting +
  import sorting. Verify no real lint regressions after the auto-fix.
- **`buf breaking` not yet in CI.** Adding it now would fail on every
  proto edit until we have a stable proto baseline. Defer to M06 (or
  later) when the proto surface is stable for a release.
- **The `_pb.js` import extension.** `buf.gen.yaml` is configured with
  `import_extension=js`. This means generated TS files import each
  other as `./foo_pb.js` (not `.ts`). With `tsconfig.base.json`'s
  `moduleResolution: "bundler"`, Vite and TSC both handle this fine.
  Don't change to `import_extension=none` — it breaks Vite's resolver.
- **`@junius/generated` exports use a glob pattern** (`./*/rpc` →
  `./src/plugins/*/rpc.ts`). pnpm + Node both support glob exports in
  modern versions; if you see resolution failures, double-check
  `pnpm install` was re-run after any change to that package's
  `package.json`.

## Out of scope (per the milestone doc — confirm with user before changing)

- **Permission enforcement on RPCs.** `option (platform.requires)` is
  declared in `annotations.proto` but not enforced. Lands in M07.
- **Narrow per-plugin RPC namespace.** Barrel re-exports the full
  service. M09 narrows per `[dependencies.<dep>].rpc_methods`.
- **Streaming RPCs.** Unary only. `ServiceBuilder` has no `.server_streaming`
  / `.client_streaming` / `.bidi_streaming` methods.
- **`buf breaking` in CI.** Deferred to M06.
- **proto3-JSON spec compliance.** Plain serde derives; covers
  scalar-only messages, will need `prost-wkt` for well-known types later.
- **`junius dev` file-watcher for protos.** Polish for a later milestone.

## How to verify the current state

Run inside the dev shell (`nix develop`):

```bash
# Lints + fmt
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p platform --features embed-frontend --all-targets -- -D warnings

# Rust tests (expect 99 passes)
cargo test --workspace

# Buf
pnpm exec buf lint
pnpm exec buf format --diff
pnpm exec buf generate          # writes packages/generated/src/proto/

# TS
pnpm install                    # already up to date
pnpm run typecheck
pnpm --filter @junius/shell build
pnpm test                       # vitest in @junius/sdk

# End-to-end RPC
target/debug/junius sync --config dev/platform.toml
target/debug/juniusd > /tmp/juniusd.log 2>&1 &
sleep 1
curl -fsS -X POST http://127.0.0.1:18080/rpc/hello.v1.HelloService/Greet \
  -H 'Content-Type: application/json' -d '{"name":"alice"}'
# → {"message":"Hello, alice!"}
kill -INT $!

# Standalone binary smoke (embedded SPA + RPC)
target/debug/junius build --config dev/platform.toml
target/debug/juniusd &
# Visit http://localhost:18080/p/hello → page renders, calls Greet, displays "Hello, Dev User!"
kill -INT $!
```

## Files touched in this milestone (uncommitted)

**Created:**
- `crates/junius-sdk/src/rpc/{mod,builder,codec,error}.rs`
- `proto/platform/v1/annotations.proto`
- `plugins/hello/proto/hello/v1/hello.proto`
- `plugins/hello/build.rs`
- `plugins/hello/tests/hello_rpc.rs`
- `buf.yaml`
- `buf.gen.yaml`
- `packages/generated/src/plugins/hello/rpc.ts` (written by junius sync)
- `packages/generated/src/proto/hello/v1/hello_pb.ts` (written by buf generate)
- `packages/generated/src/proto/platform/v1/annotations_pb.ts` (written by buf generate)
- `tools/junius/tests/snapshots/sync__rpc_barrel_hello.snap`
- `M05-HANDOFF.md` (this file)

**Modified:**
- `Cargo.toml` (workspace.dependencies)
- `crates/junius-sdk/Cargo.toml` (rpc-module deps)
- `crates/junius-sdk/src/lib.rs` (`pub mod rpc;`)
- `crates/junius-sdk/src/plugin.rs` (rpc_routes default method)
- `platform/Cargo.toml` (test deps)
- `platform/src/server.rs` (mount /rpc)
- `platform/tests/boot_with_hello.rs` (extended)
- `plugins/hello/Cargo.toml` (proto deps + build-deps + test deps)
- `plugins/hello/src/lib.rs` (mod pb + rpc_routes + greet handler)
- `plugins/hello/frontend/package.json` (Connect deps)
- `plugins/hello/frontend/src/routes/pages/HelloPage.tsx` (useQuery)
- `platform/frontend/package.json` (Connect deps)
- `platform/frontend/src/main.tsx` (TransportProvider)
- `package.json` (root devDeps for @bufbuild/buf + protoc-gen-es)
- `packages/generated/package.json` (real exports map)
- `tools/junius/src/commands/sync.rs` (rpc barrel writer)
- `tools/junius/tests/sync.rs` (snapshot for rpc barrel)
- `flake.nix` (added pkgs.protobuf)
- `pnpm-workspace.yaml` (allowBuilds entry for @bufbuild/buf)
- `pnpm-lock.yaml` (regenerated)

**Deleted:**
- `buf.work.yaml` (replaced by root `buf.yaml` v2)

## Useful files in the repo (so you don't have to grep)

- The previous session's full M05 plan:
  `/home/pplattner/.claude/plans/i-want-you-to-structured-matsumoto.md`.
- Existing implementation plan for the whole project: `docs/impl/`.
  README has the milestone index with status emoji.
- Design docs: `docs/design/` (split numbered docs) and the historical
  monolithic dump `political-platform-design.md` at the repo root.
- M00-M04 commits (look at `git log --oneline -10` for context):
  `b54652f M00`, `4a8b441 M01`, `c30f5a4 M02`, `483224f M03`,
  `9e1963a M04`.

## When you're ready to ship M05

```bash
# Verify everything green:
nix develop --command bash -c '
  cargo fmt --check &&
  cargo clippy --workspace --all-targets -- -D warnings &&
  cargo clippy -p platform --features embed-frontend --all-targets -- -D warnings &&
  cargo test --workspace &&
  pnpm exec buf lint &&
  pnpm exec buf format --diff &&
  pnpm exec biome ci &&
  pnpm run typecheck &&
  pnpm test
'

# Stage everything (including the deleted buf.work.yaml):
git add -A

# Verify what you're about to commit:
git status --short
git diff --cached --stat | tail

# Commit (replace the message with something matching prior commits):
git commit -m "M05: Connect-RPC + buf workspace + v0 cut reached

Detailed body here..."

# Push:
git push origin main

# Then delete this handoff file:
rm M05-HANDOFF.md
git add -A && git commit -m "Remove M05 handoff doc" && git push
```

---

# Appendix: original M05 working plan (verbatim from the previous session)

The following is the unedited plan file the previous session worked from. The handoff sections above are the curated, current-state version; this appendix preserves the full original plan for cross-reference.

# M05 — Concrete Implementation Plan

## Context

[docs/impl/06-M05-connect-rpc.md](docs/impl/06-M05-connect-rpc.md) describes M05
conceptually. End of M05 is the **v0 cut**: a plugin contributes a Rust
backend, a TanStack-routed React page, **and** a strongly-typed RPC contract,
all composed by `junius`. After M05 every later milestone adds depth, not
new shapes.

This plan turns the milestone into concrete files and decisions. The
biggest contextual finding: there is no production-grade Connect-RPC server
crate for Rust. The current crates.io options are `connect-rpc = "0.1.0"`
(early, client-focused) and `scion-sdk-axum-connect-rpc = "0.5.2"` (niche
maintainer, minimal API surface). Both carry meaningful churn risk for a
long-running project. We'll **hand-roll a small `junius_sdk::rpc` module**
that implements the Connect HTTP unary protocol — ~150 LoC, full control
over future extension points (M07 interceptors, M09 narrow namespaces,
M10 telemetry hooks).

### Decisions confirmed

- **URL scheme**: flat `/rpc/<pkg>.<Service>/<Method>`. Connect-Web's natural
  URL convention; the proto package name does the plugin scoping. The
  design's `/rpc/<plugin>/` wording is reinterpreted as "RPC for plugin
  `hello` lives under `/rpc/hello.v1.*`", not "/rpc/hello/...". Note this
  deviation in the M05 status update.
- **Connect protocol layer location**: inside `crates/junius-sdk` as a new
  `rpc` module. One fewer crate to maintain; plugins already depend on
  `junius_sdk`.
- **Codecs**: both binary (`application/proto`) and JSON (`application/json`).
  Binary is what `@connectrpc/connect-web` uses by default; JSON makes
  curl-based debugging trivial. proto3-JSON spec edge cases (well-known
  types, uint64 as string, etc.) deferred — plain serde derives via
  `prost-build`'s `type_attribute` match Connect-Web's JSON output for
  every type our plugins use in M05.
- **Buf delivery**: via the `@bufbuild/buf` npm package (root devDependency).
  `pnpm exec buf` from the workspace root is the canonical invocation.
  Drops the dev-shell-only constraint and avoids the "vendor a tarball"
  complexity from the doc.
- **Rust proto codegen**: `prost-build` invoked from each plugin's
  `build.rs`. Generated code lands in `OUT_DIR`, included via
  `include!(concat!(env!("OUT_DIR"), "/<pkg>.rs"))`. Plugin source trees
  stay clean (mirrors the macro-based approach used by `plugin_metadata!`).
- **TS proto codegen**: `@bufbuild/protoc-gen-es` v2 — generates both
  message types and service descriptors in one pass. The older split with
  `@connectrpc/protoc-gen-connect-es` is no longer needed.

## Part 1 — Workspace additions

### Cargo.toml (root) — new workspace.dependencies

```toml
# Protocol buffers + Connect
prost       = "0.14"
bytes       = "1"
serde-bytes = "0.11"  # used by junius-sdk::rpc::codec for raw-bytes round-trip

# Build-time codegen (only used by build.rs scripts)
prost-build = "0.14"
```

No new workspace crates. `junius-sdk` grows a `rpc` module.

### Root devDependencies (pnpm)

```json
"@bufbuild/buf": "^1.50.0",
"@bufbuild/protoc-gen-es": "^2.5.0"
```

`@bufbuild/buf` is a wrapper that downloads the right buf binary for the
host platform on install. Invoked via `pnpm exec buf ...` from the
workspace root.

### platform/frontend dependencies

```json
"@connectrpc/connect": "^2.0.0",
"@connectrpc/connect-web": "^2.0.0",
"@connectrpc/connect-query": "^2.0.0",
"@bufbuild/protobuf": "^2.5.0"
```

These are the runtime libs the shell embeds — generated code from
`@bufbuild/protoc-gen-es` v2 depends on `@bufbuild/protobuf` at runtime.

### plugins/hello/frontend dependencies

Add `@junius/generated`. No other FE deps needed — Connect-Query hooks are
imported from the shell's transport context.

## Part 2 — Proto + buf setup

### Buf workspace layout

Use buf v2 layout (single root `buf.yaml` with `modules`). Drop the
existing v1 `buf.work.yaml`:

```yaml
# buf.yaml (workspace root, v2)
version: v2
modules:
  - path: proto
    name: buf.build/junius/platform
  - path: plugins/hello/proto
    name: buf.build/junius/hello
lint:
  use:
    - STANDARD
breaking:
  use:
    - FILE
```

`buf.work.yaml` is removed. v2 is the supported path going forward and the
single-file config is less ceremony for the small number of modules we have.

### buf.gen.yaml (workspace root)

```yaml
version: v2
plugins:
  - local: pnpm exec protoc-gen-es
    out: packages/generated/src/proto
    opt:
      - target=ts
      - import_extension=js
inputs:
  - directory: proto
  - directory: plugins/hello/proto
```

`protoc-gen-es` v2 generates everything (message types + service
descriptors). One pass, one plugin.

### Proto sources

**`proto/platform/v1/annotations.proto`** — declared now (used in M07):

```proto
syntax = "proto3";
package platform.v1;
import "google/protobuf/descriptor.proto";

// Comma-separated permission keys (`<plugin>:<segment>`) required to call
// the annotated RPC method. Enforcement lands in M07 (host interceptor);
// the option is declared here so the field number stays stable and
// methods can reference it without a future schema-break.
extend google.protobuf.MethodOptions {
  optional string requires = 60001;
}
```

**`plugins/hello/proto/hello/v1/hello.proto`**:

```proto
syntax = "proto3";
package hello.v1;

service HelloService {
  rpc Greet(GreetRequest) returns (GreetResponse);
}

message GreetRequest  { string name = 1; }
message GreetResponse { string message = 1; }
```

`option (platform.requires) = "..."` is intentionally absent at M05 — the
permission system arrives in M07.

## Part 3 — `crates/junius-sdk::rpc` module

```
crates/junius-sdk/src/rpc/
├── mod.rs            # re-exports
├── builder.rs        # ServiceBuilder + UnaryHandler trait wiring
├── codec.rs          # Codec enum + encode/decode for binary + JSON
└── error.rs          # RpcError + RpcCode + IntoResponse impl
```

### Public surface (re-exported from `junius_sdk::rpc`)

```rust
pub use builder::ServiceBuilder;
pub use error::{RpcCode, RpcError};
pub type RpcResult<T> = Result<T, RpcError>;
```

### `RpcError` / `RpcCode`

`RpcCode` mirrors Connect's status set: `canceled, unknown,
invalid_argument, deadline_exceeded, not_found, already_exists,
permission_denied, resource_exhausted, failed_precondition, aborted,
out_of_range, unimplemented, internal, unavailable, data_loss,
unauthenticated`. Each maps to an HTTP status per the Connect spec.

`RpcError` implements `axum::response::IntoResponse` to emit Connect's
JSON error envelope:

```json
{ "code": "permission_denied", "message": "..." }
```

with `Content-Type: application/json` and the matching HTTP status.

### `ServiceBuilder`

```rust
pub struct ServiceBuilder { ... }

impl ServiceBuilder {
    pub fn new(type_name: &'static str) -> Self;

    pub fn unary<Req, Res, F, Fut>(
        self,
        method_name: &'static str,
        handler: F,
    ) -> Self
    where
        Req: prost::Message + serde::de::DeserializeOwned + Default + Send + 'static,
        Res: prost::Message + serde::Serialize + Send + 'static,
        F: Fn(Req) -> Fut + Clone + Send + Sync + 'static,
        Fut: std::future::Future<Output = RpcResult<Res>> + Send + 'static;

    pub fn into_router(self) -> axum::Router;
}
```

Each `.unary(...)` call registers a route at `"/{type_name}/{method_name}"`
that:

1. Reads `Content-Type` (default to `application/proto`).
2. Decodes the body via the matching codec into `Req`.
3. Invokes the handler.
4. Encodes the result via the same codec (binary or JSON), returns 200.
5. Maps `RpcError` to the JSON envelope.

### Codec module

```rust
pub(crate) enum Codec { Proto, Json }

pub(crate) fn decode_request<T>(codec: Codec, body: &[u8]) -> Result<T, RpcError>
where
    T: prost::Message + serde::de::DeserializeOwned + Default;

pub(crate) fn encode_response<T>(codec: Codec, value: &T) -> Result<(HeaderValue, Vec<u8>), RpcError>
where
    T: prost::Message + serde::Serialize;
```

`Codec::from_header(headers)` parses `Content-Type`; missing or unknown →
`Proto` (the Connect spec default).

### junius-sdk dependency additions

Add to `crates/junius-sdk/Cargo.toml`:

```toml
prost       = { workspace = true }
bytes       = { workspace = true }
serde       = { workspace = true }
serde_json  = { workspace = true }
```

## Part 4 — Plugin trait extension

Add to `crates/junius-sdk/src/plugin.rs`:

```rust
#[async_trait::async_trait]
pub trait Plugin: Send + Sync + 'static {
    fn metadata(&self) -> &'static PluginMetadata;
    fn routes(&self, resources: PluginResources) -> axum::Router;

    /// Connect-RPC services. Default no-op; plugins that expose RPCs
    /// override and return a router built via [`junius_sdk::rpc::ServiceBuilder`].
    /// The host merges these under `/rpc/` (no per-plugin path prefix —
    /// the proto package name does the namespacing).
    fn rpc_routes(&self, _resources: PluginResources) -> axum::Router {
        axum::Router::new()
    }

    async fn on_startup(&self, _resources: &PluginResources) -> Result<(), PluginError> { Ok(()) }
    async fn on_shutdown(&self, _resources: &PluginResources) -> Result<(), PluginError> { Ok(()) }
}
```

Existing plugins (just `hello`) compile unchanged; they only need to
override when they want RPC.

## Part 5 — Host mount changes

`platform/src/server.rs::build_app` grows:

```rust
pub fn build_app(plugins: &[Box<dyn Plugin>]) -> Router {
    let mut app = base_app();
    let mut rpc_root = Router::new();

    for plugin in plugins {
        let metadata = plugin.metadata();
        let resources = boot::build_resources_for(metadata.name);

        app = app.nest(metadata.mount.http_prefix, plugin.routes(resources.clone()));
        rpc_root = rpc_root.merge(plugin.rpc_routes(resources));
    }

    app.nest("/rpc", rpc_root)
}
```

The base SPA fallback still owns any path not matched by `/h/*`, `/rpc/*`,
or one of the host's own routes — so `/p/<plugin>/...` falls through to
the embedded index.html.

## Part 6 — Hello plugin updates

### plugins/hello/build.rs (new)

```rust
fn main() -> std::io::Result<()> {
    let mut config = prost_build::Config::new();
    config.type_attribute(".", "#[derive(::serde::Serialize, ::serde::Deserialize)]");
    config.compile_protos(
        &["proto/hello/v1/hello.proto"],
        &["proto", "../../proto"],
    )?;
    Ok(())
}
```

### plugins/hello/Cargo.toml additions

```toml
[dependencies]
junius-sdk  = { workspace = true }
axum        = { workspace = true }
async-trait = { workspace = true }
prost       = { workspace = true }
serde       = { workspace = true }

[build-dependencies]
prost-build = { workspace = true }
```

### plugins/hello/src/lib.rs

Add module + service implementation + `rpc_routes()` override:

```rust
mod gen {
    include!(concat!(env!("OUT_DIR"), "/hello.v1.rs"));
}

use gen::{GreetRequest, GreetResponse};

impl HelloPlugin {
    async fn greet(req: GreetRequest) -> junius_sdk::rpc::RpcResult<GreetResponse> {
        let name = if req.name.is_empty() { "world".to_string() } else { req.name };
        Ok(GreetResponse { message: format!("Hello, {name}!") })
    }
}

#[async_trait]
impl Plugin for HelloPlugin {
    // ... existing metadata + routes ...

    fn rpc_routes(&self, _resources: PluginResources) -> Router {
        junius_sdk::rpc::ServiceBuilder::new("hello.v1.HelloService")
            .unary("Greet", Self::greet)
            .into_router()
    }
}
```

## Part 7 — `junius sync` extension

`tools/junius/src/commands/sync.rs` gains a new step at the end of the
existing pipeline:

1. Existing: Rust registry + Cargo markers + FE routes/component-registry.
2. **New**: invoke `pnpm exec buf generate` from the cwd.
3. **New**: write `packages/generated/src/plugins/<name>/rpc.ts` —
   a barrel re-exporting the plugin's service from
   `../../proto/<name>/v1/...`.
4. **New**: update `packages/generated/package.json` `exports` map to
   include the per-plugin rpc subpath (via JSON parse + targeted update).

For M05's hello plugin, the barrel content is:

```ts
// Generated by junius sync — do not edit.
export { HelloService } from '../../proto/hello/v1/hello_pb.js';
```

`junius sync` runs `pnpm exec buf generate` whenever it executes. The
overhead is small (~300ms locally); idempotency is fine because buf only
rewrites changed outputs.

### Skipping buf generate when pnpm isn't on PATH

For environments without pnpm (tests inside tempdirs, CI inside Nix), the
buf-generate step is skipped with a warning rather than failing. The
sync's existing file outputs still happen. `--require-codegen` flag
(deferred to M09 polish) would force an error.

## Part 8 — Frontend changes

### platform/frontend/src/main.tsx

Wrap the existing provider stack with `TransportProvider`:

```tsx
import { createConnectTransport } from '@connectrpc/connect-web';
import { TransportProvider } from '@connectrpc/connect-query';

const transport = createConnectTransport({ baseUrl: '/rpc' });

// ... inside the JSX:
<AuthProvider>
  <QueryClientProvider client={queryClient}>
    <TransportProvider transport={transport}>
      <ComponentRegistryProvider registry={componentRegistry}>
        <RouterProvider router={router} />
      </ComponentRegistryProvider>
    </TransportProvider>
  </QueryClientProvider>
</AuthProvider>
```

### platform/frontend/vite.config.ts

The existing `/rpc → 127.0.0.1:18080` proxy entry already covers M05.
Verify it's there; no change.

### plugins/hello/frontend/src/routes/pages/HelloPage.tsx

Replace the existing `fetch('/h/hello/ping')` with a Connect-Query call:

```tsx
import { useQuery } from '@connectrpc/connect-query';
import { HelloService } from '@junius/generated/hello/rpc';

export function HelloPage() {
  const user = useUser();
  const { data, error, isLoading } = useQuery(
    HelloService.method.greet,
    { name: user?.displayName ?? 'world' },
  );

  return (
    <Stack gap="md">
      <Card>...intro card unchanged...</Card>
      <Card>
        <Stack gap="sm">
          <h2 className="text-base font-semibold">Server reply (RPC)</h2>
          {error ? <pre className="text-danger text-sm">{String(error)}</pre>
           : isLoading ? <p className="text-fg-2 text-sm">fetching…</p>
           : <pre className="text-sm">{data?.message}</pre>}
        </Stack>
      </Card>
    </Stack>
  );
}
```

Keep the `/h/hello/ping` HTTP route + handler in hello plugin for now —
useful as a backstop and exercises `Plugin::routes()` independent of RPC.
The page just no longer calls it; M07 might revisit.

## Part 9 — Tests

### crates/junius-sdk: unit tests for the `rpc` module

In `crates/junius-sdk/src/rpc/builder.rs#[cfg(test)] mod tests`:

- Round-trip via `axum::Router::oneshot`: build a tiny `EchoService.Echo`
  handler, send a binary protobuf request, assert the encoded response
  decodes back to the right message.
- Same test but with `Content-Type: application/json` body — assert the
  JSON response shape.
- Error mapping: handler returns `RpcError { code: PermissionDenied, .. }`
  → response has HTTP 403, JSON body `{"code":"permission_denied",...}`.
- Unknown method: requests to a path the service didn't register
  → 404.

The test crate needs an inline `.proto` for the echo type — use a
simple hand-written `prost::Message` impl instead to avoid dragging
prost-build into junius-sdk's test build.

### plugins/hello/tests/hello_rpc.rs (new)

In-process test: build the plugin's `rpc_routes()`, call
`/hello.v1.HelloService/Greet` with a binary `GreetRequest` for
`name = "alice"`, assert response decodes to `GreetResponse { message:
"Hello, alice!" }`.

### platform/tests/boot_with_hello.rs (extended)

Add: the full bound binary serves `/rpc/hello.v1.HelloService/Greet`
end-to-end via `reqwest` with both binary and JSON codecs.

### tools/junius/tests/sync.rs (extended)

- Snapshot the generated `packages/generated/src/plugins/hello/rpc.ts`
  barrel for a tempdir with hello enabled.
- Skip-when-pnpm-missing test: tempdir without pnpm on PATH still
  exits 0; the buf-generate step is reported as skipped.

### Vitest

No vitest changes — the FE consumes the transport from a context provider
and `useQuery` is implementation detail. Browser-level verification is
sufficient.

## Part 10 — CI

`.github/workflows/ci.yml` gains:

```yaml
- name: Buf — lint
  run: nix develop --command pnpm exec buf lint

- name: Buf — format check
  run: nix develop --command pnpm exec buf format --diff
```

`buf breaking --against` is deferred to M06 — we need a stable proto on
`main` before the comparison is meaningful, and adding it now would
cause spurious failures while we churn proto files for hello.

## Part 11 — Out of scope (per the milestone doc)

- **No permission enforcement on RPCs.** `option (platform.requires)` is
  declared in `annotations.proto` but not enforced. Lands in M07 along
  with the typed permission system.
- **No narrow per-plugin RPC namespace.** The generated barrel re-exports
  the full service. The `[dependencies.<dep>].rpc_methods`-driven
  narrowing arrives in M09 with the cross-plugin work.
- **No streaming RPCs.** Unary only. `ServiceBuilder::unary(...)` is the
  only method shape.
- **No buf-breaking check in CI.** Deferred to M06.
- **No proto3-JSON spec compliance.** Plain serde derives are good
  enough for our scalar-string messages. The well-known-types problem
  is solvable via `prost-wkt` later if anything needs it.
- **No `junius dev` proto/manifest watcher.** Polish that lands during
  M06's database work or as part of a separate dev-loop tightening
  pass. M05 verification works fine via manual `junius sync`.

## Part 12 — Execution order

1. **Workspace deps**: add `prost`, `bytes`, `serde-bytes`, `prost-build`
   to `[workspace.dependencies]`. Add npm devDeps + FE runtime deps.
   Run `pnpm install`.
2. **`junius-sdk::rpc`**: write module, including the inline-message
   unit tests. `cargo test -p junius-sdk` green.
3. **`Plugin::rpc_routes`**: add the default method on the trait. Confirm
   all existing crates still build (`cargo build --workspace`).
4. **Host mount**: extend `platform/src/server.rs::build_app` to nest
   `rpc_routes()` under `/rpc`. Update boot-with-empty-plugins tests if
   needed.
5. **Proto sources + buf workspace**: create `proto/platform/v1/annotations.proto`,
   `plugins/hello/proto/hello/v1/hello.proto`, root `buf.yaml` and
   `buf.gen.yaml`. Remove `buf.work.yaml`. Verify `pnpm exec buf lint`
   passes; `pnpm exec buf generate` populates
   `packages/generated/src/proto/hello/v1/`.
6. **Hello Rust RPC**: add `build.rs`, deps, `mod gen`, `Self::greet`,
   `rpc_routes()` override. `cargo build -p hello-plugin` green.
7. **`junius sync` codegen step**: extend sync to run buf generate (when
   pnpm is on PATH) + write the per-plugin barrel + update
   `@junius/generated/package.json` exports. Existing sync tests stay
   green; add the new snapshot for the hello rpc barrel.
8. **TS smoke**: `pnpm install`; `pnpm --filter @junius/shell exec tsc
   --noEmit` resolves the new imports cleanly.
9. **FE wiring**: add `TransportProvider` in `main.tsx`; switch HelloPage
   to `useQuery(HelloService.method.greet, ...)`.
10. **End-to-end**: `target/debug/junius build --config dev/platform.toml`
    → `target/debug/juniusd` → curl/`/rpc/hello.v1.HelloService/Greet`
    with both binary and JSON bodies. Browser at `localhost:18080/p/hello`
    shows the SPA rendering the RPC reply.
11. **CI**: add `buf lint` + `buf format --diff` steps.
12. **Docs**: mark M05 status ✅ in
    `docs/impl/06-M05-connect-rpc.md` and the README; note the URL-scheme
    deviation in the M05 doc.
13. **Commit + push**: one M05 commit at the end.

## Verification

Inside the dev shell:

```bash
# Pipeline still clean
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p platform --features embed-frontend --all-targets -- -D warnings
cargo test --workspace
pnpm exec buf lint
pnpm exec buf format --diff
pnpm exec biome ci
pnpm run typecheck
pnpm test

# RPC round-trip — binary
cat <<EOF | xxd -r -p > /tmp/greet.bin
0a05616c696365
EOF  # GreetRequest { name = "alice" } encoded
curl -fsS -X POST http://127.0.0.1:18080/rpc/hello.v1.HelloService/Greet \
  -H 'Content-Type: application/proto' \
  --data-binary @/tmp/greet.bin \
  -o /tmp/greet.out
# /tmp/greet.out decodes to GreetResponse { message: "Hello, alice!" }

# RPC round-trip — JSON
curl -fsS -X POST http://127.0.0.1:18080/rpc/hello.v1.HelloService/Greet \
  -H 'Content-Type: application/json' \
  -d '{"name":"alice"}'
# → {"message":"Hello, alice!"}

# Browser smoke
target/debug/junius dev --config dev/platform.toml
# Visit http://localhost:5173/p/hello → page shows "Hello, Dev User!" fetched via Connect-Query.

# Standalone binary
target/debug/junius build --config dev/platform.toml
./target/debug/juniusd &
# Visit http://localhost:18080/p/hello → same SPA, same RPC reply, served from one binary.
```

## Risks

- **Buf v2 module config quirks.** The workspace-style v2 `buf.yaml`
  introduced in buf 1.32+ is well-supported but slightly less documented
  than v1; expect to spend a few minutes wrangling the `inputs` and
  `modules` blocks until `buf lint` is clean.
- **prost-build serde derives.** Sprinkling `#[derive(Serialize,
  Deserialize)]` on every prost-generated type via `type_attribute` works
  for plain scalar/string fields. If we ever add `bytes`, `uint64`, or
  well-known types to a message, we'll need either `prost-wkt` or a
  per-field attribute override. Hello has only `string` fields so this
  is fine for M05.
- **`@connectrpc/connect-query` v2 ↔ TanStack Query v5.** Connect-Query
  v2's hook API matches TanStack Query v5; both are pinned to v2/v5 in
  this milestone. Pinning matters because v1 vs v2 of Connect-ES changed
  the import paths.
- **Buf availability outside Nix.** `pnpm exec buf` falls back to a
  downloaded binary on `pnpm install` — works on any platform pnpm
  supports, including CI runners that don't have buf in PATH.
- **CARGO_MANIFEST_DIR in prost-build.** prost-build resolves proto
  include paths relative to `CARGO_MANIFEST_DIR`. The `build.rs` uses
  `&["proto", "../../proto"]` — the second entry is the workspace-level
  `proto/` (where `annotations.proto` lives). Build fails if the relative
  path is wrong; tests during step 6 will catch it.

## Files to be modified or created

**New:**
- `crates/junius-sdk/src/rpc/{mod,builder,codec,error}.rs`
- `proto/platform/v1/annotations.proto`
- `proto/buf.yaml` (only if buf v2 needs per-module file — TBD during impl)
- `plugins/hello/proto/hello/v1/hello.proto`
- `plugins/hello/proto/buf.yaml` (same TBD)
- `plugins/hello/build.rs`
- `plugins/hello/tests/hello_rpc.rs`
- `buf.yaml` (workspace-level v2 config)
- `buf.gen.yaml`
- `packages/generated/src/plugins/hello/rpc.ts` (written by junius sync)
- `packages/generated/src/proto/hello/v1/hello_pb.ts` (written by buf generate)
- `tools/junius/tests/snapshots/sync__rpc_barrel_hello.snap`

**Modified:**
- `Cargo.toml` (workspace deps)
- `crates/junius-sdk/Cargo.toml` + `src/lib.rs` (add `pub mod rpc;`)
- `crates/junius-sdk/src/plugin.rs` (add `rpc_routes()`)
- `platform/src/server.rs` (mount rpc routes)
- `platform/tests/boot_with_hello.rs` (add RPC end-to-end tests)
- `plugins/hello/Cargo.toml` (deps + build-deps)
- `plugins/hello/src/lib.rs` (add `mod gen`, `Self::greet`, `rpc_routes()`)
- `plugins/hello/frontend/src/routes/pages/HelloPage.tsx` (use Connect-Query)
- `plugins/hello/frontend/package.json` (add `@junius/generated`)
- `platform/frontend/package.json` (add Connect-Web + Connect-Query)
- `platform/frontend/src/main.tsx` (TransportProvider)
- `package.json` (root devDeps)
- `tools/junius/src/commands/sync.rs` (buf generate + barrel)
- `tools/junius/tests/sync.rs` (snapshot test)
- `.github/workflows/ci.yml` (buf lint + format)
- `docs/impl/06-M05-connect-rpc.md` (status ✅)
- `docs/impl/README.md` (status column)
- `docs/impl/15-open-questions-resolution.md` (note v0 cut reached at end of M05)
- (delete) `buf.work.yaml` — replaced by root `buf.yaml`
