//! Semantic validation passes. These run *after* TOML deserialisation; they
//! check rules that aren't expressible in serde-derived schemas alone.
//!
//! The static regexes below use `.expect("static regex")`; clippy's
//! `expect_used` lint is allowed because these strings are compile-time
//! constants validated by the manifest crate's own tests.
#![allow(clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};
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

    for bucket_name in m.storage.buckets.keys() {
        if !RE_SCHEMA_IDENT.is_match(bucket_name) {
            report.error(
                "STORAGE.BUCKET.NAME",
                format!("storage.buckets.{bucket_name}"),
                format!("bucket name {bucket_name:?} must match ^[a-z][a-z0-9_]*$"),
            );
        }
    }

    plugin_config_and_secrets(m, report);
}

/// Validate a plugin's `[config]` schema and `[secrets]` declarations: names
/// must be valid Rust identifiers (so codegen maps them to fields/methods) and
/// each config default must match its declared type.
fn plugin_config_and_secrets(m: &PluginManifest, report: &mut ValidationReport) {
    for (key, field) in &m.config {
        if !RE_SCHEMA_IDENT.is_match(key) {
            report.error(
                "CONFIG.KEY.FORMAT",
                format!("config.{key}"),
                format!("config key {key:?} must match ^[a-z][a-z0-9_]*$"),
            );
        }
        if let Some(default) = &field.default {
            if !default_matches_type(default, field.ty) {
                report.error(
                    "CONFIG.DEFAULT.TYPE",
                    format!("config.{key}.default"),
                    format!(
                        "default for {key:?} does not match declared type {:?}",
                        field.ty
                    ),
                );
            }
        }
    }
    for key in m.secrets.keys() {
        if !RE_SCHEMA_IDENT.is_match(key) {
            report.error(
                "SECRET.NAME.FORMAT",
                format!("secrets.{key}"),
                format!("secret name {key:?} must match ^[a-z][a-z0-9_]*$"),
            );
        }
    }
}

