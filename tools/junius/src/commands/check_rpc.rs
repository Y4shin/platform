//! M15 Stage 5 — `junius check` rules that gate the `#[rpc_service]` discipline.
//!
//! Three syntactic checks over the plugin's Rust sources, complementing the
//! existing `PROTO.REQUIRES.UNDECLARED` rule on proto annotations:
//!
//! - `RPC.HANDLER.UNGUARDED` — a trait impl `impl <X>Service for <Y>` in
//!   source means the author wrote the connectrpc shape directly, sidestepping
//!   the macro and the `(platform.v1.requires)` single-source contract.
//! - `RPC.SERVICE.UNIMPLEMENTED` — a service in the plugin's proto with no
//!   `#[rpc_service(<that service>)]` impl block anywhere in source.
//! - `RPC.WITNESS.MISMATCH` — a handler whose ctx-parameter alias path
//!   doesn't name its own method/service. Purely syntactic: the alias path
//!   includes the method name, so the rule reconstructs the expected path
//!   from `(macro_arg_service, fn_ident)` and compares.

use std::collections::BTreeSet;
use std::path::Path;

use junius_manifest::{Severity, ValidationIssue, ValidationReport};

/// Run the three Stage 5 RPC rules. `plugin_toml` is the manifest path; the
/// plugin's `src/` directory is its sibling. Best-effort: a missing `src/` or a
/// file syn can't parse is a no-op for that file.
pub fn check_rpc_handlers(plugin_toml: &Path, validation: &mut ValidationReport) {
    let plugin_root = plugin_toml.parent().unwrap_or_else(|| Path::new("."));
    let src_dir = plugin_root.join("src");
    let proto_dir = plugin_root.join("proto");

    // Collect every `.rs` file under src/, parsed.
    let mut rs_files = Vec::new();
    collect_rs_files(&src_dir, &mut rs_files);
    rs_files.sort();

    let mut state = RpcSites::default();
    for path in &rs_files {
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(file) = syn::parse_file(&content) else {
            continue;
        };
        let rel = path
            .strip_prefix(plugin_root)
            .unwrap_or(path.as_path())
            .display()
            .to_string();
        visit_items(&file.items, &rel, &mut state);
    }

    // Rule 1: RPC.HANDLER.UNGUARDED — bare trait impls.
    for site in &state.trait_impls {
        validation.issues.push(ValidationIssue {
            severity: Severity::Error,
            code: "RPC.HANDLER.UNGUARDED",
            path: format!("{}: impl {} for {}", site.file, site.trait_name, site.self_name),
            message: format!(
                "raw `impl {trait_name} for {self_name}` sidesteps the proto-derived witness. \
                 Replace with `#[junius_sdk::rpc_service({trait_name})] impl {self_name} {{ … }}` \
                 and write each method's ctx parameter as `<PluginCtx><crate::__rpc_requires::{service_module}::<Method>>`.",
                trait_name = site.trait_name,
                self_name = site.self_name,
                service_module = junius_rpc_meta::service_module_name(&site.trait_name),
            ),
        });
    }

    // Rule 2: RPC.SERVICE.UNIMPLEMENTED — proto services with no impl.
    let mut proto_services: BTreeSet<String> = BTreeSet::new();
    let mut proto_files = Vec::new();
    junius_rpc_meta::collect_proto_files(&proto_dir, &mut proto_files);
    for path in &proto_files {
        if let Ok(content) = std::fs::read_to_string(path) {
            for (svc, _methods) in junius_rpc_meta::scan_proto_service_methods(&content) {
                proto_services.insert(svc);
            }
        }
    }
    let impled: BTreeSet<&str> = state
        .macro_impls
        .iter()
        .map(|i| i.service_arg.as_str())
        .collect();
    for svc in &proto_services {
        if !impled.contains(svc.as_str()) {
            validation.issues.push(ValidationIssue {
                severity: Severity::Error,
                code: "RPC.SERVICE.UNIMPLEMENTED",
                path: svc.clone(),
                message: format!(
                    "proto declares `service {svc}` but the plugin has no `#[junius_sdk::rpc_service({svc})] impl …` block. \
                     Add one (or run `junius rpc scaffold` to stub the methods)."
                ),
            });
        }
    }

    // Rule 3: RPC.WITNESS.MISMATCH — alias path must name this method/service.
    for impl_site in &state.macro_impls {
        let expected_module = junius_rpc_meta::service_module_name(&impl_site.service_arg);
        for method in &impl_site.methods {
            let Some(alias_path) = &method.ctx_alias_path else {
                // We couldn't parse the alias path (e.g. ctx type isn't shaped
                // as `Outer<...::Inner>`). Skip — Rust's type checker covers
                // the bigger error; this rule is just the drift guard.
                continue;
            };
            // Expected: …::<expected_module>::<expected_alias>.
            let expected_alias = junius_rpc_meta::alias_type_name(
                &junius_rpc_meta::rust_method_ident(&method.fn_ident),
            );
            // Tail two segments must match; we don't pin the leading
            // `crate::__rpc_requires::` prefix so a future refactor of the
            // include path isn't a forced check change.
            let len = alias_path.len();
            let actual_module = alias_path.get(len.wrapping_sub(2)).cloned();
            let actual_alias = alias_path.last().cloned();
            if actual_module.as_deref() != Some(expected_module.as_str())
                || actual_alias.as_deref() != Some(expected_alias.as_str())
            {
                validation.issues.push(ValidationIssue {
                    severity: Severity::Error,
                    code: "RPC.WITNESS.MISMATCH",
                    path: format!(
                        "{}: {}::{}",
                        impl_site.file, impl_site.self_name, method.fn_ident
                    ),
                    message: format!(
                        "handler `{fn_ident}` (under `#[rpc_service({svc})]`) carries witness alias `{actual}` — expected `…::{expected_module}::{expected_alias}` (i.e. `crate::__rpc_requires::{expected_module}::{expected_alias}`).",
                        fn_ident = method.fn_ident,
                        svc = impl_site.service_arg,
                        actual = alias_path.join("::"),
                    ),
                });
            }
        }
    }
}

