//! Cross-plugin validation for `junius check` (M09).
//!
//! Runs when checking a *deployment* manifest (`platform.toml`), with every
//! enabled plugin loaded, so it can validate references that span the plugin
//! boundary against the declarations that authorize them:
//!
//! - `DEP.UNDECLARED` — a frontend import of `@junius/plugin-<other>` /
//!   `@junius/generated/<other>/…`, or a Rust `<other>_plugin::` path, must have a
//!   matching `[dependencies.<other>]`.
//! - `RPC.UNDECLARED` — each `[dependencies.<dep>].rpc_methods` entry must name a
//!   real `Service.Method` in `<dep>`'s proto.
//! - `SQL.REQUIRES.MISSING` — a migration whose SQL references another plugin's
//!   schema (via a cross-schema FK) must declare `-- @requires <owner>:<migration>`.
//! - `FK.OPTIONAL.NULLABLE` — an FK into an *optional* dependency's schema must be
//!   nullable (so the row survives the dependency being absent).
//! - `FK.CROSS.CASCADE` — an FK across a plugin boundary must not `ON DELETE/UPDATE
//!   CASCADE` (one plugin must not silently delete another's rows).
//! - `FE.EXPORTS.MATCH_MANIFEST` — every `[exposes.components.X]` must have a
//!   matching named export `X` in the plugin's `frontend/src/index.ts`.
//! - `SQL.PRIVATE_TABLE_ACCESS` — a cross-plugin schema-qualified table reference
//!   (in a migration *or* an `sqlx::query*!` macro) must point at a table the
//!   owner exposes (`[exposes.tables]`) and that the consumer declares in
//!   `[dependencies.<owner>].tables`.
//! - `SQL.EXPOSED.NO_BREAKING` — a breaking change (DROP/RENAME COLUMN, DROP
//!   TABLE, narrowing type, ADD NOT NULL) to an exposed table, in a migration
//!   *added vs the diff base*, is rejected unless every consumer that declares
//!   the table also ships a migration in the same change set.
//!
//! Migration SQL is parsed with `sqlparser` (`PostgreSQL` dialect). FK detection
//! covers inline (`col … REFERENCES other.table`) and table-level constraints;
//! `SQL.PRIVATE_TABLE_ACCESS` additionally collects *every* schema-qualified table
//! reference via `visit_relations`.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use junius_manifest::{PluginManifest, Severity, ValidationIssue, ValidationReport};
use sqlparser::ast::{
    AlterColumnOperation, AlterTableOperation, ColumnOption, DataType, ObjectName, ObjectType,
    ReferentialAction, SchemaName, Statement, TableConstraint, visit_relations,
};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;

use junius::migrate::header::parse_requires;

use crate::commands::sync::scan_proto_service_methods;
use junius_rpc_meta::collect_proto_files;

/// Run every cross-plugin rule over the enabled plugins. `base` is the git ref
/// to diff against for `SQL.EXPOSED.NO_BREAKING` (no-op outside a git repo).
pub fn check_cross_plugin(
    plugins: &BTreeMap<String, PluginManifest>,
    base: &str,
    report: &mut ValidationReport,
) {
    let index = build_schema_index(plugins);
    for (name, manifest) in plugins {
        check_dep_imports(name, manifest, plugins, report);
        check_rpc_methods(name, manifest, report);
        check_migrations(name, manifest, &index.owner, report);
        check_fe_exports(name, manifest, report);
        check_private_table_access(name, manifest, &index, report);
    }
    check_no_breaking(plugins, &index, base, report);
}

/// `STORAGE.BUCKET.UNMAPPED` — every logical bucket a plugin declares
/// (`[storage.buckets.<name>]`) must be mapped to a physical bucket in the
/// deployment's `[config.storage.mapping]` as `"<plugin>:<logical>"`.
pub fn check_storage_mapping(
    plugins: &BTreeMap<String, PluginManifest>,
    mapping_keys: &BTreeSet<String>,
    report: &mut ValidationReport,
) {
    for (name, manifest) in plugins {
        for logical in manifest.storage.buckets.keys() {
            let key = format!("{name}:{logical}");
            if !mapping_keys.contains(&key) {
                err(
                    report,
                    "STORAGE.BUCKET.UNMAPPED",
                    format!("storage.buckets.{logical}"),
                    format!(
                        "logical bucket \"{key}\" is not mapped to a physical bucket in [config.storage.mapping]"
                    ),
                );
            }
        }
    }
}

