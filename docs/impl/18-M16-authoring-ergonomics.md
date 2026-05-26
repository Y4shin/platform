# 18. M16 — Plugin-authoring ergonomics (M13 dogfooding follow-ups)

> **Status:** ✅ Implemented. Sequenced after
> [M15 typed RPC handlers](17-M15-rpc-service-macro.md) (which already owned the
> largest M13 authoring papercut — the proto-vs-handler permission restatement).
> This milestone delivered the **four follow-ups recorded in the
> [M13 joint friction triage](14-M13-events-plugin.friction.md)** ("Final triage
> outcome"). See the [friction-log table](14-M13-events-plugin.friction.md) for
> per-item dispositions.

One-line goal: turn the four ergonomics gaps the `events` build hand-worked around into first-class
platform affordances, so the *next* domain plugin doesn't re-hit them.

## Why this milestone exists

Building `events` (the first real domain plugin, M13) was deliberately a dogfooding pass. Most
friction was fixed in-build or routed to M15; these four were triaged as "real, but out of M13's
scope" because each is a platform/SDK change rather than plugin code. They're small and orthogonal,
so they don't justify gating a feature milestone — but left undone, every future plugin pays the same
tax (and `events` currently carries local workarounds that should be deleted once these land).

The four items (labels carried over from the triage):

| ID | Gap | Where `events` works around it today |
|----|-----|--------------------------------------|
| **A** | `junius sync` doesn't auto-wire a proto-bearing plugin | manual `buf.yaml` edit + `pnpm exec buf generate`, and a hand-added host-FE `workspace:*` dep |
| **B** | No delete-time counterpart to `record_owner` | `EventRepo::delete` drops the row but orphans `platform.resource_principal` / `resource_share` |
| **C** | No "permission within a group" SDK helper | `events` hand-rolls `require_group_write` over `user.memberships` |
| **D** | No blessed public-handler / plugin-nav affordances | `EventCtx::<()>::from_rpc` + ungated plain-`impl` methods; a local `usePluginNavigate` cast |

## Outcome / acceptance

- A fresh proto-bearing plugin enabled via `junius plugin enable` + `junius sync` **typechecks with
  zero manual `buf`/`pnpm` steps**.
- Deleting an owned resource leaves **no orphaned** `resource_principal` / `resource_share` rows.
- Group-scoped authorization is a one-call SDK helper, used by `events`.
- The public-handler and plugin-sub-route-navigation patterns are SDK-provided and documented; `events`
  drops its local workarounds.
- The relevant friction-log rows are closed; the authoring guide reflects the new affordances.

## Items

### A — `junius sync` auto-wiring (the deferred Cluster-A items)

**Problem.** Enabling a plugin that ships a proto still needs two manual steps `sync` doesn't do:
(1) append `plugins/<name>/proto` to the workspace [`buf.yaml`](../../buf.yaml) `modules:` and run
`pnpm exec buf generate` (otherwise `@junius/generated/<plugin>/rpc` imports a not-yet-generated
`*_pb.ts` and `pnpm typecheck` breaks); (2) add `"@junius/plugin-<name>": "workspace:*"` to
[`platform/frontend/package.json`](../../platform/frontend/package.json) + `pnpm install` (otherwise
the generated `routes.ts` import fails). M13 Stage 4 fixed the rest of `sync` (rustfmt-clean output,
backend-only support) and left these two as "subprocess / format-preserving-edit heavy".

**Fix.** Extend [`sync`](../../tools/junius/src/commands/sync.rs) to, for each enabled plugin:
auto-discover a `proto/` dir and idempotently register it in `buf.yaml` (format-preserving YAML edit)
then run `buf generate`; and add/remove the host frontend's `workspace:*` dependency (mirroring how
`sync` already rewrites `platform/Cargo.toml`'s Rust deps), running `pnpm install` when it changed.
Both behind the existing `sync` drift model so `sync --dry-run` stays meaningful.

**Acceptance.** Scaffold a new proto-bearing plugin, `junius plugin enable` + `junius sync`, and the
workspace builds + typechecks with no hand edits; `sync --dry-run` is drift-free afterward.

**Risk.** Highest of the four — shells out (`buf generate`, `pnpm install`) and does
format-preserving edits to YAML/JSON. Keep each edit idempotent and re-runnable.

### B — `Authz::forget_resource` (delete-time ownership cleanup)

**Problem.** [`Authz`](../../crates/junius-sdk/src/authz.rs) records ownership on create
(`record_owner`) but has no inverse, and `platform.resource_principal` has no FK to the plugin's row
(the resource lives in the plugin schema), so deleting a resource **orphans** its ACL rows
(`resource_principal` + any `resource_share`). A least-privilege plugin role can't `DELETE` those host
tables directly.

**Fix.** Add a `SECURITY DEFINER` `platform.forget_resource(resource_kind, resource_id)` (mirroring
the M08 `record_owner` definer) that deletes the `resource_principal` and `resource_share` rows; expose
`Authz::forget_resource(&mut tx, kind, id)` that calls it in the caller's transaction (atomic with the
row delete); call it from `EventRepo::delete`. Document it as the delete-time counterpart to
`record_owner` in the authoring guide.