/// Recursively collect every `.rs` file under `dir`.
fn collect_rs_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[derive(Default)]
struct RpcSites {
    trait_impls: Vec<TraitImplSite>,
    macro_impls: Vec<MacroImplSite>,
}

struct TraitImplSite {
    file: String,
    trait_name: String,
    self_name: String,
}

struct MacroImplSite {
    file: String,
    /// The `Service` argument written in `#[rpc_service(Service)]`.
    service_arg: String,
    self_name: String,
    methods: Vec<HandlerMethodSite>,
}

struct HandlerMethodSite {
    fn_ident: String,
    /// The inner path inside the ctx-parameter type's first generic argument:
    /// `EventCtx<crate::__rpc_requires::event_service::ListEvents>` →
    /// `["crate", "__rpc_requires", "event_service", "ListEvents"]`. `None`
    /// if the shape isn't a path inside a generic.
    ctx_alias_path: Option<Vec<String>>,
}

/// Recurse into `mod { … }` blocks so plugins that split RPC impls into a
/// submodule (e.g. `mod rpc;`) are covered. Inline modules only — `mod foo;`
/// declarations are followed implicitly because `collect_rs_files` already
/// walks the whole src tree.
fn visit_items(items: &[syn::Item], file: &str, sites: &mut RpcSites) {
    for item in items {
        match item {
            syn::Item::Impl(item_impl) => visit_impl(item_impl, file, sites),
            syn::Item::Mod(item_mod) => {
                if let Some((_, inner)) = &item_mod.content {
                    visit_items(inner, file, sites);
                }
            }
            _ => {}
        }
    }
}

fn visit_impl(item_impl: &syn::ItemImpl, file: &str, sites: &mut RpcSites) {
    let self_name = type_last_segment_ident(&item_impl.self_ty).unwrap_or_else(|| "<?>".into());

    // Trait impl whose trait name ends in "Service": flag as unguarded UNLESS
    // it's a generated impl emitted by another macro (we can't see those — we
    // only parse source).
    if let Some((_bang, trait_path, _for_kw)) = &item_impl.trait_ {
        if let Some(last) = trait_path.segments.last() {
            let name = last.ident.to_string();
            if name.ends_with("Service") {
                sites.trait_impls.push(TraitImplSite {
                    file: file.into(),
                    trait_name: name,
                    self_name,
                });
            }
        }
        return;
    }

    // Inherent impl — look for #[rpc_service(SvcName)].
    let Some(service_arg) = find_rpc_service_attr(&item_impl.attrs) else {
        return;
    };

    let mut methods = Vec::new();
    for impl_item in &item_impl.items {
        if let syn::ImplItem::Fn(method) = impl_item {
            let fn_ident = method.sig.ident.to_string();
            let ctx_alias_path = method
                .sig
                .inputs
                .iter()
                .nth(1)
                .and_then(extract_ctx_alias_path);
            methods.push(HandlerMethodSite {
                fn_ident,
                ctx_alias_path,
            });
        }
    }
    sites.macro_impls.push(MacroImplSite {
        file: file.into(),
        service_arg,
        self_name,
        methods,
    });
}

/// Look for `#[rpc_service(<Path>)]` (with or without a `junius_sdk::` /
/// `junius_sdk_macros::` qualifier) and return the argument's last segment.
fn find_rpc_service_attr(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        let last = attr.path().segments.last()?;
        if last.ident != "rpc_service" {
            continue;
        }
        let path: syn::Path = attr.parse_args().ok()?;
        return path.segments.last().map(|s| s.ident.to_string());
    }
    None
}

