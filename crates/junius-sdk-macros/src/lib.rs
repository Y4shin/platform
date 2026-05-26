//! Proc-macros for Junius plugins.
//!
//! - [`plugin_metadata!`] — invoked once per plugin crate; reads the crate's
//!   `plugin.toml` at compile time and emits `pub static METADATA`, the typed
//!   `Config`/`Secrets` accessors, and a `pub mod permissions` of marker types.
//! - [`permissions!`] — builds a type-level permission witness from a `&`-list of
//!   marker types (`permissions!(A & B)` → `And<A, And<B, ()>>`).
//! - [`i18n_catalog!`] — invoked once per plugin crate, expands to an `include!`
//!   of the per-plugin codegen file written by `junius-i18n-build` from `build.rs`.
//!
//! `Repository` / `PluginCtx` derives land in M07.

mod ctx;
mod expand;
mod repo;

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

/// Attribute on a marker struct (`#[repository] pub struct HelloRepo<P = ()>;`)
/// that rewrites it into a repository: injects the private db/user/audit fields
/// and generates `new`, the sealed `pool()` accessor, and a `Clone` impl. Method
/// impls go in `#[impl_repository]` blocks.
#[proc_macro_attribute]
pub fn repository(_attr: TokenStream, item: TokenStream) -> TokenStream {
    repo::repository(item.into()).into()
}

/// Attribute on a repository's method impl block
/// (`#[impl_repository(HelloRepo)] impl<P: Has<HelloRead>> HelloRepo<P> { … }`).
/// Injects a fresh index type-param for each `Has<Perm>` bound so the underlying
/// `Has<X, Idx>` membership trait stays coherent, and anchors the
/// `junius check` data-access scan.
#[proc_macro_attribute]
pub fn impl_repository(attr: TokenStream, item: TokenStream) -> TokenStream {
    repo::impl_repository(attr.into(), item.into()).into()
}

/// Derive the per-request Axum extractor for a plugin's state struct. On
/// `#[derive(PluginCtx)] struct HelloState<P = ()> { #[repo] greetings:
/// HelloRepo<P> }` it generates a `Clone` impl and `FromRequestParts` for
/// `PluginContext<HelloState<P>, P>` (resolve resources + caller from
/// extensions, check `P`, build the repos). Only `#[repo]` fields are supported.
#[proc_macro_derive(PluginCtx, attributes(repo))]
pub fn plugin_ctx(input: TokenStream) -> TokenStream {
    ctx::derive(input.into()).into()
}

/// Expands to `include!(concat!(env!("OUT_DIR"), "/i18n_messages.rs"))`. The
/// plugin's `build.rs` produces that file by calling
/// `junius_i18n_build::generate(...)`, which parses the plugin's `i18n/*.po`
/// catalogs and emits one `messages::<Msgid>` struct per msgid plus the
/// per-locale `catalog::CATALOG_<LOCALE>` arrays + a `register` function.
///
/// Example (inside a plugin's `lib.rs`):
/// ```ignore
/// junius_sdk::i18n_catalog!();
///
/// // somewhere later, in a job handler:
/// ctx.localizer
///    .for_locale(locale)
///    .t(crate::messages::EventSignupSubject { title: &event.title });
/// ```
#[proc_macro]
pub fn i18n_catalog(_input: TokenStream) -> TokenStream {
    quote::quote! {
        include!(concat!(env!("OUT_DIR"), "/i18n_messages.rs"));
    }
    .into()
}
