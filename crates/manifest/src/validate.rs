//! Semantic validation passes. These run *after* TOML deserialisation; they
//! check rules that aren't expressible in serde-derived schemas alone.
//!
//! The static regexes below use `.expect("static regex")`; clippy's
//! `expect_used` lint is allowed because these strings are compile-time
//! constants validated by the manifest crate's own tests.
#![allow(clippy::expect_used)]

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;

use crate::error::ValidationReport;
use crate::platform::PlatformManifest;
use crate::plugin::PluginManifest;

const SUPPORTED_MANIFEST_SCHEMAS: &[u32] = &[1];

static RE_PLUGIN_NAME: LazyLock<Regex> = LazyLock::new(|| {
    // Lowercase identifier: starts with a letter, then letters/digits/_/- allowed.
    Regex::new(r"^[a-z][a-z0-9_\-]*$").expect("static regex")
});

static RE_PERM_TAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9_]*$").expect("static regex"));

static RE_SCHEMA_IDENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9_]*$").expect("static regex"));

static RE_CAPABILITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z]+\.[a-z_]+$").expect("static regex"));

pub(crate) fn plugin(m: &PluginManifest, report: &mut ValidationReport) {
    let name = &m.plugin.name;

    if !RE_PLUGIN_NAME.is_match(name) {
        report.error(
            "PLUGIN.NAME.INVALID",
            "plugin.name",
            format!("plugin name {name:?} must match ^[a-z][a-z0-9_-]*$ (kebab/snake, lowercase)"),
        );
    }

    if !SUPPORTED_MANIFEST_SCHEMAS.contains(&m.plugin.manifest_schema) {
        report.error(
            "PLUGIN.SCHEMA.UNSUPPORTED",
            "plugin.manifest_schema",
            format!(
                "manifest_schema = {} is not in the supported set {:?}",
                m.plugin.manifest_schema, SUPPORTED_MANIFEST_SCHEMAS
            ),
        );
    }

    if let Some(p) = &m.mount.route_prefix {
        if !p.starts_with("/p/") {
            report.error(
                "MOUNT.ROUTE.PREFIX",
                "mount.route_prefix",
                format!("route_prefix {p:?} must start with \"/p/\""),
            );
        }
    }
    if let Some(p) = &m.mount.rpc_prefix {
        if !p.starts_with("/rpc/") {
            report.error(
                "MOUNT.RPC.PREFIX",
                "mount.rpc_prefix",
                format!("rpc_prefix {p:?} must start with \"/rpc/\""),
            );
        }
    }
    if let Some(p) = &m.mount.http_prefix {
        if !p.starts_with("/h/") {
            report.error(
                "MOUNT.HTTP.PREFIX",
                "mount.http_prefix",
                format!("http_prefix {p:?} must start with \"/h/\""),
            );
        }
    }

    for key in m.permissions.keys() {
        // Format: "<plugin.name>:<tail>"; tail = [a-z][a-z0-9_]*
        let valid = match key.split_once(':') {
            Some((prefix, tail)) => prefix == name && RE_PERM_TAIL.is_match(tail),
            None => false,
        };
        if !valid {
            report.error(
                "PERM.NAME.FORMAT",
                format!("permissions.{key:?}"),
                format!(
                    "permission key {key:?} must be \"{name}:<tail>\" where <tail> matches ^[a-z][a-z0-9_]*$"
                ),
            );
        }
    }

    for (table_name, table) in &m.exposes.tables {
        if !RE_SCHEMA_IDENT.is_match(&table.schema) {
            report.error(
                "EXPOSE.TABLE.SCHEMA",
                format!("exposes.tables.{table_name}.schema"),
                format!(
                    "exposed table schema {:?} must match ^[a-z][a-z0-9_]*$",
                    table.schema
                ),
            );
        }
    }

    for dep_name in m.dependencies.keys() {
        if !RE_PLUGIN_NAME.is_match(dep_name) {
            report.error(
                "DEP.KEY.FORMAT",
                format!("dependencies.{dep_name}"),
                format!("dependency key {dep_name:?} must match ^[a-z][a-z0-9_-]*$"),
            );
        }
    }

    for cap in &m.requires.capabilities {
        if !RE_CAPABILITY.is_match(cap) {
            report.error(
                "CAP.NAME.FORMAT",
                "requires.capabilities",
                format!("capability {cap:?} must match ^[a-z]+\\.[a-z_]+$ (e.g. \"db.read\")"),
            );
        }
    }
}

