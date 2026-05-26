# 17. M15 — Typed RPC handlers (`#[rpc_service]` + proto-derived witnesses)

> **Status:** ✅ Implemented. Realizes the RPC half of
> [design §11.8](../design/11-backend-plugin-interface.md) that §11.12 deferred —
> **without forking `connectrpc-build`**. Sequenced after
> [M14 i18n](16-M14-internationalization.md); adoption was gated on
> [M13](14-M13-events-plugin.md), now ✅.

One-line goal: make the proto `option (platform.v1.requires)` the **single source of truth** for an
RPC method's permissions, and let plugin authors write handlers that *receive* an explicitly-typed,
already-permission-checked context — instead of restating the permission set in every handler body.

## Why this milestone exists

Today an RPC method's permission set is written **twice**, in two encodings that nothing reconciles:

1. In the proto — `option (platform.v1.requires) = "events:read,events:write"`.
2. In the handler body — `EventCtx::<permissions!(EventsRead & EventsWrite)>::from_rpc(&ctx)?`.

The host guard ([`platform/src/rpc_guard.rs`](../../platform/src/rpc_guard.rs)) enforces #1 at runtime
from the generated [`RPC_REQUIRES`](../../platform/src/generated/rpc_requires.rs) table; the handler
witness enforces (and *unlocks the typed repository* for) #2. They can drift, and the only existing
guardrail — `PROTO.REQUIRES.UNDECLARED` in [`check.rs`](../../tools/junius/src/commands/check.rs) —
checks the proto string against the manifest, **not** against the handler. The realistic failure is
not privilege escalation (the repo `Has<X>` gating backstops the dangerous direction) but
*inconsistency*: the frontend gates UI off the proto annotation while the handler decides
accept/reject off the witness. See the friction discussion captured for
[M13's authoring log](14-M13-events-plugin.friction.md).

[Design §11.8](../design/11-backend-plugin-interface.md) intended codegen to synthesize the typed
context parameter from the annotation; §11.12 deferred it because it looked like it required a buf
custom-options round-trip into the service-trait codegen. This milestone delivers the same outcome
with a **thin attribute-macro adapter on top of the fixed `connectrpc` trait** plus a small
build-time codegen step — no third-party fork.

## Outcome / acceptance

- The proto annotation is the single upstream. The host `RPC_REQUIRES` table, the plugin-side witness
  types, and `junius check` all derive from **one** scanner.
- Authors implement a service as an `#[rpc_service(Svc)]` `impl`, where each handler receives the typed
  context as a parameter; the macro rewrites it into the shape the `connectrpc` trait expects.
- `junius check` fails if a service is implemented without the macro, if a proto service has no impl,
  or if a handler's witness alias does not correspond to its method.
- `junius rpc scaffold` inserts a compiling `todo!()` stub for every proto method missing from its
  service impl.

## Decisions (confirmed with the user, 2026-05-25)

1. **Explicit alias in the signature — not a bare ctx.** A bare `ctx: EventCtx` that the macro
   silently fills was rejected as "too much magic" when reading a handler. Authors write the full
   witness alias in the parameter type, so the requirement is visible at the call site. Consequence:
   **the macro is plumbing-only** — it uses the author's written ctx type verbatim and never invents a
   witness. (This also lets `junius check` verify the alias *syntactically*, since the alias path names
   the method; see Stage 5.)
2. **Validation stays centralized in `junius check`; mutation is isolated in a dedicated command.**
   `junius sync` is deployment-composition-scoped (reads `platform.toml`, regenerates glue for one
   deployment), so per the M13 command-taxonomy principle the source-mutating, plugin-intrinsic
   scaffolder does **not** belong in it. Instead: `junius check` is the one "is everything alright"
   gate (it gains the enforcement rules), and a new `junius rpc scaffold` does the fixing, with a
   `--check` dry-run mode that `junius check` invokes — so the central gate *detects* drift and the
   dedicated command *fixes* it.

## Scope

**In:** the shared scanner crate; build-time witness generation; the `#[rpc_service]` macro; adoption
in `events` (the only plugin with an RPC surface; the `greetings`/`hello`
illustrations in earlier drafts referenced dummy plugins now deleted); the `junius check` enforcement rules; the
`junius rpc scaffold` codemod; docs + friction-log closure.

**Out (deferred, noted at the call sites):**
- **Streaming RPC methods.** All current handlers are unary. The macro rejects non-unary shapes with a
  clear `compile_error!`; streaming support is a follow-up.
- **Cross-plugin permission requirements on a method** (a method requiring another plugin's
  permission). The generated witness references `crate::permissions::*`; a requirement naming another
  plugin's segment is a build error for now.
- **Full-service scaffolding** (generating a whole missing `impl` + marker struct). Stretch goal in
  Stage 6; the committed scope is "fill missing *methods* into an existing service impl".
- **Bare-ctx mode** (per decision #1).

## The naming contract (the part that must be reliable)

Everything hinges on the author, the build step, and the codemod agreeing on one type path without
guessing. The contract:

- **build.rs** emits, per service, a module `snake(ServiceName)` containing one **type alias per
  method**, named `UpperCamel(rust_fn_ident)`, where `rust_fn_ident` is computed by the *same* logic
  `connectrpc-build` uses for the trait method name (via the shared crate — so the one risky
  Pascal→snake conversion happens once, in the same place the trait codegen does it).
- Each alias resolves to the right-nested `And`-chain of `crate::permissions::*` markers
  (`junius_sdk::permissions::And<…, …>`); a method with no annotation → `()` (the empty witness).
- **the author** writes that path in the ctx parameter:
  `ctx: EventCtx<crate::__rpc_requires::event_service::CreateEvent>`.
- **the macro** ignores naming entirely — it calls `<written type>::from_rpc(&__ctx)?`.
- **`junius rpc scaffold`** and **`junius check`** reconstruct the path the *safe* direction:
  `UpperCamel` of the snake fn ident + `snake` of the service name. Both Pascal-case the same snake
  ident, so they agree by construction.

The alias is used **directly** as the `P` type parameter. It must **not** be wrapped in
`permissions!(…)` — the alias is already a built witness, and wrapping it makes the chain head an
`And` rather than a `Permission`, which breaks `Has<X>` resolution and silently disables repo gating.

## Authoring shape (before → after)

Before ([`plugins/events/src/lib.rs`](../../plugins/events/src/lib.rs) at M13):

```rust
impl EventService for EventRpc {
    async fn create_event(&self, ctx: RequestContext, request: OwnedCreateEventRequestView)
        -> ServiceResult<impl Encodable<pb::CreateEventResponse>> {
        let ectx = EventCtx::<junius_sdk::permissions!(EventsRead & EventsWrite)>::from_rpc(&ctx)?;
        let created = ectx.state.events.create(/* … */).await?;
        Ok(Response::new(/* … */))
    }
}
```

After (post-M15, current source):

```rust
#[junius_sdk::rpc_service(EventService)]
impl EventRpc {
    async fn create_event(
        &self,
        ectx: EventCtx<crate::__rpc_requires::event_service::CreateEvent>,
        request: OwnedCreateEventRequestView,
    ) -> ServiceResult<impl Encodable<pb::CreateEventResponse>> {
        let created = ectx.state.events.create(/* … */).await?;   // ectx already resolved + checked
        Ok(Response::new(/* … */))
    }
}
```

Expansion (conceptual):

```rust
impl EventService for EventRpc {
    async fn create_event(&self, __ctx: RequestContext, request: OwnedCreateEventRequestView)
        -> ServiceResult<impl Encodable<pb::CreateEventResponse>> {
        let ectx = EventCtx::<crate::__rpc_requires::event_service::CreateEvent>::from_rpc(&__ctx)?;
        let created = ectx.state.events.create(/* … */).await?;
        Ok(Response::new(/* … */))
    }
}
```

The permission set now exists only in the proto; the alias the author writes is generated *from* it.
Registration is unchanged — `Arc::new(EventRpc).register(router)` in `register_rpc` is orthogonal.

## Stages

Workflow per the repo norm: one **unsigned** commit per stage, `task ci` green per stage, sign+push
the batch at the end.

### Stage 0 — Spike / go-no-go (throwaway branch)
De-risk the two things that historically sink signature-rewriting macros: (a) **rust-analyzer**
behaviour on an `#[rpc_service]`-rewritten handler — completion + inline diagnostics inside the body;
(b) confirm `connectrpc-build`'s exact names for the request-view type (`Owned<Method>RequestView`),
the response type, and the trait fn ident, and **pin the `connectrpc` version**.
**Verify:** a hand-written expansion compiles for one events method and RA stays usable. If RA
degrades, the explicit-alias decision already keeps the macro minimal — fall back to a macro that does
*only* the `RequestContext`→`from_rpc` rewrite (no other behaviour change). Go/no-go gate.

### Stage 1 — Shared `junius-rpc-meta` crate (no behaviour change)
Lift `scan_proto_requires` ([`sync.rs`](../../tools/junius/src/commands/sync.rs)), the
permission-key→marker `pascal_case` helper (currently in
[`macros/src/expand.rs`](../../crates/junius-sdk-macros/src/expand.rs)), and the
proto-method→rust-ident casing into a small crate usable both as a normal dependency (for
`tools/junius`) and as a `build-dependencies` entry (for plugins). Rewire `sync` and `check` to call
it.
**Verify:** `task ci` green; the `sync__*` snapshots are byte-identical (proof the refactor changed
nothing).

### Stage 2 — build-time `__rpc_requires` generation
Add a one-call helper to `junius-rpc-meta` (`emit_rpc_requires(&proto_files, out_dir)`) so each
plugin's [`build.rs`](../../plugins/events/build.rs) grows ~2 lines after the existing
`connectrpc_build` call. It writes `_rpc_requires.rs` (a `pub mod __rpc_requires`) into `OUT_DIR`,
`include!`d from `lib.rs`. Land it in `events` first.
**Verify:** `events` compiles with the module present; a temporary
`EventCtx::<crate::__rpc_requires::event_service::CreateEvent>::from_rpc(&ctx)` type-checks and
`Has<EventsWrite>` still resolves (this is exactly where the `permissions!`-double-wrap bug would
bite — the alias is used directly as `P`).

### Stage 3 — the `#[rpc_service]` macro (plumbing-only, per decision #1)
New proc-macro in [`junius-sdk-macros`](../../crates/junius-sdk-macros). Scope: unary methods. It
consumes an inherent `impl EventRpc`, and for each method: replaces the ctx parameter with
`__ctx: RequestContext`, prepends `let <ident> = <written ctx type>::from_rpc(&__ctx)?;`, passes the
body through with **preserved spans** (for error quality), and emits `impl EventService for
EventRpc`. It reads the ctx parameter's *written* type verbatim — it never constructs a witness.
The ctx parameter is identified by position (first non-`&self` parameter, the `RequestContext` slot);
a non-unary/streaming shape → a clear `compile_error!`.
**Verify:** a `trybuild` suite — passing case; compile-fail cases for a too-weak witness (the repo
method is unnameable), a missing ctx parameter, and a streaming-shaped method.

### Stage 4 — Adopt
Migrate `events` (`EventService` 6 methods, `InviteService` 7 methods,
`CalendarService` 5 methods — the only plugin with an RPC surface) and
validate end-to-end: the host guard still rejects pre-dispatch, `from_rpc`
still runtime-checks, repo `Has<X>` gating is intact.
**Verify:** per plugin, `task ci` green and the existing RPC integration tests pass unchanged; the diff
shows handlers lost their `permissions!(…)` / `from_rpc` lines and gained the typed ctx parameter.

### Stage 5 — `junius check` enforcement
New rules alongside `check_proto_requires` in [`check.rs`](../../tools/junius/src/commands/check.rs),
all syn-parsing the plugin's `src/**/*.rs`:
- `RPC.HANDLER.UNGUARDED` — an `impl <X>Service for …` not carrying `#[rpc_service]`.
- `RPC.SERVICE.UNIMPLEMENTED` — a service present in the plugin's proto with no `#[rpc_service]` impl
  (this also produces the worklist for Stage 6).
- `RPC.WITNESS.MISMATCH` — a handler whose ctx-parameter alias path does **not** correspond to its
  method/service. This is a purely *syntactic* check, enabled by decision #1: the alias path names the
  method (`…::event_service::CreateEvent` ↔ `fn create_event` under `#[rpc_service(EventService)]`),
  so no type evaluation is required. This is the drift guard the explicit-alias form buys back.
**Verify:** snapshot tests — all migrated plugins pass; fixtures with (a) a raw trait impl, (b) a
missing service, and (c) a mismatched alias each fail with the right code.

### Stage 6 — `junius rpc scaffold` (the codemod)
New source-mutating command (decision #2). It scans the plugin's proto → methods, syn-parses the
plugin to find existing `#[rpc_service]` methods, diffs, and inserts a stub for each missing method:

```rust
async fn delete_event(
    &self,
    ctx: EventCtx<crate::__rpc_requires::event_service::DeleteEvent>,
    request: OwnedDeleteEventRequestView,
) -> ServiceResult<impl Encodable<pb::DeleteEventResponse>> {
    todo!("delete_event")
}
```

Insertion is **text-based, before the impl's closing brace, then `rustfmt`** (not a syn re-print), so
the author's existing formatting + comments survive — the same approach the M13 friction triage chose
for `sync`'s generated Rust. The request-view / response type names come from the
`connectrpc-build` naming convention centralized in `junius-rpc-meta`. A `--check` mode reports missing
stubs without writing (called by `junius check`). Stretch: scaffold a whole missing service impl +
marker struct.
**Verify:** a fixture plugin whose proto has a method the Rust lacks → running the command yields a
**compiling** file whose new method is `todo!()`; the run is **idempotent** (a second run is a no-op);
`--check` exits non-zero exactly when stubs are missing.

### Stage 7 — Docs & friction closure
Authoring-guide section ("implementing an RPC service"); update
[design §11.8 / §11.12](../design/11-backend-plugin-interface.md) (the deferred item is now shipped);
add the friction-log row with disposition = "fixed by `#[rpc_service]` + proto-derived witnesses".
**Verify:** doc-links referenced by `junius check` resolve; `task ci` green.

## Risks & mitigations (against the "must be reliable" bar)

- **The macro (Stages 3–4) is low-risk** — with decision #1 it is a pure syntactic rewrite using the
  author's written type; `trybuild` pins behaviour. The only real risk is the **IDE experience**,
  which Stage 0 gates before any adoption.
- **Enforcement (Stage 5) is low-risk** — syn scans of source; the alias-matches-method rule is
  syntactic (no type evaluation) thanks to explicit aliases.
- **The codemod (Stage 6) is the soft spot.** It depends on `connectrpc-build`'s type-naming
  convention staying stable and on format-preserving text insertion. Mitigation: pin `connectrpc`;
  centralize the convention in `junius-rpc-meta`; and have the command **rustfmt-verify and
  compile-check** its output before writing, failing loudly rather than emitting subtly-wrong stubs.
  Treat full reliability here as a stretch relative to Stages 1–5.

## Downstream doc updates

- [`README.md`](README.md) milestone index — add the M15 row.
- [`../design/11-backend-plugin-interface.md`](../design/11-backend-plugin-interface.md) §11.8/§11.12 —
  mark the typed-RPC-handler item delivered, document `#[rpc_service]` + `__rpc_requires`.
- [`14-M13-events-plugin.friction.md`](14-M13-events-plugin.friction.md) — close the
  restate-the-witness row.
