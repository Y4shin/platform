//! Proc-macros for Junius plugins.
//!
//! At M02 this crate exposes a single macro:
//!
//! - [`plugin_metadata!`] — invoked once per plugin crate; reads the crate's
//!   `plugin.toml` at compile time and emits `pub static METADATA`, the typed
//!   `Config`/`Secrets` accessors, and a `pub mod permissions` of marker types.
//! - [`permissions!`] — builds a type-level permission witness from a `&`-list of
//!   marker types (`permissions!(A & B)` → `And<A, And<B, ()>>`).
//!
//! The `Repository`/`PluginCtx` derives arrive later in M07.

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

/// Build a type-level permission witness from a `&`-separated list of permission
/// marker types: `permissions!(HelloRead & HelloWrite)` expands to
/// `junius_sdk::permissions::And<HelloRead, And<HelloWrite, ()>>`. The marker
/// types come from a plugin's `plugin_metadata!()`-generated `permissions`
/// module; an empty invocation yields `()` (the no-permission witness).
#[proc_macro]
pub fn permissions(input: TokenStream) -> TokenStream {
    expand::permissions_macro(input.into()).into()
}
