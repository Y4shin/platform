# Validation and the CI gate

A change is ready when `task ci` is fully green with **zero skips and zero
warnings**. This chapter is the checklist for getting there, plus a decoder ring
for the static checks `junius check` runs.

## After a change: regenerate, then gate

```bash
# 1. composition glue + auto-wiring (manifest/proto/route changes):
task sync

# 2. the .sqlx offline cache, if you added/changed a query:
#    (Postgres up, host + plugin migrations applied)
DATABASE_URL=… SQLX_OFFLINE=false cargo sqlx prepare --workspace
git add .sqlx

# 3. formatting:
task fmt          # cargo fmt · biome · buf format

# 4. the full gate:
task ci
```

`task ci` runs fmt-check, clippy (`-D warnings`), the no-default `junius` build,
`biome ci`, buf lint/format, `junius check`, the Rust + JS tests, the i18n gate,
and E2E (auto-skipped only when Docker is unreachable).

## The CI jobs, and their local mirrors

CI runs these in parallel; each maps to a `task` target you can run to reproduce
a failure locally:

| CI job | Local | Covers |
| --- | --- | --- |
| lint | `task ci:lint` | fmt-check, clippy, no-default `junius` build, biome, buf lint/format |
| check | `task ci:check` | workspace build, Rust + JS tests, buf breaking vs `main` |
| integration | `task ci:integration` | migrate the example deployment + verify the `.sqlx` cache |
| deployment | `task ci:deployment` | validate + build the example deployment |
| i18n | `task ci:i18n` | BE/FE catalog validation, extract-drift, pseudo completeness |
| e2e / e2e:split | `task ci:e2e` / `ci:e2e:split` | Playwright (embedded + SSR-split topologies) |

## Decoding `junius check` failures

`junius check` is the plugin-aware static gate. The failures you're most likely
to hit:

| Code | Means | Fix |
| --- | --- | --- |
| `PROTO.REQUIRES.UNDECLARED` | a method's `requires` string isn't in `[permissions]` | declare it, or fix the typo |
| `RPC.HANDLER.UNGUARDED` | a bare `impl XService for Y` in source | use `#[rpc_service]` |
| `RPC.SERVICE.UNIMPLEMENTED` | a proto service has no handler block | `junius rpc scaffold --plugin <name>` |
| `RPC.WITNESS.MISMATCH` | a handler's ctx alias names the wrong method | point the alias at *this* method |
| `SQL.PRIVATE_TABLE_ACCESS` | you queried another plugin's unexposed table | only touch exposed tables; or expose yours |
| `FK.CROSS.CASCADE` | a cross-plugin FK uses `ON DELETE CASCADE` | make it nullable + non-cascading |
| `FE.EXPORTS.MATCH_MANIFEST` | an exposed component isn't exported from `index.ts` | add the named export |
| `STORAGE.BUCKET.UNMAPPED` | a declared bucket isn't mapped in the deployment | map it in `platform.toml` |

Run it on its own for a fast check:

```bash
cargo run -p junius -- check --config dev/platform.toml
```

## Two formatting gotchas

- **`biome format --write` ≠ `biome check --write`.** The CI gate's `biome ci`
  also enforces import sorting (an *assist* action). Run `biome check --write` on
  hand-written TS, not just `format --write`, or you'll pass locally and fail in
  CI.
- **A stale `.sqlx` cache** is the most common red-CI surprise. Any time you add
  or change a compile-time query, regenerate and commit `.sqlx/`.

## Inspecting a plugin

```bash
cargo run -p junius -- plugin info events   # permissions, capabilities, mounts, exposes
cargo run -p junius -- check                # the full static gate
```

## A PR isn't done until remote CI is green

Passing `task ci` locally is necessary but not sufficient — the remote checks are
the source of truth. After your final push, wait for the run to finish:

```bash
gh pr checks --watch    # blocks until every check completes, then reports pass/fail
```

If a check fails, fix it, push, and wait on the new run. Don't report a PR as
done on a pending or red run. (The repo's
[git workflow rules](../../../../.claude/rules/git-workflow.md) have the full
policy: branch from `main`, Conventional Commits with a scope, merge — never
squash.)