/// Cross-plugin schema knowledge derived from manifests + migrations.
struct SchemaIndex {
    /// SQL schema name → the plugin that owns it.
    owner: BTreeMap<String, String>,
    /// plugin → the `schema.table` identifiers it exposes (`[exposes.tables]`).
    exposed: BTreeMap<String, BTreeSet<String>>,
}

/// Build the schema index. Ownership comes from each plugin's exposed-table
/// schemas *and* the schemas it creates in its migrations (private tables aren't
/// in `[exposes]`, so manifests alone are insufficient).
fn build_schema_index(plugins: &BTreeMap<String, PluginManifest>) -> SchemaIndex {
    let mut owner = BTreeMap::new();
    let mut exposed: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (name, manifest) in plugins {
        let mut exp = BTreeSet::new();
        for (table, decl) in &manifest.exposes.tables {
            exp.insert(format!("{}.{}", decl.schema, table));
            owner
                .entry(decl.schema.clone())
                .or_insert_with(|| name.clone());
        }
        exposed.insert(name.clone(), exp);
        for schema in created_schemas(name) {
            owner.entry(schema).or_insert_with(|| name.clone());
        }
    }
    SchemaIndex { owner, exposed }
}

/// Schemas a plugin creates in its migrations (`CREATE SCHEMA` + `CREATE TABLE
/// schema.t`). Unparseable migrations are skipped (the runner surfaces them).
fn created_schemas(plugin: &str) -> BTreeSet<String> {
    let mut schemas = BTreeSet::new();
    for sql in plugin_migration_sql(plugin) {
        let Ok(stmts) = Parser::parse_sql(&PostgreSqlDialect {}, &sql) else {
            continue;
        };
        for stmt in stmts {
            match stmt {
                Statement::CreateSchema { schema_name, .. } => {
                    if let Some(s) = schema_name_ident(&schema_name) {
                        schemas.insert(s);
                    }
                }
                Statement::CreateTable(ct) => {
                    if let Some(schema) = qualifier(&ct.name) {
                        schemas.insert(schema);
                    }
                }
                _ => {}
            }
        }
    }
    schemas
}

fn schema_name_ident(s: &SchemaName) -> Option<String> {
    match s {
        SchemaName::Simple(name) | SchemaName::NamedAuthorization(name, _) => {
            name.0.last().map(|i| i.value.clone())
        }
        SchemaName::UnnamedAuthorization(_) => None,
    }
}

/// The schema qualifier of an object name (`schema.table` → `schema`), if any.
fn qualifier(name: &ObjectName) -> Option<String> {
    let parts = &name.0;
    (parts.len() >= 2).then(|| parts[parts.len() - 2].value.clone())
}

/// The `.up.sql` migration contents for a plugin, in filename order.
fn plugin_migration_sql(plugin: &str) -> Vec<String> {
    let dir = PathBuf::from("plugins").join(plugin).join("migrations");
    let mut files = Vec::new();
    collect_files(&dir, &["sql"], &mut files);
    files.sort();
    files
        .into_iter()
        .filter(|f| {
            f.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|n| n.ends_with(".up.sql"))
        })
        .filter_map(|f| std::fs::read_to_string(f).ok())
        .collect()
}

fn err(report: &mut ValidationReport, code: &'static str, path: String, message: String) {
    report.issues.push(ValidationIssue {
        severity: Severity::Error,
        code,
        path,
        message,
    });
}

// --- DEP.UNDECLARED ----------------------------------------------------------

