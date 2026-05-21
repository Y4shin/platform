//! Proc-macros for Junius plugins.
//!
//! At M02 this crate exposes a single macro:
//!
//! - [`plugin_metadata!`] — invoked once per plugin crate; reads the crate's
//!   `plugin.toml` at compile time and emits a `pub static METADATA` of type
//!   [`junius_sdk::PluginMetadata`].
//!
//! Repository/permissions/PluginCtx derives arrive in M07.

mod expand;

use proc_macro::TokenStream;

/// Invoked once per plugin crate. Reads `plugin.toml` from the crate's
/// `CARGO_MANIFEST_DIR`, validates it, and emits a `pub static METADATA`
/// constant of type [`junius_sdk::PluginMetadata`]. Validation failures and
/// missing files are reported as compile errors.
///
/// Example: `junius_sdk::plugin_metadata!();`
#[proc_macro]
pub fn plugin_metadata(_input: TokenStream) -> TokenStream {
    // CARGO_MANIFEST_DIR is set by Cargo for every compilation unit it
    // invokes a proc-macro from. Bail with a compile error rather than
    // panicking in the rare case it isn't.
    #[allow(
        clippy::disallowed_methods,
        reason = "CARGO_MANIFEST_DIR is the Cargo-supplied path of the caller's crate at proc-macro expansion time, not deployment config"
    )]
    let Ok(dir) = std::env::var("CARGO_MANIFEST_DIR") else {
        return expand::compile_error(
            "junius: CARGO_MANIFEST_DIR is not set; cannot locate plugin.toml",
        )
        .into();
    };
    let path = std::path::Path::new(&dir).join("plugin.toml");

    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            return expand::compile_error(format!(
                "junius: failed to read {}: {e}",
                path.display()
            ))
            .into();
        }
    };

    expand::plugin_metadata(&content, &path).into()
}
