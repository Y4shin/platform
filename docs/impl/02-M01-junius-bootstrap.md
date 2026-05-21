# M01 — `junius` Bootstrap

## Goal

A working `junius` CLI binary that parses `plugin.toml` and `platform.toml` against typed schemas, validates them with `junius check`, and stubs every other subcommand. No code generation yet — there's nothing to generate for.

## Why now

Every subsequent milestone uses `junius` for scaffolding (`new plugin`, `new migration`) or composition (`sync`, `build`, `dev`, `migrate`). Building the CLI shell + manifest parser first removes the chicken-and-egg from later milestones, and the parser becomes the canonical specification of `plugin.toml` / `platform.toml` schemas referenced by [../design/06-plugin-shape.md](../design/06-plugin-shape.md) and [../design/05-repository-and-deployment-layout.md](../design/05-repository-and-deployment-layout.md) §5.5.

## Scope (in)

### Crate structure

```
tools/junius/
├── Cargo.toml
└── src/
    ├── main.rs                       # entry: parse CLI, dispatch
    ├── cli.rs                        # clap derive structs
    ├── manifest/
    │   ├── mod.rs                    # re-exports
    │   ├── plugin.rs                 # PluginManifest = plugin.toml schema
    │   ├── platform.rs               # PlatformManifest = platform.toml schema
    │   └── errors.rs
    ├── commands/
    │   ├── mod.rs
    │   ├── check.rs                  # implemented in this milestone
    │   ├── sync.rs                   # stubbed
    │   ├── build.rs                  # stubbed
    │   ├── dev.rs                    # stubbed
    │   ├── migrate.rs                # stubbed
    │   ├── plugin_cmd.rs             # `plugin list` implemented; rest stubbed
    │   └── new.rs                    # stubbed
    └── output.rs                     # plain-text + --format json formatters
```

### Subcommands (clap derive)

Every subcommand must parse and run. Stubs print "not yet implemented" + a `TODO: M<n>` hint and exit non-zero so CI catches accidental dependence on unfinished features.

```
junius
├── check [--manifest <path>] [--plugin <name>]   IMPLEMENTED
├── sync [--plugin <name>] [--dry-run]            stub → M03
├── build [--release]                             stub → M04
├── dev [--config <path>]                         stub → M04
├── migrate
│   ├── up                                        stub → M06
│   ├── down                                      stub → M06
│   └── status                                    stub → M06
├── plugin
│   ├── list                                      IMPLEMENTED
│   ├── info <name>                               IMPLEMENTED
│   ├── enable <name>                             stub → M11
│   └── disable <name>                            stub → M11
└── new
    ├── plugin <name>                             stub → M03
    ├── component <plugin> <name>                 stub → M07
    ├── rpc <plugin> <service>                    stub → M05
    ├── migration <plugin> <name>                 stub → M06
    └── permission <plugin> <perm>                stub → M07
```

Global flags: `--format <plain|json>` (default `plain`), `-v` / `--verbose`, `--cwd <path>` (run as if in `<path>`).

### Manifest schemas

Serde structs mirror the design exactly. Key types:

```rust
// manifest/plugin.rs
#[derive(Deserialize)]
pub struct PluginManifest {
    pub plugin: PluginIdentity,
    #[serde(default)]
    pub mount: PluginMount,
    #[serde(default)]
    pub dependencies: BTreeMap<String, PluginDep>,
    #[serde(default)]
    pub exposes: PluginExposes,
    #[serde(default)]
    pub permissions: BTreeMap<String, String>,
    #[serde(default)]
    pub requires: PluginRequires,
}

#[derive(Deserialize)]
pub struct PluginIdentity {
    pub name: String,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub manifest_schema: u32,
}

// Mount defaults are derived from plugin.name in a post-parse pass.
#[derive(Deserialize, Default)]
pub struct PluginMount {
    pub route_prefix: Option<String>,
    pub rpc_prefix: Option<String>,
    pub http_prefix: Option<String>,
}

#[derive(Deserialize)]
pub struct PluginDep {
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub tables: Vec<String>,
    #[serde(default)]
    pub rpc_methods: Vec<String>,
}

#[derive(Deserialize, Default)]
pub struct PluginExposes {
    #[serde(default)]
    pub components: BTreeMap<String, ExposedComponent>,
    #[serde(default)]
    pub tables: BTreeMap<String, ExposedTable>,
}

// platform.toml schema follows [../design/05-repository-and-deployment-layout.md §5.5].
```

### `junius check` implementation