fn check_dep_imports(
    name: &str,
    manifest: &PluginManifest,
    plugins: &BTreeMap<String, PluginManifest>,
    report: &mut ValidationReport,
) {
    let declared: BTreeSet<&str> = manifest.dependencies.keys().map(String::as_str).collect();
    let mut used: BTreeSet<String> = BTreeSet::new();

    // Frontend TS: `@junius/plugin-<other>` and `@junius/generated/<other>/…`.
    // Skip generated/ (derived; its imports mirror declared deps anyway).
    let fe_src = PathBuf::from("plugins")
        .join(name)
        .join("frontend")
        .join("src");
    let mut ts_files = Vec::new();
    collect_files(&fe_src, &["ts", "tsx"], &mut ts_files);
    let re_plugin = regex_plugin();
    let re_generated = regex_generated();
    for file in &ts_files {
        if file.components().any(|c| c.as_os_str() == "generated") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(file) else {
            continue;
        };
        for caps in re_plugin
            .captures_iter(&content)
            .chain(re_generated.captures_iter(&content))
        {
            used.insert(caps[1].to_string());
        }
    }

    // Rust: a `<other>_plugin::` path to another plugin's crate.
    let rs_src = PathBuf::from("plugins").join(name).join("src");
    let mut rs_files = Vec::new();
    collect_files(&rs_src, &["rs"], &mut rs_files);
    let rust_src: String = rs_files
        .iter()
        .filter_map(|f| std::fs::read_to_string(f).ok())
        .collect();
    for other in plugins.keys() {
        let module = format!("{}_plugin::", other.replace('-', "_"));
        if rust_src.contains(&module) {
            used.insert(other.clone());
        }
    }

    for other in used {
        if other != name && !declared.contains(other.as_str()) {
            err(
                report,
                "DEP.UNDECLARED",
                format!("plugins/{name}"),
                format!(
                    "uses plugin {other:?} (import or `{other}_plugin::`) without a matching [dependencies.{other}]"
                ),
            );
        }
    }
}

// --- RPC.UNDECLARED ----------------------------------------------------------

fn check_rpc_methods(name: &str, manifest: &PluginManifest, report: &mut ValidationReport) {
    for (dep_name, dep) in &manifest.dependencies {
        if dep.rpc_methods.is_empty() {
            continue;
        }
        let services = plugin_proto_services(dep_name);
        for spec in &dep.rpc_methods {
            let Some((service, method)) = spec.split_once('.') else {
                err(
                    report,
                    "RPC.UNDECLARED",
                    format!("plugins/{name}/dependencies.{dep_name}.rpc_methods"),
                    format!("rpc_methods entry {spec:?} must be \"Service.Method\""),
                );
                continue;
            };
            let found = services
                .iter()
                .any(|(svc, methods)| svc == service && methods.iter().any(|m| m == method));
            if !found {
                err(
                    report,
                    "RPC.UNDECLARED",
                    format!("plugins/{name}/dependencies.{dep_name}.rpc_methods"),
                    format!(
                        "declared RPC {spec:?} is not a method of any service in plugin {dep_name:?}"
                    ),
                );
            }
        }
    }
}

/// All `(service, [method])` declared by a plugin's protos (`PascalCase` names).
fn plugin_proto_services(plugin: &str) -> Vec<(String, Vec<String>)> {
    let mut files = Vec::new();
    collect_proto_files(
        &PathBuf::from("plugins").join(plugin).join("proto"),
        &mut files,
    );
    let mut out = Vec::new();
    for file in files {
        if let Ok(content) = std::fs::read_to_string(&file) {
            out.extend(scan_proto_service_methods(&content));
        }
    }
    out
}

// --- FE.EXPORTS.MATCH_MANIFEST -----------------------------------------------

/// Every component a plugin declares in `[exposes.components]` must be a named
/// export of its `frontend/src/index.ts` (a wrong module path is left to the TS
/// type-check). Plugins with no exposed components are skipped.
fn check_fe_exports(name: &str, manifest: &PluginManifest, report: &mut ValidationReport) {
    if manifest.exposes.components.is_empty() {
        return;
    }
    let index = PathBuf::from("plugins")
        .join(name)
        .join("frontend")
        .join("src")
        .join("index.ts");
    let path = format!("plugins/{name}/frontend/src/index.ts");
    let Ok(content) = std::fs::read_to_string(&index) else {
        for comp in manifest.exposes.components.keys() {
            err(
                report,
                "FE.EXPORTS.MATCH_MANIFEST",
                path.clone(),
                format!(
                    "component {comp:?} is declared in [exposes.components] but {path} is missing or unreadable"
                ),
            );
        }
        return;
    };
    let exported = scan_named_exports(&content);
    for comp in manifest.exposes.components.keys() {
        if !exported.contains(comp) {
            err(
                report,
                "FE.EXPORTS.MATCH_MANIFEST",
                path.clone(),
                format!(
                    "component {comp:?} is declared in [exposes.components] but not exported from index.ts"
                ),
            );
        }
    }
}

