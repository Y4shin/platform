# M15 — Plugin Authoring DX & SDK Hardening (follow-up)

> **Status:** 🚧 Planned (follow-up). Collected from the M13 plugin-authoring
> friction log ([14-M13-events-plugin.friction.md](14-M13-events-plugin.friction.md))
> during the mid-M13 triage. Sequenced after M14 (i18n).

## Goal

Address the plugin-authoring friction surfaced while building the first real
domain plugin (events) that was **deferred** at triage — the SDK/host ergonomics
and directory-authorization items. (The `junius new`/`sync` workflow gaps —
"Cluster A" — were fixed *during* M13, Stage 4; this milestone is the remainder.)

## Scope (in)

### B — `PluginResourceCtx` construction is not change-safe
`PluginResourceCtx::new` is a 13-positional-argument constructor; adding a host
handle breaks callers with an opaque arg-count error and relies on
`#[allow(too_many_arguments)]`.

**Requirement (per triage):** the fix must make **build time catch** a
missing/changed handle — a *builder is insufficient* because a forgotten
`.with_x()` still compiles. Prefer a **public named-field struct literal**
(`PluginResourceCtx { config, telemetry, db, auth, users, groups, … }`): adding a
field is then a compile error at every construction site, naming the missing
field, while staying self-documenting. (Consider `#[non_exhaustive]` only if
external construction must be forbidden; here we *want* construction sites to
break loudly.)

### C — `Groups` directory authorization + name uniqueness
- **Privacy:** `Groups` runs on the platform pool, so any plugin can enumerate
  any group's full roster + emails and resolve any group name — no per-call
  authorization (extends the `Users` trust model, but `members()` is more
  sensitive). Add an authorization-checked path — e.g. `members(group,
  as_caller)` (caller must be a member / hold a permission in that group), or gate
  behind a `directory.groups` capability.
- **`by_name` ambiguity:** `platform.group.name` has no unique constraint, so
  `by_name` returns the earliest-created match. Decide: add a unique constraint,
  expose `by_name_all() -> Vec` and 404 on ambiguity, or keep id-based lookups
  canonical (M13's current choice for `/ics/g/<name>`).

### D — Centralize the "redirect to login?" decision (frontend)
There are two independent redirect-to-login paths (`AuthProvider`'s `/api/me`
handler and `queryClient.onError` on `Code.Unauthenticated`); both had to be
taught the public-path allowlist separately. Centralize the decision (one helper
both consult, or have the transport/provider own it) so a public page can't
regress by missing one.

## Scope (out)
- The `junius new`/`sync` workflow fixes (Cluster A) — done in M13 Stage 4.
- Anything already covered by M12 hardening / M14 i18n.

## Verification
- Adding a new `PluginResources` handle is a compile error at every
  `PluginResourceCtx` construction site (no silent positional drift).
- A plugin cannot read a group's roster it isn't authorized for (the
  authorization-checked `members` path); `by_name` ambiguity has a defined
  behavior.
- A public page makes an unauthenticated RPC without redirecting (covered by one
  centralized check, with a regression test).
