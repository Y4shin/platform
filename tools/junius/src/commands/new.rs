//! `junius new` — scaffolding. M03 implements `new plugin <name>`; the other
//! `new` subcommands stay stubbed until their owning milestones.

use std::path::PathBuf;

use super::not_implemented;
use crate::cli::{NewCmd, OutputFormat, SyncArgs};
use crate::exit;

pub fn run(cmd: &NewCmd, format: OutputFormat) -> i32 {
    match cmd {
        NewCmd::Plugin { name } => scaffold_plugin(name, format),
        NewCmd::Component { .. } => not_implemented("new component", "M07"),
        NewCmd::Rpc { .. } => not_implemented("new rpc", "M05"),
        NewCmd::Migration { plugin, name } => scaffold_migration(plugin, name),
        NewCmd::Permission { .. } => not_implemented("new permission", "M07"),
    }
}

fn scaffold_plugin(name: &str, format: OutputFormat) -> i32 {
    // Reject names that would fail manifest validation; cheaper to refuse
    // here than to scaffold an invalid plugin and then fail on the next sync.
    if !is_valid_plugin_name(name) {
        eprintln!("junius: invalid plugin name {name:?}; must match ^[a-z][a-z0-9_-]*$");
        return exit::VALIDATION;
    }

    let dir: PathBuf = ["plugins", name].iter().collect();
    if dir.exists() {
        eprintln!("junius: plugin directory already exists: {}", dir.display());
        return exit::PARSE_ERROR;
    }

    if let Err(code) = write_template(name, &dir) {
        return code;
    }

    eprintln!("junius: scaffolded plugin {name:?} at {}", dir.display());
    eprintln!("junius: remember to add {name:?} to your platform.toml [plugins].enabled");

    // Run sync against the default config (or the env-default config) so the
    // generated files include the new plugin if it's already enabled.
    let sync_args = SyncArgs {
        plugin: None,
        dry_run: false,
        config: None,
    };
    super::sync::run(&sync_args, format)
}

fn scaffold_migration(plugin: &str, name: &str) -> i32 {
    if !is_valid_plugin_name(plugin) {
        eprintln!("junius: invalid plugin name {plugin:?}");
        return exit::VALIDATION;
    }
    if !is_valid_migration_name(name) {
        eprintln!("junius: invalid migration name {name:?}; must match ^[a-z][a-z0-9_]*$");
        return exit::VALIDATION;
    }

    let plugin_dir: PathBuf = ["plugins", plugin].iter().collect();
    if !plugin_dir.exists() {
        eprintln!("junius: no such plugin directory: {}", plugin_dir.display());
        return exit::PARSE_ERROR;
    }

    let migrations_dir = plugin_dir.join("migrations");
    let existing = read_migration_names(&migrations_dir);
    let number = next_migration_number(existing.iter().map(String::as_str));
    let filename = format!("{number:04}_{name}.up.sql");
    let path = migrations_dir.join(&filename);
    if path.exists() {
        eprintln!("junius: migration already exists: {}", path.display());
        return exit::PARSE_ERROR;
    }

    let template = "-- @requires platform:0007_audit_event\n\
         -- Add an @requires line for each cross-plugin table this migration references,\n\
         -- in the form `-- @requires <plugin>:<migration_name>`.\n\
         \n\
         -- Migration body goes here.\n";

    if let Err(code) = create(&migrations_dir) {
        return code;
    }
    if let Err(code) = write(&path, template) {
        return code;
    }
    eprintln!("junius: created migration {}", path.display());
    exit::OK
}

fn is_valid_migration_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Read the `NNNN_name` stems of existing `*.up.sql` files in `dir`.
fn read_migration_names(dir: &std::path::Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Some(stem) = entry
                .file_name()
                .to_str()
                .and_then(|n| n.strip_suffix(".up.sql"))
            {
                names.push(stem.to_string());
            }
        }
    }
    names
}

/// Next zero-padded sequence number: max existing `NNNN_` prefix + 1 (1 if none).
fn next_migration_number<'a>(stems: impl Iterator<Item = &'a str>) -> u32 {
    let mut max = 0u32;
    for stem in stems {
        if let Some((num, _)) = stem.split_once('_') {
            if let Ok(n) = num.parse::<u32>() {
                max = max.max(n);
            }
        }
    }
    max + 1
}

fn is_valid_plugin_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