/// Collect the value-level named exports from an `index.ts`: the names inside
/// `export { … }` (handling `A as B` → `B`, multiline), skipping type-only
/// exports (`export type { … }` and inline `type X`).
fn scan_named_exports(content: &str) -> BTreeSet<String> {
    let re = regex_named_export();
    let mut out = BTreeSet::new();
    for caps in re.captures_iter(content) {
        if caps.get(1).is_some() {
            continue; // `export type { … }`
        }
        for entry in caps[2].split(',') {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            // Inline type export inside a value export list: `export { type X }`.
            if entry.split_whitespace().next() == Some("type") {
                continue;
            }
            // `Name as Alias` exports `Alias`; otherwise the name itself.
            let exported = match entry.split_once(" as ") {
                Some((_, alias)) => alias.trim(),
                None => entry,
            };
            out.insert(exported.to_string());
        }
    }
    out
}

// --- SQL rules (sqlparser) ---------------------------------------------------

fn check_migrations(
    name: &str,
    manifest: &PluginManifest,
    schema_owner: &BTreeMap<String, String>,
    report: &mut ValidationReport,
) {
    let dir = PathBuf::from("plugins").join(name).join("migrations");
    let mut files = Vec::new();
    collect_files(&dir, &["sql"], &mut files);
    files.sort();
    for file in files {
        let Some(fname) = file.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !fname.ends_with(".up.sql") {
            continue;
        }
        let Ok(sql) = std::fs::read_to_string(&file) else {
            continue;
        };
        let required: BTreeSet<String> =
            parse_requires(&sql).into_iter().map(|e| e.plugin).collect();
        let Ok(statements) = Parser::parse_sql(&PostgreSqlDialect {}, &sql) else {
            // Unparseable SQL: the migration runner will surface it; skip here.
            continue;
        };
        for stmt in statements {
            let Statement::CreateTable(ct) = stmt else {
                continue;
            };
            let not_null: BTreeMap<String, bool> = ct
                .columns
                .iter()
                .map(|c| {
                    (
                        c.name.value.clone(),
                        c.options
                            .iter()
                            .any(|o| matches!(o.option, ColumnOption::NotNull)),
                    )
                })
                .collect();

            for col in &ct.columns {
                for opt in &col.options {
                    if let ColumnOption::ForeignKey {
                        foreign_table,
                        on_delete,
                        on_update,
                        ..
                    } = &opt.option
                    {
                        check_fk(
                            name,
                            manifest,
                            schema_owner,
                            &required,
                            fname,
                            foreign_table,
                            *on_delete,
                            *on_update,
                            std::slice::from_ref(&col.name.value),
                            &not_null,
                            report,
                        );
                    }
                }
            }

            for constraint in &ct.constraints {
                if let TableConstraint::ForeignKey {
                    foreign_table,
                    on_delete,
                    on_update,
                    columns,
                    ..
                } = constraint
                {
                    let local: Vec<String> = columns.iter().map(|c| c.value.clone()).collect();
                    check_fk(
                        name,
                        manifest,
                        schema_owner,
                        &required,
                        fname,
                        foreign_table,
                        *on_delete,
                        *on_update,
                        &local,
                        &not_null,
                        report,
                    );
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn check_fk(
    name: &str,
    manifest: &PluginManifest,
    schema_owner: &BTreeMap<String, String>,
    required: &BTreeSet<String>,
    migration: &str,
    foreign_table: &ObjectName,
    on_delete: Option<ReferentialAction>,
    on_update: Option<ReferentialAction>,
    local_columns: &[String],
    not_null: &BTreeMap<String, bool>,
    report: &mut ValidationReport,
) {
    // Schema-qualified target only; an unqualified table is same-schema.
    let parts = &foreign_table.0;
    if parts.len() < 2 {
        return;
    }
    let schema = parts[parts.len() - 2].value.as_str();
    let Some(owner) = schema_owner.get(schema) else {
        return; // unknown schema (e.g. `platform`): not a plugin boundary here.
    };
    if owner == name {
        return; // own schema
    }
    let path = format!("plugins/{name}/migrations/{migration}");

    if !required.contains(owner) {
        err(
            report,
            "SQL.REQUIRES.MISSING",
            path.clone(),
            format!(
                "FK references {schema}.* (owned by {owner:?}) but the migration has no `-- @requires {owner}:<migration>`"
            ),
        );
    }

    if matches!(on_delete, Some(ReferentialAction::Cascade))
        || matches!(on_update, Some(ReferentialAction::Cascade))
    {
        err(
            report,
            "FK.CROSS.CASCADE",
            path.clone(),
            format!(
                "cross-plugin FK into {owner:?}'s schema must not use ON DELETE/UPDATE CASCADE"
            ),
        );
    }

    let optional = manifest.dependencies.get(owner).is_some_and(|d| d.optional);
    if optional {
        let any_not_null = local_columns
            .iter()
            .any(|c| not_null.get(c).copied().unwrap_or(false));
        if any_not_null {
            err(
                report,
                "FK.OPTIONAL.NULLABLE",
                path,
                format!(
                    "FK into optional dependency {owner:?} must be nullable (column(s) {local_columns:?} are NOT NULL)"
                ),
            );
        }
    }
}

// --- SQL.PRIVATE_TABLE_ACCESS ------------------------------------------------

/// Every cross-plugin schema-qualified table reference — in a migration or an
/// `sqlx::query*!` macro — must point at a table the owner exposes and that the
/// consumer declares in `[dependencies.<owner>].tables`.
fn check_private_table_access(
    name: &str,
    manifest: &PluginManifest,
    index: &SchemaIndex,
    report: &mut ValidationReport,
) {
    let mut refs: BTreeSet<(String, String)> = BTreeSet::new();

    // Migrations.
    for sql in plugin_migration_sql(name) {
        collect_sql_relations(&sql, &mut refs);
    }
    // Repo queries: SQL inside sqlx `query*!` macros under `src/`.
    let rs_dir = PathBuf::from("plugins").join(name).join("src");
    let mut rs_files = Vec::new();
    collect_files(&rs_dir, &["rs"], &mut rs_files);
    for file in rs_files {
        let Ok(src) = std::fs::read_to_string(&file) else {
            continue;
        };
        for sql in extract_sqlx_queries(&src) {
            collect_sql_relations(&sql, &mut refs);
        }
    }

    for (schema, table) in &refs {
        let Some(owner) = index.owner.get(schema) else {
            continue; // unowned schema (platform/public) — not a plugin boundary.
        };
        if owner == name {
            continue; // own schema
        }
        let qualified = format!("{schema}.{table}");
        let path = format!("plugins/{name}");
        let exposed = index
            .exposed
            .get(owner)
            .is_some_and(|s| s.contains(&qualified));
        if !exposed {
            err(
                report,
                "SQL.PRIVATE_TABLE_ACCESS",
                path,
                format!(
                    "references {qualified}, which {owner:?} does not expose ([exposes.tables])"
                ),
            );
            continue;
        }
        let declared = manifest
            .dependencies
            .get(owner)
            .is_some_and(|d| d.tables.iter().any(|t| t == table));
        if !declared {
            err(
                report,
                "SQL.PRIVATE_TABLE_ACCESS",
                path,
                format!(
                    "references exposed table {qualified} without declaring it in [dependencies.{owner}].tables"
                ),
            );
        }
    }
}

/// Collect every schema-qualified (`schema.table`) relation in a SQL string.
/// `visit_relations` covers query positions (FROM/JOIN/INTO, subqueries, CTEs);
/// FK targets inside `CREATE TABLE` are not reported as relations, so inline and
/// table-level FK `REFERENCES` are pulled out explicitly. Unparseable SQL is
/// skipped.
fn collect_sql_relations(sql: &str, out: &mut BTreeSet<(String, String)>) {
    let Ok(statements) = Parser::parse_sql(&PostgreSqlDialect {}, sql) else {
        return;
    };
    let _ = visit_relations(&statements, |name| {
        if let Some(pair) = schema_table(name) {
            out.insert(pair);
        }
        ControlFlow::<()>::Continue(())
    });
    for stmt in &statements {
        let Statement::CreateTable(ct) = stmt else {
            continue;
        };
        for col in &ct.columns {
            for opt in &col.options {
                if let ColumnOption::ForeignKey { foreign_table, .. } = &opt.option {
                    out.extend(schema_table(foreign_table));
                }
            }
        }
        for constraint in &ct.constraints {
            if let TableConstraint::ForeignKey { foreign_table, .. } = constraint {
                out.extend(schema_table(foreign_table));
            }
        }
    }
}

/// `(schema, table)` for a schema-qualified object name; `None` if unqualified.
fn schema_table(name: &ObjectName) -> Option<(String, String)> {
    let parts = &name.0;
    (parts.len() >= 2).then(|| {
        (
            parts[parts.len() - 2].value.clone(),
            parts[parts.len() - 1].value.clone(),
        )
    })
}

// --- SQL.EXPOSED.NO_BREAKING -------------------------------------------------

/// A breaking change to an exposed table — in a migration *added vs `base`* — is
/// allowed only if every consumer that declares the table also has a migration
/// in the same change set (the spec's coordination heuristic). No-op when git or
/// the base ref is unavailable.
fn check_no_breaking(
    plugins: &BTreeMap<String, PluginManifest>,
    index: &SchemaIndex,
    base: &str,
    report: &mut ValidationReport,
) {
    let Some(added) = added_migrations(base) else {
        return;
    };
    if added.is_empty() {
        return;
    }
    let added_plugins: BTreeSet<String> = added
        .iter()
        .filter_map(|p| plugin_of_migration(p))
        .collect();

    for file in &added {
        let Some(producer) = plugin_of_migration(file) else {
            continue;
        };
        let Ok(sql) = std::fs::read_to_string(file) else {
            continue;
        };
        let Ok(statements) = Parser::parse_sql(&PostgreSqlDialect {}, &sql) else {
            continue;
        };
        for stmt in &statements {
            for (schema, table, reason) in breaking_changes(stmt) {
                let qualified = format!("{schema}.{table}");
                let exposed = index
                    .exposed
                    .get(&producer)
                    .is_some_and(|s| s.contains(&qualified));
                if !exposed {
                    continue; // private table: the producer's own business.
                }
                for (consumer, manifest) in plugins {
                    if consumer == &producer {
                        continue;
                    }
                    let declares = manifest
                        .dependencies
                        .get(&producer)
                        .is_some_and(|d| d.tables.iter().any(|t| t == &table));
                    if declares && !added_plugins.contains(consumer) {
                        err(
                            report,
                            "SQL.EXPOSED.NO_BREAKING",
                            file.display().to_string(),
                            format!(
                                "{reason} on exposed table {qualified} (owned by {producer:?}), but consumer {consumer:?} declares it and has no coordinating migration in this change"
                            ),
                        );
                    }
                }
            }
        }
    }
}

/// Migration `.up.sql` files added vs `base` (committed + untracked),
/// repo-root-relative. `None` when not a git repo, `base` is unresolvable, or
/// git is unavailable — the rule then no-ops rather than failing the check.
fn added_migrations(base: &str) -> Option<BTreeSet<PathBuf>> {
    use crate::source::git;
    git(&["rev-parse", "--git-dir"]).ok()?;
    git(&["rev-parse", "--verify", "--quiet", base]).ok()?;

    let mut files = BTreeSet::new();
    let committed = git(&[
        "diff",
        "--diff-filter=A",
        "--name-only",
        &format!("{base}...HEAD"),
        "--",
        "plugins",
    ]);
    let untracked = git(&[
        "ls-files",
        "--others",
        "--exclude-standard",
        "--",
        "plugins",
    ]);
    for out in [committed, untracked].into_iter().flatten() {
        for line in out.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let path = PathBuf::from(line);
            if is_migration_file(&path) {
                files.insert(path);
            }
        }
    }
    Some(files)
}

fn is_migration_file(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "migrations")
        && path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|n| n.ends_with(".up.sql"))
}

/// The `<name>` in `plugins/<name>/migrations/…`.
fn plugin_of_migration(path: &Path) -> Option<String> {
    let comps: Vec<String> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let i = comps.iter().position(|c| c == "plugins")?;
    comps.get(i + 1).cloned()
}

/// Breaking `(schema, table, reason)` changes in one statement (exposed-table
/// filtering happens at the call site).
fn breaking_changes(stmt: &Statement) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    match stmt {
        Statement::AlterTable {
            name, operations, ..
        } => {
            if let Some((schema, table)) = schema_table(name) {
                for op in operations {
                    if let Some(reason) = breaking_alter_op(op) {
                        out.push((schema.clone(), table.clone(), reason));
                    }
                }
            }
        }
        Statement::Drop {
            object_type: ObjectType::Table,
            names,
            ..
        } => {
            for n in names {
                if let Some((schema, table)) = schema_table(n) {
                    out.push((schema, table, "drops the table".to_string()));
                }
            }
        }
        _ => {}
    }
    out
}

/// Classify one `ALTER TABLE` operation; `Some(reason)` if breaking. Type changes
/// are conservatively breaking unless the target is an obvious widening (the old
/// type isn't reconstructed here).
fn breaking_alter_op(op: &AlterTableOperation) -> Option<String> {
    match op {
        AlterTableOperation::DropColumn { column_name, .. } => {
            Some(format!("drops column {}", column_name.value))
        }
        AlterTableOperation::RenameColumn {
            old_column_name,
            new_column_name,
        } => Some(format!(
            "renames column {} to {}",
            old_column_name.value, new_column_name.value
        )),
        AlterTableOperation::ChangeColumn {
            old_name, new_name, ..
        } => Some(format!(
            "changes column {} to {}",
            old_name.value, new_name.value
        )),
        AlterTableOperation::AlterColumn { column_name, op } => match op {
            AlterColumnOperation::SetNotNull => {
                Some(format!("adds NOT NULL to column {}", column_name.value))
            }
            AlterColumnOperation::SetDataType { data_type, .. } if !is_widening(data_type) => {
                Some(format!(
                    "narrows the type of column {} to {data_type}",
                    column_name.value
                ))
            }
            _ => None,
        },
        _ => None, // AddColumn, DropNotNull, defaults, indexes, etc. are compatible.
    }
}

/// Obvious widening targets (compatible). Conservative: anything else is treated
/// as a narrowing/breaking type change.
fn is_widening(dt: &DataType) -> bool {
    matches!(
        dt,
        DataType::Text
            | DataType::Varchar(None)
            | DataType::CharacterVarying(None)
            | DataType::BigInt(_)
    )
}

/// Pull the SQL string out of every sqlx `query*!` macro in a Rust source file.
/// The SQL is the first string-literal argument (`query_as!` puts a type path
/// first; the first *string* literal is still the SQL). `LitStr::value()`
/// normalises raw/escaped/`\`-continued strings.
fn extract_sqlx_queries(rust_src: &str) -> Vec<String> {
    let Ok(file) = syn::parse_file(rust_src) else {
        return Vec::new();
    };
    let mut collector = QueryCollector::default();
    syn::visit::Visit::visit_file(&mut collector, &file);
    collector.queries
}

#[derive(Default)]
struct QueryCollector {
    queries: Vec<String>,
}

impl<'ast> syn::visit::Visit<'ast> for QueryCollector {
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        let is_query = mac.path.segments.last().is_some_and(|s| {
            matches!(
                s.ident.to_string().as_str(),
                "query"
                    | "query_as"
                    | "query_scalar"
                    | "query_unchecked"
                    | "query_as_unchecked"
                    | "query_scalar_unchecked"
            )
        });
        if is_query {
            if let Ok(args) = mac.parse_body_with(
                syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated,
            ) {
                for arg in args {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = arg
                    {
                        self.queries.push(s.value());
                        break; // first string literal is the SQL
                    }
                }
            }
        }
        syn::visit::visit_macro(self, mac);
    }
}