For M01, `check` validates:
1. Manifest deserializes against the schema (TOML parse + serde structural check).
2. `plugin.name` matches the regex `^[a-z][a-z0-9_-]*$` (kebab/snake-friendly identifier).
3. `manifest_schema` is in the set of supported versions (initially just `1`).
4. Mount prefixes (if explicit) begin with `/p/`, `/rpc/`, `/h/` respectively.
5. Permission keys match `^<plugin>:<segment>$` where `<plugin>` matches `plugin.name`.
6. Exposed table names match the schema-qualified-table conventions (regex check).

Cross-plugin validation (dep graph, FK targets, etc.) lands in M09/M12. M01 only validates a single manifest in isolation.

`check` exits 0 on success, 1 on parse error, 2 on validation error. Error messages include the file path and line/column (via `toml::Spanned` for top-level fields).

### `junius plugin list` and `plugin info`

- `list` reads `platform.toml` (or the path passed via `--config`) and prints enabled plugins. At M01 there are no real plugins, so it runs against a **fixture** under `tools/junius/fixtures/sample-deployment/platform.toml` that lists fake plugin names. The point is to exercise the `platform.toml` parser end-to-end.
- `info <name>` reads the named plugin's `plugin.toml` (located by convention at `plugins/<name>/plugin.toml`) and prints a summary: mount points, declared deps, exposed components, declared permissions, declared capabilities.

### Tests

- `tests/cli.rs` — `assert_cmd`-based integration tests invoking `junius` as a child process. Snapshot the `--help` output via `insta`.
- `tests/manifest.rs` — table-driven tests over `fixtures/` directory: valid manifests parse successfully, each invalid fixture fails with the expected error category.

Fixture directory:
```
tools/junius/fixtures/
├── plugins/
│   ├── valid-minimal/plugin.toml
│   ├── valid-full/plugin.toml           # exercises every field
│   ├── invalid-bad-name/plugin.toml
│   ├── invalid-missing-schema/plugin.toml
│   └── invalid-wrong-mount-prefix/plugin.toml
└── deployments/
    ├── valid-minimal/platform.toml
    └── invalid-undefined-plugin/platform.toml
```

## Scope (out)

- No file generation (sync). Even though the `sync` subcommand exists as a stub, it does nothing.
- No buf invocation, no cargo invocation, no pnpm invocation. `build` and `dev` are stubs.
- No source resolution (`[source]` block in `platform.toml`). Parse it, but don't act on it — that's M11.
- No cross-manifest validation (dep graphs). Manifests are validated in isolation.

## Library choices — confirm with user before starting

| Choice | Proposed default | Rationale | Downstream milestones to update if changed |
|---|---|---|---|
| **CLI framework** | `clap` (derive macros) | De facto Rust CLI standard; subcommand handling fits the noun-verb structure from [../design/07-junius.md](../design/07-junius.md) §7.6 | junius only |
| **TOML parser** | `toml` crate via `serde` | Standard; `Spanned<T>` gives line/col errors for free | M02 (`plugin_metadata!` parses the same file), M07 (re-uses schema), M11 (parses `[source]`) |
| **Snapshot tests** | `insta` | Best-in-class for CLI output testing; trivial review workflow | junius only |
| **CLI test harness** | `assert_cmd` + `predicates` | Standard Rust integration testing for binaries | junius only |
| **Output format** | Plain text default, `--format json` flag | Matches design §7.6 | Every milestone that adds new junius output |
| **Error reporting** | `miette` for diagnostic rendering | Pretty errors with source-span highlighting; "fancy" feature when in a TTY, plain JSON when `--format json` | Every milestone that emits user-facing errors from junius |
| **Logging inside junius** | `tracing` + `tracing-subscriber` (env-filter via `RUST_LOG`) | Same stack as the host, learned once | Every milestone that adds junius logs |

## Open questions resolved

None blocking M01. The "v0 milestone definition" open question is partially answered by this plan (M05 = v0); M01 contributes the CLI piece.

## Verification

```bash
# Build junius
cargo build -p junius --release

# Help works
target/release/junius --help
target/release/junius check --help
target/release/junius plugin --help

# Manifest parsing
target/release/junius check --manifest tools/junius/fixtures/plugins/valid-full/plugin.toml
# → exits 0; prints "OK" (or JSON {"ok":true} with --format json)

target/release/junius check --manifest tools/junius/fixtures/plugins/invalid-bad-name/plugin.toml
# → exits 2; prints a diagnostic pointing at the offending `name` field

# Deployment config parsing
target/release/junius plugin list --config tools/junius/fixtures/deployments/valid-minimal/platform.toml
# → prints the enabled plugins listed in the fixture

# Tests pass
cargo test -p junius
# (includes insta snapshot tests; run `cargo insta review` if any snapshots changed)
```

The CI workflow added in M00 gets a new step: `cargo test -p junius`.
