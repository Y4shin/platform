//! Pure-function expansion of the `plugin_metadata!()` macro. Separated from
//! the proc-macro entry point so it can be exercised by unit tests inside this
//! crate without dragging in the proc-macro ABI.

use std::path::Path;

use junius_manifest::{ConfigType, PluginManifest, ValidationReport};
use junius_rpc_meta::permission_marker_pascal;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// Generate the `pub static METADATA` constant for a plugin whose manifest is
/// `content` (the literal text of `plugin.toml`). `path` is used only in error
/// diagnostics. On parse or validation failure, returns a `compile_error!()`
/// invocation.
pub fn plugin_metadata(content: &str, path: &Path) -> TokenStream {
    let manifest = match PluginManifest::parse(content) {
        Ok(m) => m.with_defaults(),
        Err(e) => {
            return compile_error(format!("junius: invalid TOML in {}: {e}", path.display()));
        }
    };

    let report = manifest.validate();
    if !report.is_ok() {
        return compile_error(format_validation_report(path, &report));
    }

    let metadata_tokens = expand_metadata(&manifest);
    let config_tokens = expand_config(&manifest);
    let secrets_tokens = expand_secrets(&manifest);
    let permission_markers = expand_permission_markers(&manifest);
    let bucket_enum = expand_bucket_enum(&manifest);
    let tracker = include_bytes_tracker();
    quote! {
        #tracker
        #metadata_tokens
        #config_tokens
        #secrets_tokens
        #permission_markers
        #bucket_enum
    }
}