// --- helpers -----------------------------------------------------------------

fn collect_files(dir: &std::path::Path, exts: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, exts, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| exts.contains(&e))
        {
            out.push(path);
        }
    }
}

#[allow(
    clippy::unwrap_used,
    reason = "compile-constant regexes are known-valid"
)]
fn regex_plugin() -> regex::Regex {
    regex::Regex::new(r"@junius/plugin-([a-z0-9_-]+)").unwrap()
}

#[allow(
    clippy::unwrap_used,
    reason = "compile-constant regexes are known-valid"
)]
fn regex_generated() -> regex::Regex {
    regex::Regex::new(r"@junius/generated/([a-z0-9_-]+)/").unwrap()
}

#[allow(
    clippy::unwrap_used,
    reason = "compile-constant regexes are known-valid"
)]
fn regex_named_export() -> regex::Regex {
    // `export { … }` (group 2 = body); group 1 present means `export type { … }`.
    regex::Regex::new(r"export\s+(type\s+)?\{([^}]*)\}").unwrap()
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "tests panic on unexpected failures")]
mod tests {
    use super::*;

    fn manifest_with_bucket(name: &str, bucket: &str) -> PluginManifest {
        let toml = format!(
            "[plugin]\nname = \"{name}\"\ndisplay_name = \"X\"\nmanifest_schema = 1\n\
             [storage.buckets.{bucket}]\n"
        );
        PluginManifest::parse(&toml).expect("valid manifest")
    }

