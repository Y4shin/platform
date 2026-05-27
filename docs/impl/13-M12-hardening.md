# M12 — Hardening: Full `junius check` Ruleset + CI Schema Test + Schema Evolution

> **Status:** ✅ Done (2026-05-25).

## Reconciliation (as built)

The plan below is the design; the shipped implementation differs in a few spots:

- **`sqlparser` is 0.52** (not the 0.48 implied earlier); `SQL.PRIVATE_TABLE_ACCESS`
  collects table references generically via its `visit_relations` visitor (the
  `visitor` feature is now enabled), plus explicit FK `REFERENCES` targets the
  visitor doesn't report inside `CREATE TABLE`.
- **Repo-query SQL is extracted with `syn`** (not regex): plugin `src/*.rs` is
  parsed and the SQL string is pulled from `query!`/`query_as!`/`query_scalar!`
  (+ `_unchecked`) macros — `LitStr::value()` normalises raw/escaped/continued
  strings.
- **`SQL.EXPOSED.NO_BREAKING` type classification is conservative**: any column
  type change is treated as breaking unless the target is an obvious widening
  (`TEXT`, unbounded `VARCHAR`, `BIGINT`); the prior type isn't reconstructed.
  The coordination check is the spec heuristic — a breaking change to an exposed
  table is allowed only if every consumer that declares the table also ships a
  migration in the same diff. It diffs against `--base` (default `main`,
  `origin/main` in CI) and **no-ops outside a git repo** or when the base is
  unresolvable.
- **`SQLX.PREPARE_CHECK` runs in the `integration` job, not `check`** — it needs a
  migrated live DB, so it follows `junius migrate up` against the job's Postgres
  service.
- **`MIGRATIONS.CHECKSUM` adds no new `junius check` code**: the runtime `junius
  migrate up` already enforces checksums and now runs in the `integration` job
  (Layer 2). The DB-connected static fast-path was not built.
- **`junius check` diagnostics gained a doc link** derived from the rule ID; precise
  `line:col` is deferred (`ValidationIssue` carries no span yet).
- **Layer 3** (schema-snapshot diff) remains **deferred**, per the design's trigger.

## Goal

Every static safety rule the design promises is implemented and enforced in CI. Schema-evolution Layer 1 (`junius check` static rules) and Layer 2 (CI integration test) both run. Every "the platform refuses to compile/deploy a broken state" claim becomes a literal test.

## Why now

Through M11, the platform works *if everyone follows the design*. M12 closes the gaps where a careless plugin author could introduce silent breakage — cross-plugin SQL refs without `@requires`, breaking changes to exposed tables, raw `sqlx::query` outside repos, etc. Doing this before M13 means the first real domain plugin (Speakers) lands on a fully-enforced architecture.

## Scope (in)

### `junius check` — full ruleset

The rules below replace, extend, or formalise checks introduced in earlier milestones. Each rule has a stable ID so error messages can reference docs.

| ID | Rule | Introduced | M12 work |
|---|---|---|---|
| `MANIFEST.SCHEMA` | Each `plugin.toml` matches the schema. | M01 | unchanged |
| `MANIFEST.ID` | Plugin `name` matches `^[a-z][a-z0-9_-]*$`. | M01 | unchanged |
| `MANIFEST.PERMS.NAME` | Permission keys match `<plugin>:<segment>`. | M01 | unchanged |
| `MACRO.ONCE` | Each plugin invokes `plugin_metadata!()` exactly once. | M07 | unchanged |
| `RPC.PERMS.DECLARED` | Every `option (platform.requires) = "X"` references a declared permission. | M07 | unchanged |
| `RPC.PERMS.NARROW` | Cross-plugin RPC use matches `[dependencies.<dep>].rpc_methods`. | M09 | unchanged |
| `IMPORT.MANIFEST` | TS/Rust imports from another plugin require a matching `[dependencies.<dep>]`. | M09 | unchanged |
| `SQL.NO_RAW_PG_POOL` | No `use sqlx::PgPool` or `sqlx::query*!` outside `#[impl_repository(...)]`. | M07 | unchanged |
| `SQL.CROSSREF_REQUIRES` | Every cross-plugin schema-qualified SQL reference has a matching `-- @requires`. | M09 | extended below |
| `SQL.CROSS_FK_NULLABLE` | FKs into optional-dep schemas are nullable. | M09 | unchanged |
| `SQL.NO_CROSS_CASCADE` | No `CASCADE` on cross-plugin FKs. | M09 | unchanged |
| `SQL.PRIVATE_TABLE_ACCESS` | No reference to a non-exposed table in another plugin's schema. | **M12** | **new** |
| `SQL.EXPOSED.NO_BREAKING` | `ALTER`/`DROP`/`RENAME` on `[exposes.tables]` requires consumer migrations in the same revision. | **M12** | **new** |
| `MIGRATIONS.DAG` | Every `-- @requires` resolves; no cycles. | M06 | unchanged |
| `MIGRATIONS.CHECKSUM` | `meta.migrations` checksums match files for already-applied migrations. | M06 | extended below |
| `SQLX.PREPARE_CHECK` | `cargo sqlx prepare --check` is clean for every plugin. | M07 | extended below |
| `PROTO.LINT` | `buf lint` passes. | M05 | unchanged |
| `PROTO.BREAKING` | `buf breaking` against `main` passes. | M05 | extended below |
| `FE.EXPORTS.MATCH_MANIFEST` | Each plugin's `src/index.ts` exports the components named in `[exposes.components]`. | **M12** | **new** |