fn default_matches_type(value: &toml::Value, ty: crate::plugin::ConfigType) -> bool {
    use crate::plugin::ConfigType;
    match ty {
        ConfigType::String => value.is_str(),
        ConfigType::Integer => value.is_integer(),
        ConfigType::Boolean => value.is_bool(),
        ConfigType::Float => value.is_float(),
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

/// Cross-check a deployment's `[plugins.<name>]` config/secrets against each
/// enabled plugin's declared schema. `plugins` maps plugin name → its manifest;
/// plugins absent from the map are skipped (best-effort). Appends issues to
/// `report`. This is the deploy-time guarantee that a deployment provides every
/// required config key + declared secret and references nothing undeclared.
pub fn deployment(
    platform: &PlatformManifest,
    plugins: &BTreeMap<String, PluginManifest>,
    report: &mut ValidationReport,
) {
    for name in &platform.plugins.enabled {
        let Some(plugin) = plugins.get(name) else {
            continue;
        };
        let table = platform
            .plugins
            .overrides
            .get(name)
            .and_then(|v| v.as_table());
        let provided_config = sub_keys(table, "config");
        let provided_secrets = sub_keys(table, "secrets");

        for (key, field) in &plugin.config {
            if field.required && field.default.is_none() && !provided_config.contains(key.as_str())
            {
                report.error(
                    "DEPLOY.CONFIG.MISSING",
                    format!("plugins.{name}.config.{key}"),
                    format!(
                        "plugin {name:?} requires config key {key:?}, not set in the deployment"
                    ),
                );
            }
        }
        for key in plugin.secrets.keys() {
            if !provided_secrets.contains(key.as_str()) {
                report.error(
                    "DEPLOY.SECRET.MISSING",
                    format!("plugins.{name}.secrets.{key}"),
                    format!(
                        "plugin {name:?} declares secret {key:?}, not provided by the deployment"
                    ),
                );
            }
        }
        for key in &provided_config {
            if !plugin.config.contains_key(*key) {
                report.error(
                    "DEPLOY.CONFIG.UNDECLARED",
                    format!("plugins.{name}.config.{key}"),
                    format!("config key {key:?} is not declared by plugin {name:?}"),
                );
            }
        }
        for key in &provided_secrets {
            if !plugin.secrets.contains_key(*key) {
                report.error(
                    "DEPLOY.SECRET.UNDECLARED",
                    format!("plugins.{name}.secrets.{key}"),
                    format!("secret {key:?} is not declared by plugin {name:?}"),
                );
            }
        }
    }
}

fn sub_keys<'a>(table: Option<&'a toml::Table>, section: &str) -> BTreeSet<&'a str> {
    table
        .and_then(|t| t.get(section))
        .and_then(|v| v.as_table())
        .map(|t| t.keys().map(String::as_str).collect())
        .unwrap_or_default()
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
    fn plugin_valid_config_and_secrets_ok() {
        let m = parse_plugin(
            r#"
            [plugin]
            name = "hello"
            display_name = "Hello"
            manifest_schema = 1

            [config.greeting]
            type = "string"
            default = "Hi"

            [secrets.api_key]
            description = "x"
            "#,
        );
        let r = m.validate();
        assert!(r.is_ok(), "expected no issues, got {:?}", r.issues);
    }

    #[test]
    fn plugin_bad_config_key_and_default_type_flagged() {
        let m = parse_plugin(
            r#"
            [plugin]
            name = "hello"
            display_name = "Hello"
            manifest_schema = 1

            [config.BadKey]
            type = "string"

            [config.count]
            type = "integer"
            default = "not-an-int"
            "#,
        );
        let found = codes(&m.validate());
        assert!(found.contains(&"CONFIG.KEY.FORMAT"), "got {found:?}");
        assert!(found.contains(&"CONFIG.DEFAULT.TYPE"), "got {found:?}");
    }

    fn deployment_codes(platform_src: &str, plugins: &[(&str, &str)]) -> Vec<&'static str> {
        let platform = parse_platform(platform_src);
        let mut map = BTreeMap::new();
        for (name, src) in plugins {
            map.insert((*name).to_string(), parse_plugin(src));
        }
        let mut report = ValidationReport::default();
        super::deployment(&platform, &map, &mut report);
        codes(&report)
    }

    const HELLO_SCHEMA: &str = r#"
        [plugin]
        name = "hello"
        display_name = "Hello"
        manifest_schema = 1
        [config.greeting]
        type = "string"
        default = "Hi"
        [secrets.api_key]
        description = "x"
    "#;

    #[test]
    fn deployment_consistent_is_ok() {
        let codes = deployment_codes(
            "[source]\npath = \".\"\n[plugins]\nenabled = [\"hello\"]\n\
             [plugins.hello.secrets]\napi_key = \"dev\"\n",
            &[("hello", HELLO_SCHEMA)],
        );
        assert!(codes.is_empty(), "expected no issues, got {codes:?}");
    }

    #[test]
    fn deployment_missing_declared_secret_flagged() {
        let codes = deployment_codes(
            "[source]\npath = \".\"\n[plugins]\nenabled = [\"hello\"]\n",
            &[("hello", HELLO_SCHEMA)],
        );
        assert_eq!(codes, vec!["DEPLOY.SECRET.MISSING"]);
    }

    #[test]
    fn deployment_undeclared_secret_flagged() {
        let codes = deployment_codes(
            "[source]\npath = \".\"\n[plugins]\nenabled = [\"hello\"]\n\
             [plugins.hello.secrets]\napi_key = \"dev\"\nrogue = \"x\"\n",
            &[("hello", HELLO_SCHEMA)],
        );
        assert_eq!(codes, vec!["DEPLOY.SECRET.UNDECLARED"]);
    }

    #[test]
    fn plugin_bad_secret_name_flagged() {
        let m = parse_plugin(
            r#"
            [plugin]
            name = "hello"
            display_name = "Hello"
            manifest_schema = 1

            [secrets.BadName]
            description = "x"
            "#,
        );
        assert_eq!(codes(&m.validate()), vec!["SECRET.NAME.FORMAT"]);
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