    #[test]
    fn unmapped_bucket_is_flagged_and_mapped_bucket_passes() {
        let mut plugins = BTreeMap::new();
        plugins.insert(
            "hello".to_string(),
            manifest_with_bucket("hello", "attachments"),
        );

        // No mapping → violation.
        let mut report = ValidationReport::default();
        check_storage_mapping(&plugins, &BTreeSet::new(), &mut report);
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.code == "STORAGE.BUCKET.UNMAPPED"),
            "expected STORAGE.BUCKET.UNMAPPED, got {:?}",
            report.issues
        );

        // With the mapping → clean.
        let mut mapped = BTreeSet::new();
        mapped.insert("hello:attachments".to_string());
        let mut report = ValidationReport::default();
        check_storage_mapping(&plugins, &mapped, &mut report);
        assert!(
            report.is_ok(),
            "mapped bucket should pass: {:?}",
            report.issues
        );
    }

    #[test]
    fn extracts_sql_from_query_macro_variants() {
        let src = r##"
            fn a() { let _ = sqlx::query!("SELECT 1 FROM x"); }
            fn b() { let _ = query_as!(MyRow, "SELECT id FROM y", arg); }
            fn c() { let _ = sqlx::query_scalar!(r#"SELECT count(*)
                       FROM z"#); }
            fn d() { let _ = query!("INSERT INTO w (a) VALUES ($1) \
                       RETURNING id", v); }
            fn not_sql() { println!("query! looking string"); }
        "##;
        let q = extract_sqlx_queries(src);
        assert_eq!(q.len(), 4, "got {q:?}");
        assert!(q[0].contains("FROM x"));
        // query_as!: the SQL is the literal after the type path, not `MyRow`.
        assert!(q[1].contains("FROM y") && !q[1].contains("MyRow"));
        assert!(q[2].contains("FROM z")); // raw multiline
        assert!(q[3].contains("RETURNING id")); // `\`-continuation
    }

    #[test]
    fn classifies_breaking_vs_compatible_changes() {
        let reasons = |sql: &str| -> Vec<String> {
            Parser::parse_sql(&PostgreSqlDialect {}, sql)
                .expect("parses")
                .iter()
                .flat_map(breaking_changes)
                .map(|(_, _, r)| r)
                .collect()
        };
        // Breaking.
        for sql in [
            "ALTER TABLE a.t DROP COLUMN c",
            "ALTER TABLE a.t RENAME COLUMN c TO d",
            "DROP TABLE a.t",
            "ALTER TABLE a.t ALTER COLUMN c SET NOT NULL",
            "ALTER TABLE a.t ALTER COLUMN c TYPE varchar(10)",
        ] {
            assert!(!reasons(sql).is_empty(), "expected breaking: {sql}");
        }
        // Compatible.
        for sql in [
            "ALTER TABLE a.t ADD COLUMN c text",
            "ALTER TABLE a.t ALTER COLUMN c DROP NOT NULL",
            "ALTER TABLE a.t ALTER COLUMN c TYPE text",
            "CREATE INDEX idx ON a.t (c)",
            "DROP INDEX a.idx",
        ] {
            assert!(reasons(sql).is_empty(), "expected compatible: {sql}");
        }
    }

    #[test]
    fn collects_schema_qualified_relations_only() {
        let mut out = BTreeSet::new();
        collect_sql_relations(
            "SELECT * FROM hello.greeting g JOIN other.t ON g.id = t.id WHERE platform.fn(g.id)",
            &mut out,
        );
        assert!(out.contains(&("hello".to_string(), "greeting".to_string())));
        assert!(out.contains(&("other".to_string(), "t".to_string())));
        // `platform.fn(...)` is a function call, not a relation — not collected.
        assert!(!out.iter().any(|(s, _)| s == "platform"));
    }
}