#### Rule details — new in M12

<a id="sql-private-table-access"></a>
**`SQL.PRIVATE_TABLE_ACCESS`**

Uses `sqlparser-rs` (already brought in at M09) to extract every schema-qualified table reference from every migration **and** from queries inside `#[impl_repository(...)]` blocks. For each reference of the form `<other_schema>.<table>` where `<other_schema>` is another plugin's name:

- The referencing plugin must have `[dependencies.<other_schema>]` declared.
- `<table>` must appear in `<other_schema>`'s `[exposes.tables]`.
- `<table>` must appear in the referencing plugin's `[dependencies.<other_schema>].tables = [...]` list.

Postgres role grants already make this fail at runtime; this static rule catches it at PR time.

<a id="sql-exposed-no-breaking"></a>
**`SQL.EXPOSED.NO_BREAKING`**

For every PR / branch diff:
1. List migrations added in the diff (not previously in `meta.migrations`).
2. For each new migration containing `ALTER TABLE` / `DROP TABLE` / `ALTER COLUMN ... RENAME` / `DROP COLUMN` on a table listed in any plugin's `[exposes.tables]`:
   - Classify the change (compatible vs breaking per [../design/10-infrastructure-and-data.md](../design/10-infrastructure-and-data.md) §10.6).
   - If breaking, verify that every consumer plugin (declared via `[dependencies.<dep>].tables`) either:
     - has a matching migration in this same diff that drops the table from its dep declaration, **or**
     - has a matching migration that updates its code to handle the breakage.
3. Reject the diff otherwise.

This is the design's "Layer 1" enforcement for schema evolution.

> **As built:** added migrations are taken from `git diff --diff-filter=A
> <base>...HEAD` ∪ untracked files; type changes are conservatively breaking
> (see Reconciliation); a breaking change is rejected when a declaring consumer
> has no migration in the same diff. No-ops outside a git repo.

<a id="fe-exports-match-manifest"></a>
**`FE.EXPORTS.MATCH_MANIFEST`**

For each plugin with `[exposes.components.X]`, scan the plugin's `frontend/src/index.ts` for a named export `X`. Missing → fail. Present but pointing at a missing module → TypeScript catches that separately during type-check; we don't duplicate it.

#### Rule details — extended in M12

<a id="sql-crossref-requires"></a>
**`SQL.CROSSREF_REQUIRES`** (extended)

Previously only checked migration files. Now also checks queries inside `#[impl_repository]` blocks for cross-plugin table references — those don't need `@requires` (they're runtime), but they **do** need the consumer to have the dep declared, which is the existing `IMPORT.MANIFEST` rule. M12 makes the cross-validation explicit.

<a id="migrations-checksum"></a>
**`MIGRATIONS.CHECKSUM`** (extended)

The check applies to `meta.migrations` rows only — there's no checksum row for unapplied migrations, since they haven't been run yet. **As built:** no new `junius check` code — the runtime `junius migrate up` enforces checksums (failing on `ChecksumMismatch`) and runs in the CI `integration` job, so Layer 2 covers this; the DB-connected static fast-path was not built.

<a id="sqlx-prepare-check"></a>
**`SQLX.PREPARE_CHECK`** (extended)

`cargo sqlx prepare --workspace --check` runs in CI's `integration` job (after `junius migrate up` against the Postgres service, since it needs the live schema). Per-plugin `.sqlx/` cache dirs must be in sync.

