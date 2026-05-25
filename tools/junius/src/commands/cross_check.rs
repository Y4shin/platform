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
//!
//! Migration SQL is parsed with `sqlparser` (`PostgreSQL` dialect). FK detection
//! covers inline (`col … REFERENCES other.table`) and table-level constraints;
//! non-FK cross-schema references in DDL are out of scope.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use junius_manifest::{PluginManifest, Severity, ValidationIssue, ValidationReport};
use sqlparser::ast::{ColumnOption, ObjectName, ReferentialAction, Statement, TableConstraint};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;

use junius::migrate::header::parse_requires;

use crate::commands::sync::{collect_proto_files, scan_proto_service_methods};

/// Run every cross-plugin rule over the enabled plugins.
pub fn check_cross_plugin(
    plugins: &BTreeMap<String, PluginManifest>,
    report: &mut ValidationReport,
) {
    let schema_owner = build_schema_owner(plugins);
    for (name, manifest) in plugins {
        check_dep_imports(name, manifest, plugins, report);
        check_rpc_methods(name, manifest, report);
        check_migrations(name, manifest, &schema_owner, report);
        check_fe_exports(name, manifest, report);
    }
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

/// schema name → the plugin that owns it (declares a table under it).
fn build_schema_owner(plugins: &BTreeMap<String, PluginManifest>) -> BTreeMap<String, String> {
    let mut owner = BTreeMap::new();
    for (name, manifest) in plugins {
        for table in manifest.exposes.tables.values() {
            owner
                .entry(table.schema.clone())
                .or_insert_with(|| name.clone());
        }
    }
    owner
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
}