pub(crate) fn platform(m: &PlatformManifest, report: &mut ValidationReport) {
    let s = &m.source;
    let repo_set = s.repo.is_some();
    let rev_set = s.rev.is_some();
    let path_set = s.path.is_some();

    // Exactly one of (repo+rev) or path.
    let git_form = repo_set && rev_set && !path_set;
    let path_form = path_set && !repo_set && !rev_set;
    if !(git_form || path_form) {
        report.error(
            "SOURCE.ONE_OF",
            "source",
            "exactly one of `path = \"...\"` or (`repo = \"git+...\"` + `rev = \"...\"`) must be set"
                .to_string(),
        );
    }

    if let Some(repo) = &s.repo {
        if !repo.starts_with("git+") {
            report.error(
                "SOURCE.REPO.SCHEME",
                "source.repo",
                format!("source.repo {repo:?} must start with \"git+\""),
            );
        }
    }

    let mut seen = BTreeSet::new();
    for name in &m.plugins.enabled {
        if !seen.insert(name.as_str()) {
            report.error(
                "PLUGINS.ENABLED.UNIQUE",
                "plugins.enabled",
                format!("plugin {name:?} appears more than once in plugins.enabled"),
            );
        }
        if !RE_PLUGIN_NAME.is_match(name) {
            report.error(
                "PLUGINS.ENABLED.FORMAT",
                "plugins.enabled",
                format!("enabled plugin name {name:?} must match ^[a-z][a-z0-9_-]*$"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_plugin(src: &str) -> PluginManifest {
        PluginManifest::parse(src).unwrap_or_else(|e| panic!("expected valid TOML: {e}"))
    }

    fn parse_platform(src: &str) -> PlatformManifest {
        PlatformManifest::parse(src).unwrap_or_else(|e| panic!("expected valid TOML: {e}"))
    }

    fn codes(report: &ValidationReport) -> Vec<&'static str> {
        report.issues.iter().map(|i| i.code).collect()
    }

    #[test]
    fn plugin_valid_minimal_has_no_issues() {
        let m = parse_plugin(
            r#"
            [plugin]
            name = "hello"
            display_name = "Hello"
            manifest_schema = 1
            "#,
        );
        let r = m.validate();
        assert!(r.is_ok(), "expected no issues, got {:?}", r.issues);
    }

    #[test]
    fn plugin_bad_name_flagged() {
        let m = parse_plugin(
            r#"
            [plugin]
            name = "Hello"
            display_name = "Hello"
            manifest_schema = 1
            "#,
        );
        assert_eq!(codes(&m.validate()), vec!["PLUGIN.NAME.INVALID"]);
    }

    #[test]
    fn plugin_wrong_mount_prefix_flagged() {
        let m = parse_plugin(
            r#"
            [plugin]
            name = "hello"
            display_name = "Hello"
            manifest_schema = 1

            [mount]
            route_prefix = "/wrong"
            "#,
        );
        assert_eq!(codes(&m.validate()), vec!["MOUNT.ROUTE.PREFIX"]);
    }

    #[test]
    fn plugin_unknown_schema_flagged() {
        let m = parse_plugin(
            r#"
            [plugin]
            name = "hello"
            display_name = "Hello"
            manifest_schema = 99
            "#,
        );
        assert_eq!(codes(&m.validate()), vec!["PLUGIN.SCHEMA.UNSUPPORTED"]);
    }

    #[test]
    fn plugin_bad_permission_key_flagged() {
        let m = parse_plugin(
            r#"
            [plugin]
            name = "hello"
            display_name = "Hello"
            manifest_schema = 1

            [permissions]
            "hello:Read" = "uppercase"
            "#,
        );
        assert_eq!(codes(&m.validate()), vec!["PERM.NAME.FORMAT"]);
    }

    #[test]
    fn plugin_bad_capability_flagged() {
        let m = parse_plugin(
            r#"
            [plugin]
            name = "hello"
            display_name = "Hello"
            manifest_schema = 1

            [requires]
            capabilities = ["db_read"]
            "#,
        );
        assert_eq!(codes(&m.validate()), vec!["CAP.NAME.FORMAT"]);
    }

    #[test]
    fn plugin_with_defaults_fills_mount() {
        let m = parse_plugin(
            r#"
            [plugin]
            name = "hello"
            display_name = "Hello"
            manifest_schema = 1
            "#,
        )
        .with_defaults();
        assert_eq!(m.mount.route_prefix.as_deref(), Some("/p/hello"));
        assert_eq!(m.mount.rpc_prefix.as_deref(), Some("/rpc/hello"));
        assert_eq!(m.mount.http_prefix.as_deref(), Some("/h/hello"));
    }

    #[test]
    fn platform_valid_path_form() {
        let m = parse_platform(
            r#"
            [source]
            path = "."

            [plugins]
            enabled = ["hello"]
            "#,
        );
        assert!(m.validate().is_ok());
    }

    #[test]
    fn platform_valid_git_form() {
        let m = parse_platform(
            r#"
            [source]
            repo = "git+https://example.invalid/junius.git"
            rev = "v0.1.0"

            [plugins]
            enabled = ["hello"]
            "#,
        );
        assert!(m.validate().is_ok());
    }

    #[test]
    fn platform_one_of_source_violated() {
        let m = parse_platform(
            r#"
            [source]
            repo = "git+https://example.invalid/junius.git"

            [plugins]
            enabled = ["hello"]
            "#,
        );
        let c = codes(&m.validate());
        assert!(c.contains(&"SOURCE.ONE_OF"), "got {c:?}");
    }

    #[test]
    fn platform_duplicate_enabled_flagged() {
        let m = parse_platform(
            r#"
            [source]
            path = "."

            [plugins]
            enabled = ["hello", "hello"]
            "#,
        );
        let c = codes(&m.validate());
        assert!(c.contains(&"PLUGINS.ENABLED.UNIQUE"), "got {c:?}");
    }
}