<a id="proto-breaking"></a>
**`PROTO.BREAKING`** (extended)

`pnpm exec buf breaking` runs in the CI `check` job, `--against '.git#branch=main'` (the workflow checks out full history and pins a local `main` ref; locally `task buf:breaking` accepts an `AGAINST=` override).

### Layer 2: CI schema integration test

A dedicated CI job:

```yaml
- name: Integration test (Postgres schema)
  services:
    postgres:
      image: postgres:17
      env:
        POSTGRES_USER: platform_migrator
        POSTGRES_PASSWORD: ci
      ports: ["5432:5432"]
      options: >-
        --health-cmd pg_isready --health-interval 5s --health-timeout 5s --health-retries 5
  run: |
    # Build junius
    cargo build --release -p junius

    # Build platform with all sample plugins enabled
    cd examples/example-deployment
    ../../target/release/junius build

    # Migrate against the ephemeral DB
    DATABASE_URL=postgres://platform_migrator:ci@localhost:5432/postgres \
      ../../target/release/junius migrate up

    # Run every plugin's test suite against the migrated DB
    cd ../..
    DATABASE_URL=postgres://platform_migrator:ci@localhost:5432/postgres \
      cargo test --workspace --test '*'

    # Run E2E
    pnpm exec playwright test
```

The example deployment under `examples/example-deployment/` is configured to enable every sample plugin (`hello`, `greetings`, `widgets`) so the CI test exercises the most cross-plugin permutations.

### Layer 3 (schema snapshot) — deferred

Per [../design/10-infrastructure-and-data.md](../design/10-infrastructure-and-data.md) §10.6: per-plugin `exposed-schema.sql` snapshots diffed by `junius`, with breaking changes requiring an `@breaking-change` annotation. M12 documents the future addition but doesn't implement it. Trigger to adopt: manual coordination starts missing things.

### `junius check` output

Every violation reports its rule ID, the file/field path, a one-sentence
description, and a doc pointer derived mechanically from the rule ID
(`<RULE>` → `docs/impl/13-M12-hardening.md#<rule lowercased, . and _ → ->`).
**As built:** the doc pointer is a `doc` field in `--format json` and a miette
`help:` line in the plain rendering; precise `line:col` is deferred (the
`ValidationIssue` type carries no source span yet).

```
SQL.PRIVATE_TABLE_ACCESS

  × references hello.greeting, which "hello" does not expose ([exposes.tables])
  │ (plugins/greetings)
  help: see docs/impl/13-M12-hardening.md#sql-private-table-access
```

<a id="rule-reference"></a>
### Rule reference (anchors)

The doc pointer resolves to the M12-rule sections above for the rules M12 adds
or extends. The remaining cross-plugin and proto rules `junius check` emits
anchor here:

- <a id="dep-undeclared"></a>**`DEP.UNDECLARED`** — a frontend `@junius/plugin-<dep>` import or Rust `<dep>_plugin::` path needs a matching `[dependencies.<dep>]`.
- <a id="rpc-undeclared"></a>**`RPC.UNDECLARED`** — each `[dependencies.<dep>].rpc_methods` entry must name a real `Service.Method` in `<dep>`'s proto.
- <a id="sql-requires-missing"></a>**`SQL.REQUIRES.MISSING`** — a migration with a cross-schema FK must declare `-- @requires <owner>:<migration>`.
- <a id="fk-cross-cascade"></a>**`FK.CROSS.CASCADE`** — a cross-plugin FK must not `ON DELETE/UPDATE CASCADE`.
- <a id="fk-optional-nullable"></a>**`FK.OPTIONAL.NULLABLE`** — an FK into an *optional* dependency's schema must be nullable.
- <a id="storage-bucket-unmapped"></a>**`STORAGE.BUCKET.UNMAPPED`** — every declared logical bucket must be mapped in `[config.storage.mapping]`.
- <a id="proto-requires-undeclared"></a>**`PROTO.REQUIRES.UNDECLARED`** — every `option (platform.requires)` in a `.proto` must name a declared permission.

Manifest-schema rules (single-plugin `junius check`) are listed in the table at
the top of this doc; an unknown anchor simply lands at the top of this page.

### CI workflow consolidation

The CI workflow (originating in M00, extended at every later milestone) is reorganised into four jobs that run in parallel:

1. **`lint`** (`task ci:lint`) — `cargo fmt --check`, `cargo clippy`, the no-default-features CLI build, `biome ci`, `buf lint`, `buf format --diff`, `junius check`.
2. **`check`** (`task ci:check`) — `cargo build --workspace`, the Rust + JS test suites, and `buf breaking` against `main`.
3. **`integration`** (`task ci:integration`) — Layer 2: a Postgres service container, `junius migrate up` of the deployment via the CLI, then `cargo sqlx prepare --workspace --check` against the migrated schema.
4. **`deployment`** (`task ci:deployment`) — `junius build --check-config` from `examples/example-deployment/`; the full build stays behind `JUNIUS_E2E`.

A merge requires all four green. **As built** differs from the original sketch:
`cargo sqlx prepare --check` lives in `integration` (it needs a live migrated DB),
not `check`; testcontainers integration tests run in `check` (they self-provision
via the runner's Docker); Playwright stays gated behind `JUNIUS_E2E`.

> **Updated by [M14](16-M14-internationalization.md):** a fifth parallel job
> `i18n` (`task ci:i18n`) was added for catalog validation.
> **Updated by [M17](19-M17-e2e-testing.md):** a sixth parallel job `e2e`
> (`task ci:e2e`) was added. It spins an ephemeral testcontainers stack and
> runs the per-plugin Playwright suite, replacing the old `JUNIUS_E2E` gate.

## Scope (out)

- Layer 3 schema snapshot enforcement. Deferred.
- OR-style permission combinators. Still deferred (M07 noted this).
- Capability runtime enforcement beyond the existing `CapabilityNotDeclared` errors — out of scope until third-party plugin support is on the roadmap.
- Performance benchmarks in CI. Out of scope.
- Mutation testing or fuzz testing. Out of scope.

## Library choices — confirm with user before starting

| Choice | Proposed default | Rationale | Downstream milestones to update if changed |
|---|---|---|---|
| **SQL parser for `junius check`** | `sqlparser-rs` (PostgreSQL dialect) | Locked at M09; M12 stresses it harder | — |
| **TS scanner** | Custom regex-based import scanner (M09) extended with named-export scanning for `FE.EXPORTS.MATCH_MANIFEST` | Lightweight; the patterns we check don't justify a full TS AST | — |
| **Diff base for `SQL.EXPOSED.NO_BREAKING`** | `main` (default branch) | Standard PR diff base | — |
| **CI runner** | GitHub Actions (locked since M00) | — | — |
| **`junius check` exit codes** | `0` ok; `1` setup/internal error; `2` rule violation(s) | Lets shell scripts distinguish "broken" vs "invalid" | — |
| **CI matrix** | Single OS (Linux) and single Rust toolchain | Single-tenant deployments mean we don't need Windows/macOS CI | — |

## Open questions resolved

None new. M12 closes out the "schema evolution enforcement" decision-log entry by implementing Layers 1+2.

## Verification

```bash
# Run junius check locally against the source repo
target/release/junius check
# → ✔ all rules pass

# Deliberate failures (run one at a time, then revert)

# (a) Private cross-plugin table access
# Edit plugins/greetings/src/repo/X.rs to SELECT FROM hello.greeting (private, not exposed).
target/release/junius check
# → ✖ SQL.PRIVATE_TABLE_ACCESS at plugins/greetings/src/repo/X.rs:N

# (b) Breaking change without coordinated consumer migration
# Add plugins/hello/migrations/<ts>_drop_template_body.up.sql with `ALTER TABLE hello.greeting_template DROP COLUMN body;`
target/release/junius check
# → ✖ SQL.EXPOSED.NO_BREAKING: hello.greeting_template.body is referenced by consumer greetings; missing coordinated migration

# (c) Missing exposed component
# Remove `export { GreeterCard } from './lib/GreeterCard';` from plugins/hello/frontend/src/index.ts
target/release/junius check
# → ✖ FE.EXPORTS.MATCH_MANIFEST: hello declares [exposes.components.GreeterCard] but doesn't export it

# (d) Drift in .sqlx/
# Delete plugins/hello/.sqlx/ then `cargo sqlx prepare --check --workspace`
# → fails

# Layer 2 (run the full CI integration job locally via act, or rely on CI)
# act -j integration
# → expected pass

# Update CI: confirm all four jobs run on a representative PR
git checkout -b test-ci-coverage
# Make a deliberate violation; push; verify each job catches it.

# CI green on main after this milestone lands.
```

After M12, the platform is hardened. M13 builds the first real domain plugin on top.