fn write_template(name: &str, dir: &std::path::Path) -> Result<(), i32> {
    let pascal = pascal_case(name);

    let manifest = format!(
        "[plugin]\n\
         name = \"{name}\"\n\
         display_name = \"{pascal}\"\n\
         description = \"TODO: describe this plugin.\"\n\
         manifest_schema = 1\n"
    );

    let cargo_toml = format!(
        "[package]\n\
         name         = \"{name}-plugin\"\n\
         version      = \"0.0.0\"\n\
         edition.workspace      = true\n\
         license.workspace      = true\n\
         rust-version.workspace = true\n\
         publish      = false\n\
         \n\
         [dependencies]\n\
         junius-sdk = {{ workspace = true }}\n\
         \n\
         axum        = {{ workspace = true }}\n\
         async-trait = {{ workspace = true }}\n\
         \n\
         [dev-dependencies]\n\
         tokio = {{ workspace = true }}\n\
         tower = {{ workspace = true }}\n\
         http  = {{ workspace = true }}\n\
         \n\
         [lints]\n\
         workspace = true\n"
    );

    let lib_rs = format!(
        "//! `{name}` plugin.\n\
         \n\
         use async_trait::async_trait;\n\
         use axum::{{Router, routing::get}};\n\
         use junius_sdk::{{Plugin, PluginMetadata}};\n\
         \n\
         junius_sdk::plugin_metadata!();\n\
         \n\
         pub struct {pascal}Plugin;\n\
         \n\
         impl {pascal}Plugin {{\n\
         \x20   #[must_use]\n\
         \x20   pub fn new() -> Self {{\n\
         \x20       Self\n\
         \x20   }}\n\
         }}\n\
         \n\
         impl Default for {pascal}Plugin {{\n\
         \x20   fn default() -> Self {{\n\
         \x20       Self::new()\n\
         \x20   }}\n\
         }}\n\
         \n\
         #[async_trait]\n\
         impl Plugin for {pascal}Plugin {{\n\
         \x20   fn metadata(&self) -> &'static PluginMetadata {{\n\
         \x20       &METADATA\n\
         \x20   }}\n\
         \n\
         \x20   // Plain-HTTP routes; the host nests them under `/h/{name}`. Handlers\n\
         \x20   // obtain host resources per request via the `PluginResources` extractor.\n\
         \x20   fn routes(&self) -> Router {{\n\
         \x20       Router::new().route(\"/ping\", get(|| async {{ \"pong\" }}))\n\
         \x20   }}\n\
         \n\
         \x20   // Optionally register Connect-RPC services:\n\
         \x20   // fn register_rpc(&self, router: connectrpc::Router) -> connectrpc::Router {{\n\
         \x20   //     std::sync::Arc::new(MyService).register(router)\n\
         \x20   // }}\n\
         }}\n"
    );

    create(dir)?;
    create(&dir.join("src"))?;

    write(&dir.join("plugin.toml"), &manifest)?;
    write(&dir.join("Cargo.toml"), &cargo_toml)?;
    write(&dir.join("src/lib.rs"), &lib_rs)?;

    Ok(())
}

fn create(dir: &std::path::Path) -> Result<(), i32> {
    std::fs::create_dir_all(dir).map_err(|e| {
        eprintln!("junius: cannot create {}: {e}", dir.display());
        exit::PARSE_ERROR
    })
}

fn write(path: &std::path::Path, content: &str) -> Result<(), i32> {
    std::fs::write(path, content).map_err(|e| {
        eprintln!("junius: cannot write {}: {e}", path.display());
        exit::PARSE_ERROR
    })
}

fn pascal_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper_next = true;
    for c in s.chars() {
        if c == '-' || c == '_' {
            upper_next = true;
        } else if upper_next {
            out.extend(c.to_uppercase());
            upper_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_validation() {
        assert!(is_valid_plugin_name("hello"));
        assert!(is_valid_plugin_name("hello-world"));
        assert!(is_valid_plugin_name("hello_world"));
        assert!(is_valid_plugin_name("h1"));
        assert!(!is_valid_plugin_name("Hello"));
        assert!(!is_valid_plugin_name("1hello"));
        assert!(!is_valid_plugin_name(""));
        assert!(!is_valid_plugin_name("hello world"));
    }

    #[test]
    fn pascal_case_basic() {
        assert_eq!(pascal_case("hello"), "Hello");
        assert_eq!(pascal_case("hello-world"), "HelloWorld");
        assert_eq!(pascal_case("user_admin"), "UserAdmin");
    }

    #[test]
    fn migration_name_validation() {
        assert!(is_valid_migration_name("create_speaker"));
        assert!(is_valid_migration_name("add_index2"));
        assert!(!is_valid_migration_name("Create"));
        assert!(!is_valid_migration_name("with-dash"));
        assert!(!is_valid_migration_name(""));
    }

    #[test]
    fn next_number_from_existing_stems() {
        assert_eq!(next_migration_number(std::iter::empty()), 1);
        let stems = ["0001_users", "0003_audit", "0002_sessions"];
        assert_eq!(next_migration_number(stems.into_iter()), 4);
        // Non-numeric prefixes are ignored.
        let mixed = ["bogus_name", "0005_x"];
        assert_eq!(next_migration_number(mixed.into_iter()), 6);
    }
}