**Acceptance.** A new host migration adds the definer fn; `event_access_pg`/a focused test asserts that
after `delete` there are zero `resource_principal`/`resource_share` rows for the id; `junius check`
unaffected (intra-tx definer call, like `record_owner`).

**Files.** new `platform/migrations/00NN_forget_resource.up.sql`,
[`crates/junius-sdk/src/authz.rs`](../../crates/junius-sdk/src/authz.rs),
[`plugins/events/src/repo/event.rs`](../../plugins/events/src/repo/event.rs).

### C — `User::has_permission_in_group`

**Problem.** Some actions need "does the caller hold permission *P* **within group G**" (e.g. publishing
a group calendar / minting a group key requires `events:write` in that group). The static RPC gate only
proves *P* is held *somewhere*, so `events` hand-rolls the per-group check over `user.memberships`
(`require_group_write` in [`plugins/events/src/lib.rs`](../../plugins/events/src/lib.rs)).

**Fix.** Add `User::has_permission_in_group(&self, group: GroupId, permission: &str) -> bool` to the SDK
[`User`](../../crates/junius-sdk/src/auth.rs) (it already carries `memberships[].permissions`). Replace
`events`' `require_group_write` with it. Note the relationship to the group-ownership access rule in the
authoring guide (membership alone isn't access — the role must grant the permission).

**Acceptance.** `events` uses the helper; a unit test over a constructed `User`; the guide's
group-scoped-authz note references it.

### D — Blessed public-handler + plugin-navigation affordances

**Problem.** Two patterns every plugin with a public surface must currently re-derive:
- **Backend:** an ungated handler builds the **no-permission-witness** context
  (`EventCtx::<()>::from_rpc(&ctx)`) and puts its public methods in a plain `impl<P>` block so they're
  callable on `Repo<()>`; the access check is the runtime SQL ACL, not a witness. Works, but it's an
  undocumented idiom.
- **Frontend:** plugin sub-routes are type-erased (`buildRoutes` returns `AnyRoute`), so a typed
  `<Link to="/p/<plugin>/$id">` won't compile; `events` ships a local `usePluginNavigate` that casts
  through `NavigateOptions` ([`plugins/events/frontend/src/nav.ts`](../../plugins/events/frontend/src/nav.ts)).

**Fix.** Backend: add a `PluginContext::public(&ctx)` sugar (equivalent to `::<()>::from_rpc`, but
named so intent is legible) in [`context.rs`](../../crates/junius-sdk/src/context.rs), and document the
plain-`impl<P>` public-method idiom. Frontend: ship `usePluginNavigate` and a `PluginLink` from
`@junius/sdk` (one blessed escape hatch / or a `buildRoutes` change that preserves route typing), and
have `events` consume them (deleting its local `nav.ts`).

**Acceptance.** `events` imports the SDK affordances and drops its local copies; a vitest covers
`PluginLink`; the authoring guide's "public handler" + "frontend routes" sections point at them.

## Stages

Repo norm: one **unsigned** commit per stage, `task ci` green per stage, sign+push the batch at the
end. Ordered easiest → riskiest; the items are independent, so the order is only convenience.

- **Stage 1 — C (`has_permission_in_group`).** Smallest; SDK helper + `events` adoption + unit test.
- **Stage 2 — B (`forget_resource`).** Host migration (definer) + `Authz` method + `EventRepo::delete`
  + orphan-free assertion (regenerate/commit `.sqlx` if a macro query is added).
- **Stage 3 — D (public-handler + nav affordances).** `PluginContext::public` + SDK `usePluginNavigate`/
  `PluginLink`; migrate `events`; vitest.
- **Stage 4 — A (`sync` auto-wiring).** The buf.yaml + `buf generate` + host-FE-dep automation; prove on
  a throwaway scaffolded plugin; keep `sync --dry-run` drift-free.
- **Stage 5 — Docs & friction closure.** Authoring-guide updates for B/C/D; close the corresponding
  friction-log rows; decision-log entry.

## Out of scope

- A group/role **management UI** (assigning permissions without raw SQL) — a much larger platform
  feature surfaced separately during M13 testing; not one of the four triaged items.
- Anything already owned by **M15** (the proto-vs-handler witness restatement). `PluginContext::public`
  (D) is complementary — it's the *ungated* path, orthogonal to M15's gated typed-context work.

## Downstream doc updates

- [`README.md`](README.md) milestone index — add the M16 row.
- [`14-M13-events-plugin.friction.md`](14-M13-events-plugin.friction.md) — close the A/B/C/D rows in
  the "Final triage outcome" with disposition = "fixed in M16".
- [`../plugin-authoring-guide.md`](../plugin-authoring-guide.md) — update the public-handler,
  group-authz, and forms/regen sections once the affordances exist.
- [`../design/14-decision-log.md`](../design/14-decision-log.md) — an M16 entry.
