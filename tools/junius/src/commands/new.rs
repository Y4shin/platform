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
        NewCmd::Migration { .. } => not_implemented("new migration", "M06"),
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
         use junius_sdk::{{Plugin, PluginMetadata, PluginResources}};\n\
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
         \x20   fn routes(&self, _resources: PluginResources) -> Router {{\n\
         \x20       Router::new().route(\"/ping\", get(|| async {{ \"pong\" }}))\n\
         \x20   }}\n\
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
}