/// Emit `pub mod buckets { pub enum Bucket { … } }` with one variant per declared
/// `[storage.buckets]` key, implementing [`BucketName`](::junius_sdk::BucketName).
/// Empty when the plugin declares no buckets (referencing `Bucket::X` is then a
/// compile error — the variant doesn't exist). This is the generated, manifest-
/// authoritative bucket surface: removing a bucket breaks compilation.
fn expand_bucket_enum(manifest: &PluginManifest) -> TokenStream {
    if manifest.storage.buckets.is_empty() {
        return quote! {};
    }
    let variants = manifest.storage.buckets.keys().map(|name| {
        let ty = format_ident!("{}", permission_marker_pascal(name));
        let doc = format!("Logical bucket `{name}`.");
        quote! { #[doc = #doc] #ty }
    });
    let arms = manifest.storage.buckets.keys().map(|name| {
        let ty = format_ident!("{}", permission_marker_pascal(name));
        let lit = name.as_str();
        quote! { Bucket::#ty => #lit }
    });
    quote! {
        /// Logical storage buckets generated from `[storage.buckets]` in `plugin.toml`.
        /// Pass a variant to `resources.storage.bucket(..)`.
        pub mod buckets {
            #[derive(Clone, Copy, Debug, PartialEq, Eq)]
            pub enum Bucket { #(#variants),* }
            impl ::junius_sdk::BucketName for Bucket {
                fn logical(&self) -> &'static str {
                    match self { #(#arms),* }
                }
            }
        }
    }
}

/// Expand `permissions!(A & B & C)` into a right-nested, `()`-terminated witness
/// `And<A, And<B, And<C, ()>>>`. Empty input → `()`. Marker paths resolve in the
/// caller's scope (typically `use crate::permissions::{…}`).
pub fn permissions_macro(input: proc_macro2::TokenStream) -> TokenStream {
    use syn::parse::Parser as _;
    let parser = syn::punctuated::Punctuated::<syn::Path, syn::Token![&]>::parse_terminated;
    let paths = match parser.parse2(input) {
        Ok(p) => p,
        Err(e) => return e.to_compile_error(),
    };
    let mut chain = quote! { () };
    for path in paths.iter().rev() {
        chain = quote! { ::junius_sdk::permissions::And<#path, #chain> };
    }
    chain
}

/// Emit `pub mod permissions { … }` with one zero-sized marker type per declared
/// `[permissions]` key, each implementing `Permission`. Empty when the plugin
/// declares none (referencing an undeclared permission is then a compile error —
/// the marker type doesn't exist).
fn expand_permission_markers(manifest: &PluginManifest) -> TokenStream {
    if manifest.permissions.is_empty() {
        return quote! {};
    }

    let markers = manifest.permissions.keys().map(|key| {
        let ty = format_ident!("{}", permission_marker_pascal(key));
        let name_lit = key.as_str();
        let doc = format!("Permission marker for `{key}`.");
        quote! {
            #[doc = #doc]
            pub struct #ty;
            impl ::junius_sdk::permissions::Permission for #ty {
                const NAME: &'static str = #name_lit;
            }
        }
    });

    quote! {
        /// Permission marker types generated from `[permissions]` in `plugin.toml`.
        /// Combine them in a witness via `junius_sdk::permissions!(A & B)`.
        pub mod permissions {
            #(#markers)*
        }
    }
}

/// Emit a typed `Config` struct + `Config::load` from `[config]`. Empty when the
/// plugin declares no config.
fn expand_config(manifest: &PluginManifest) -> TokenStream {
    if manifest.config.is_empty() {
        return quote! {};
    }

    let fields = manifest.config.iter().map(|(key, field)| {
        let ident = format_ident!("{key}");
        let ty = rust_type(field.ty);
        let optional = !field.required && field.default.is_none();
        let field_ty = if optional {
            quote! { ::core::option::Option<#ty> }
        } else {
            ty
        };
        quote! { pub #ident: #field_ty, }
    });

    let loads = manifest.config.iter().map(|(key, field)| {
        let ident = format_ident!("{key}");
        let ty = rust_type(field.ty);
        let key_lit = key.as_str();
        let expr = if let Some(default) = &field.default {
            let default_lit = default_literal(default, field.ty);
            quote! { cfg.get_opt::<#ty>(#key_lit)?.unwrap_or_else(|| #default_lit) }
        } else if field.required {
            quote! { cfg.get::<#ty>(#key_lit)? }
        } else {
            quote! { cfg.get_opt::<#ty>(#key_lit)? }
        };
        quote! { #ident: #expr, }
    });

    quote! {
        /// Typed view of this plugin's `[config]`, generated from `plugin.toml`.
        #[allow(dead_code)]
        pub struct Config { #(#fields)* }

        impl Config {
            /// Read + coerce this plugin's config, applying declared defaults.
            #[allow(dead_code)]
            pub fn load(
                cfg: &::junius_sdk::PluginConfig,
            ) -> ::core::result::Result<Self, ::junius_sdk::PluginError> {
                ::core::result::Result::Ok(Self { #(#loads)* })
            }
        }
    }
}

/// Emit a `Secrets` accessor with one method per declared secret. Empty when the
/// plugin declares no secrets. Referencing an undeclared secret is a compile
/// error (no method is generated for it).
fn expand_secrets(manifest: &PluginManifest) -> TokenStream {
    if manifest.secrets.is_empty() {
        return quote! {};
    }

    let methods = manifest.secrets.keys().map(|name| {
        let ident = format_ident!("{name}");
        let name_lit = name.as_str();
        quote! {
            /// The named secret, resolved by the host at boot.
            #[allow(dead_code, clippy::expect_used)]
            pub fn #ident(&self) -> &::junius_sdk::SecretString {
                self.0.get(#name_lit).expect(
                    ::core::concat!("secret '", #name_lit, "' is declared but missing at runtime"),
                )
            }
        }
    });

    quote! {
        /// Typed accessor for this plugin's declared secrets, generated from
        /// `plugin.toml`. Construct with `Secrets::new(resources.secrets())`.
        #[allow(dead_code)]
        pub struct Secrets<'a>(&'a ::junius_sdk::SecretStore);

        impl<'a> Secrets<'a> {
            #[allow(dead_code)]
            #[must_use]
            pub fn new(store: &'a ::junius_sdk::SecretStore) -> Self {
                Self(store)
            }
            #(#methods)*
        }
    }
}

fn rust_type(ty: ConfigType) -> TokenStream {
    match ty {
        ConfigType::String => quote! { ::std::string::String },
        ConfigType::Integer => quote! { i64 },
        ConfigType::Boolean => quote! { bool },
        ConfigType::Float => quote! { f64 },
    }
}

fn default_literal(value: &toml::Value, ty: ConfigType) -> TokenStream {
    match ty {
        ConfigType::String => {
            let s = value.as_str().unwrap_or_default();
            quote! { ::std::string::String::from(#s) }
        }
        ConfigType::Integer => {
            let n = value.as_integer().unwrap_or_default();
            quote! { #n }
        }
        ConfigType::Boolean => {
            let b = value.as_bool().unwrap_or_default();
            quote! { #b }
        }
        ConfigType::Float => {
            let f = value.as_float().unwrap_or_default();
            quote! { #f }
        }
    }
}

fn expand_metadata(manifest: &PluginManifest) -> TokenStream {
    let name = &manifest.plugin.name;
    let display_name = &manifest.plugin.display_name;
    let description = option_str(manifest.plugin.description.as_deref());
    let manifest_schema = manifest.plugin.manifest_schema;

    // `with_defaults()` guarantees the mount prefixes are populated.
    let route_prefix = manifest.mount.route_prefix.as_deref().unwrap_or_default();
    let rpc_prefix = manifest.mount.rpc_prefix.as_deref().unwrap_or_default();
    let http_prefix = manifest.mount.http_prefix.as_deref().unwrap_or_default();

    let dependencies = expand_dependencies(manifest);
    let exposed_components = expand_components(manifest);
    let exposed_tables = expand_tables(manifest);
    let permissions = expand_permissions(manifest);
    let capabilities = expand_capabilities(manifest);
    let buckets = expand_buckets(manifest);

    quote! {
        pub static METADATA: ::junius_sdk::PluginMetadata = ::junius_sdk::PluginMetadata {
            name: #name,
            display_name: #display_name,
            description: #description,
            manifest_schema: #manifest_schema,
            mount: ::junius_sdk::MountPoints {
                route_prefix: #route_prefix,
                rpc_prefix: #rpc_prefix,
                http_prefix: #http_prefix,
            },
            dependencies: #dependencies,
            exposed_components: #exposed_components,
            exposed_tables: #exposed_tables,
            permissions: #permissions,
            capabilities: #capabilities,
            buckets: #buckets,
        };
    }
}

/// Emit the `&[&str]` of declared logical bucket names for `PluginMetadata`.
fn expand_buckets(manifest: &PluginManifest) -> TokenStream {
    let names = manifest.storage.buckets.keys().map(String::as_str);
    quote! { &[ #(#names),* ] }
}

fn expand_dependencies(manifest: &PluginManifest) -> TokenStream {
    let entries = manifest.dependencies.iter().map(|(name, dep)| {
        let tables = string_slice(&dep.tables);
        let methods = string_slice(&dep.rpc_methods);
        let optional = dep.optional;
        quote! {
            ::junius_sdk::DependencyDecl {
                name: #name,
                optional: #optional,
                tables: #tables,
                rpc_methods: #methods,
            }
        }
    });
    quote! { &[ #(#entries),* ] }
}

fn expand_components(manifest: &PluginManifest) -> TokenStream {
    let entries = manifest.exposes.components.iter().map(|(name, c)| {
        let module = &c.module;
        let description = option_str(c.description.as_deref());
        quote! {
            ::junius_sdk::ExposedComponentDecl {
                name: #name,
                module: #module,
                description: #description,
            }
        }
    });
    quote! { &[ #(#entries),* ] }
}

fn expand_tables(manifest: &PluginManifest) -> TokenStream {
    let entries = manifest.exposes.tables.iter().map(|(name, t)| {
        let schema = &t.schema;
        let description = option_str(t.description.as_deref());
        quote! {
            ::junius_sdk::ExposedTableDecl {
                name: #name,
                schema: #schema,
                description: #description,
            }
        }
    });
    quote! { &[ #(#entries),* ] }
}

fn expand_permissions(manifest: &PluginManifest) -> TokenStream {
    let entries = manifest.permissions.iter().map(|(name, description)| {
        quote! {
            ::junius_sdk::PermissionDecl {
                name: #name,
                description: #description,
            }
        }
    });
    quote! { &[ #(#entries),* ] }
}

fn expand_capabilities(manifest: &PluginManifest) -> TokenStream {
    let caps = &manifest.requires.capabilities;
    string_slice(caps)
}

fn string_slice<S: AsRef<str>>(items: &[S]) -> TokenStream {
    let entries = items.iter().map(|s| {
        let lit = s.as_ref();
        quote! { #lit }
    });
    quote! { &[ #(#entries),* ] }
}

fn option_str(value: Option<&str>) -> TokenStream {
    if let Some(s) = value {
        quote! { ::core::option::Option::Some(#s) }
    } else {
        quote! { ::core::option::Option::None }
    }
}

fn include_bytes_tracker() -> TokenStream {
    // Force cargo to consider plugin.toml part of the build's dependency
    // graph. The const is named with a leading underscore so it's allowed to
    // be unused.
    quote! {
        const _PLUGIN_TOML_TRACKED: &[u8] = ::core::include_bytes!(
            ::core::concat!(::core::env!("CARGO_MANIFEST_DIR"), "/plugin.toml")
        );
    }
}

fn format_validation_report(path: &Path, report: &ValidationReport) -> String {
    use std::fmt::Write as _;

    let mut buf = format!(
        "junius: plugin.toml validation failed for {}\n",
        path.display()
    );
    for issue in &report.issues {
        let _ = writeln!(buf, "  [{}] {}: {}", issue.code, issue.path, issue.message);
    }
    buf
}

pub(crate) fn compile_error(message: impl AsRef<str>) -> TokenStream {
    let lit = syn::LitStr::new(message.as_ref(), proc_macro2::Span::call_site());
    quote! { ::core::compile_error!(#lit); }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::path::PathBuf;

    fn dummy_path() -> PathBuf {
        PathBuf::from("/tmp/plugin.toml")
    }

    fn render(content: &str) -> String {
        plugin_metadata(content, &dummy_path()).to_string()
    }

    fn parse_as_file(rendered: &str) -> syn::File {
        // Wrap in a tiny shim so include_bytes!() doesn't fail at parse time.
        // We don't actually expand the macros; we only check the surrounding
        // Rust is syntactically valid.
        syn::parse_str::<syn::File>(rendered).expect("emitted tokens should be valid Rust syntax")
    }

    #[test]
    fn minimal_manifest_emits_valid_syntax() {
        let rendered = render(
            r#"
            [plugin]
            name = "hello"
            display_name = "Hello"
            manifest_schema = 1
            "#,
        );
        let file = parse_as_file(&rendered);
        // The emitted file should contain a `pub static METADATA: ...` item.
        let has_metadata = file.items.iter().any(|item| {
            matches!(
                item,
                syn::Item::Static(s) if s.ident == "METADATA"
            )
        });
        assert!(
            has_metadata,
            "expected `pub static METADATA` in:\n{rendered}"
        );

        // It should also contain the include_bytes tracker.
        let has_tracker = file.items.iter().any(|item| {
            matches!(
                item,
                syn::Item::Const(c) if c.ident == "_PLUGIN_TOML_TRACKED"
            )
        });
        assert!(has_tracker, "expected include_bytes tracker constant");
    }

    #[test]
    fn full_manifest_includes_all_fields() {
        let rendered = render(
            r#"
            [plugin]
            name = "speakers"
            display_name = "Speakers"
            description = "Manage speakers."
            manifest_schema = 1

            [dependencies.identity]
            optional = false

            [dependencies.venues]
            optional = true

            [exposes.components.SpeakerCard]
            module = "./frontend/lib/SpeakerCard"

            [exposes.tables.speaker]
            schema = "speakers"

            [permissions]
            "speakers:read" = "View speakers."

            [requires]
            capabilities = ["db.read"]
            "#,
        );
        let _ = parse_as_file(&rendered);

        // Spot-check that critical literals made it into the token stream.
        for needle in [
            "\"speakers\"",
            "\"Speakers\"",
            "\"Manage speakers.\"",
            "\"identity\"",
            "\"venues\"",
            "\"SpeakerCard\"",
            "\"./frontend/lib/SpeakerCard\"",
            "\"speaker\"",
            "\"speakers:read\"",
            "\"View speakers.\"",
            "\"db.read\"",
        ] {
            assert!(
                rendered.contains(needle),
                "missing {needle} in:\n{rendered}"
            );
        }
    }

    #[test]
    fn config_and_secrets_emit_typed_codegen() {
        let rendered = render(
            r#"
            [plugin]
            name = "demo"
            display_name = "Demo"
            manifest_schema = 1

            [config.greeting]
            type = "string"
            default = "Hi"

            [config.max_items]
            type = "integer"
            required = true

            [secrets.api_key]
            description = "x"
            "#,
        );
        let _ = parse_as_file(&rendered);
        for needle in [
            "struct Config",
            "greeting",
            "max_items",
            "struct Secrets",
            "fn api_key",
            "SecretString",
            "PluginConfig",
        ] {
            assert!(
                rendered.contains(needle),
                "missing {needle} in:\n{rendered}"
            );
        }
    }

    #[test]
    fn no_config_or_secrets_emits_neither_type() {
        let rendered = render(
            r#"
            [plugin]
            name = "demo"
            display_name = "Demo"
            manifest_schema = 1
            "#,
        );
        assert!(
            !rendered.contains("struct Config"),
            "unexpected Config in:\n{rendered}"
        );
        assert!(
            !rendered.contains("struct Secrets"),
            "unexpected Secrets in:\n{rendered}"
        );
    }

    #[test]
    fn permissions_module_and_markers_emitted() {
        let rendered = render(
            r#"
            [plugin]
            name = "hello"
            display_name = "Hello"
            manifest_schema = 1

            [permissions]
            "hello:read" = "Read."
            "hello:write" = "Write."
            "#,
        );
        let _ = parse_as_file(&rendered);
        for needle in [
            "mod permissions",
            "struct HelloRead",
            "struct HelloWrite",
            "Permission for HelloRead",
            "\"hello:read\"",
            "\"hello:write\"",
        ] {
            assert!(
                rendered.contains(needle),
                "missing {needle} in:\n{rendered}"
            );
        }
    }

    #[test]
    fn no_permissions_emits_no_module() {
        let rendered = render(
            r#"
            [plugin]
            name = "hello"
            display_name = "Hello"
            manifest_schema = 1
            "#,
        );
        assert!(
            !rendered.contains("mod permissions"),
            "unexpected permissions module in:\n{rendered}"
        );
    }

    #[test]
    fn permissions_macro_builds_terminated_and_chain() {
        let single = permissions_macro(quote! { HelloRead }).to_string();
        assert!(single.contains("And < HelloRead"), "got: {single}");
        assert!(single.contains("()"), "got: {single}");

        let pair = permissions_macro(quote! { HelloRead & HelloWrite }).to_string();
        assert!(pair.contains("And < HelloRead"), "got: {pair}");
        assert!(pair.contains("And < HelloWrite"), "got: {pair}");

        // Empty witness is the unit type.
        assert_eq!(permissions_macro(quote! {}).to_string(), "()");
    }

    #[test]
    fn invalid_manifest_emits_compile_error() {
        let rendered = render(
            r#"
            [plugin]
            name = "Speakers"
            display_name = "Speakers"
            manifest_schema = 1
            "#,
        );
        assert!(
            rendered.contains("compile_error"),
            "expected compile_error in:\n{rendered}"
        );
        assert!(
            rendered.contains("PLUGIN.NAME.INVALID"),
            "expected the validation code in:\n{rendered}"
        );
    }

    #[test]
    fn malformed_toml_emits_compile_error() {
        let rendered = render("[plugin\nname = bad");
        assert!(
            rendered.contains("compile_error"),
            "expected compile_error in:\n{rendered}"
        );
    }
}