/// Pull the alias path out of a ctx parameter type. Given
/// `<Outer><<Inner::Path::With::Segments>>`, return
/// `["Inner", "Path", "With", "Segments"]`. `None` for any other shape.
fn extract_ctx_alias_path(arg: &syn::FnArg) -> Option<Vec<String>> {
    let syn::FnArg::Typed(pat_type) = arg else {
        return None;
    };
    let syn::Type::Path(type_path) = &*pat_type.ty else {
        return None;
    };
    let last = type_path.path.segments.last()?;
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    let first_arg = args.args.first()?;
    let syn::GenericArgument::Type(syn::Type::Path(inner)) = first_arg else {
        return None;
    };
    let segments = inner
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>();
    if segments.is_empty() {
        None
    } else {
        Some(segments)
    }
}

/// `EventRpc` from `EventRpc`; `MyCrate::EventRpc` from `MyCrate::EventRpc`.
fn type_last_segment_ident(ty: &syn::Type) -> Option<String> {
    let syn::Type::Path(p) = ty else { return None };
    p.path.segments.last().map(|s| s.ident.to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::path::PathBuf;

    fn tmp_dir(label: &str) -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("junius-check-rpc-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(p.join("src")).unwrap();
        std::fs::create_dir_all(p.join("proto/x/v1")).unwrap();
        p
    }

    fn run(plugin_root: &Path) -> ValidationReport {
        let mut r = ValidationReport::default();
        check_rpc_handlers(&plugin_root.join("plugin.toml"), &mut r);
        r
    }

    fn write(path: &Path, body: &str) {
        std::fs::write(path, body).unwrap();
    }

    #[test]
    fn rpc_handler_unguarded_fires_on_bare_trait_impl() {
        let dir = tmp_dir("unguarded");
        write(
            &dir.join("src/lib.rs"),
            "
            struct ThingRpc;
            impl ThingService for ThingRpc {
                async fn do_a(&self, ctx: RequestContext, request: ()) {}
            }
            ",
        );
        let report = run(&dir);
        let codes: Vec<&str> = report.issues.iter().map(|i| i.code).collect();
        assert!(
            codes.contains(&"RPC.HANDLER.UNGUARDED"),
            "got codes: {codes:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rpc_service_unimplemented_fires_when_proto_lacks_a_handler() {
        let dir = tmp_dir("unimplemented");
        write(
            &dir.join("proto/x/v1/svc.proto"),
            "package x.v1;\nservice MissingService {\n  rpc DoThing(D) returns (D);\n}\n",
        );
        // Only ThingService is implemented; MissingService is the gap.
        write(
            &dir.join("src/lib.rs"),
            "
            struct ThingRpc;
            #[rpc_service(ThingService)]
            impl ThingRpc {
                async fn do_a(
                    &self,
                    _ectx: ThingCtx<crate::__rpc_requires::thing_service::DoA>,
                    _request: (),
                ) {}
            }
            ",
        );
        let report = run(&dir);
        let issue = report
            .issues
            .iter()
            .find(|i| i.code == "RPC.SERVICE.UNIMPLEMENTED");
        let issue = issue.expect("expected RPC.SERVICE.UNIMPLEMENTED");
        assert_eq!(issue.path, "MissingService");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rpc_witness_mismatch_fires_on_wrong_alias_path() {
        let dir = tmp_dir("mismatch");
        write(
            &dir.join("src/lib.rs"),
            "
            struct EventRpc;
            #[rpc_service(EventService)]
            impl EventRpc {
                async fn create_event(
                    &self,
                    _ectx: EventCtx<crate::__rpc_requires::event_service::ListEvents>,
                    _request: (),
                ) {}
            }
            ",
        );
        let report = run(&dir);
        let issue = report
            .issues
            .iter()
            .find(|i| i.code == "RPC.WITNESS.MISMATCH");
        let issue = issue.expect("expected RPC.WITNESS.MISMATCH");
        assert!(
            issue.message.contains("CreateEvent"),
            "expected CreateEvent in: {}",
            issue.message
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn correctly_wired_plugin_produces_no_rpc_issues() {
        let dir = tmp_dir("clean");
        write(
            &dir.join("proto/x/v1/svc.proto"),
            "package x.v1;\nservice EventService {\n  rpc CreateEvent(C) returns (C);\n}\n",
        );
        write(
            &dir.join("src/lib.rs"),
            "
            struct EventRpc;
            #[rpc_service(EventService)]
            impl EventRpc {
                async fn create_event(
                    &self,
                    _ectx: EventCtx<crate::__rpc_requires::event_service::CreateEvent>,
                    _request: (),
                ) {}
            }
            ",
        );
        let report = run(&dir);
        let rpc_issues: Vec<&str> = report
            .issues
            .iter()
            .map(|i| i.code)
            .filter(|c| c.starts_with("RPC."))
            .collect();
        assert!(rpc_issues.is_empty(), "got: {rpc_issues:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
