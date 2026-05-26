//! `junius rpc scaffold` — codemod that inserts a `todo!()` stub for every
//! proto method that's missing from its plugin's `#[rpc_service]` impl block
//! (M15 Stage 6).
//!
//! Scope: methods-into-existing-impl. Scaffolding a whole missing service impl
//! (struct + impl block) is a stretch goal noted in the design doc and
//! deferred. `junius check`'s `RPC.SERVICE.UNIMPLEMENTED` flags that case.
//!
//! Insertion strategy is **text-based**, not syn-reprint: locate the
//! `#[rpc_service(Svc)]` annotation, walk forward to the matching `}` of the
//! `impl` block via a brace counter, splice the stub in just before it. The
//! whole file is then piped through `rustfmt` so the inserted block lands
//! cleanly. This preserves comments, formatting, and import ordering — the
//! same approach the M13 friction triage chose for `junius sync`'s generated
//! Rust.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::cli::RpcScaffoldArgs;
use crate::exit;

/// Per-method stub the scaffold would (or did) write.
struct PlannedStub {
    file: PathBuf,
    /// Byte offset in the file's source where the new method body goes —
    /// immediately before the `}` that closes the impl block.
    insert_at: usize,
    /// Pre-rendered method text (with a leading newline).
    body: String,
    /// For reporting / --check output.
    service: String,
    method_snake: String,
}

#[allow(
    clippy::too_many_lines,
    reason = "single linear pipeline (scan → diff → apply) reads better as one function than artificially split"
)]
pub fn run(args: &RpcScaffoldArgs) -> i32 {
    let plugin_root = PathBuf::from("plugins").join(&args.plugin);
    if !plugin_root.is_dir() {
        eprintln!("junius: no plugin directory at {}", plugin_root.display());
        return exit::PARSE_ERROR;
    }

    // 1. Enumerate proto (service, [method]) pairs.
    let mut proto_files = Vec::new();
    junius_rpc_meta::collect_proto_files(&plugin_root.join("proto"), &mut proto_files);
    proto_files.sort();
    let mut proto_methods: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in &proto_files {
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        for (svc, methods) in junius_rpc_meta::scan_proto_service_methods(&content) {
            proto_methods.entry(svc).or_default().extend(methods);
        }
    }
    // De-dup methods within each service (defensive — proto syntax doesn't
    // allow duplicates within a service, but a malformed source might).
    for methods in proto_methods.values_mut() {
        methods.sort();
        methods.dedup();
    }

    // 2. Parse the plugin's rust source to find existing #[rpc_service] impls.
    let mut rs_files = Vec::new();
    collect_rs_files(&plugin_root.join("src"), &mut rs_files);
    rs_files.sort();
    let impls = scan_rpc_service_impls(&rs_files);

    // 3. Compute the diff: for each (service, method) without a handler in
    //    that service's impl block, build a PlannedStub.
    let mut planned: Vec<PlannedStub> = Vec::new();
    for (svc, methods) in &proto_methods {
        let Some(impl_site) = impls.iter().find(|i| &i.service_arg == svc) else {
            // No #[rpc_service(svc)] impl exists at all. RPC.SERVICE.UNIMPLEMENTED
            // catches this; the scaffold can't fix it (would need to write a
            // whole new impl block + struct).
            continue;
        };
        let present: std::collections::BTreeSet<String> = impl_site
            .existing_methods
            .iter()
            .map(|m| m.snake.clone())
            .collect();
        // Template the new method against any existing method in the impl
        // (for the CtxType and pb alias). If the impl is empty, we fall back
        // to () for the ctx type.
        let template = impl_site.existing_methods.first();

        for proto_method in methods {
            let snake = junius_rpc_meta::rust_method_ident(proto_method);
            if present.contains(&snake) {
                continue;
            }
            let body = render_stub(svc, proto_method, &snake, template);
            planned.push(PlannedStub {
                file: impl_site.file.clone(),
                insert_at: impl_site.closing_brace_at,
                body,
                service: svc.clone(),
                method_snake: snake,
            });
        }
    }

    // 4. Report / apply.
    if args.check {
        if planned.is_empty() {
            println!("junius rpc scaffold --check: all proto methods have handlers.");
            return exit::OK;
        }
        eprintln!(
            "junius rpc scaffold --check: {} stub(s) missing:",
            planned.len()
        );
        for stub in &planned {
            eprintln!(
                "  - {}::{}  ({})",
                stub.service,
                stub.method_snake,
                stub.file.display()
            );
        }
        return 1;
    }

    if planned.is_empty() {
        println!("junius rpc scaffold: nothing to do — every proto method already has a handler.");
        return exit::OK;
    }

    // Group by file (multiple stubs per impl block reuse the same insert
    // position, so we apply in reverse insertion-order per file to keep byte
    // offsets stable). All stubs targeting the same impl block share an
    // insert_at, so re-using that offset N times works: each splice grows the
    // file by the stub's length, and the *original* position still points
    // before the closing brace of the impl, just further from the (now
    // moved) end of file — we want all stubs to land at that position.
    let mut by_file: BTreeMap<PathBuf, Vec<&PlannedStub>> = BTreeMap::new();
    for stub in &planned {
        by_file.entry(stub.file.clone()).or_default().push(stub);
    }

    for (file, stubs) in by_file {
        let original = match std::fs::read_to_string(&file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("junius rpc scaffold: cannot read {}: {e}", file.display());
                return exit::PARSE_ERROR;
            }
        };
        // Splice each stub at its insertion point in descending offset order
        // so byte indices into the *original* string stay valid.
        let mut work = original.clone();
        let mut by_offset = stubs.clone();
        by_offset.sort_by_key(|s| std::cmp::Reverse(s.insert_at));
        for stub in by_offset {
            // The insert_at points at the byte of the `}`. Insert *before* it,
            // preserving the closing brace.
            work.insert_str(stub.insert_at, &stub.body);
        }
        let formatted = junius_rpc_meta::rustfmt_str(work);
        if let Err(e) = std::fs::write(&file, &formatted) {
            eprintln!("junius rpc scaffold: cannot write {}: {e}", file.display());
            return exit::PARSE_ERROR;
        }
        println!(
            "junius rpc scaffold: wrote {} stub(s) to {}",
            stubs.len(),
            file.display()
        );
    }

    exit::OK
}

/// Recursively collect every `.rs` file under `dir`.
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
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

struct ImplSite {
    file: PathBuf,
    service_arg: String,
    /// Byte offset of the `}` that closes the impl block. New methods are
    /// inserted at this position (the original `}` byte stays — `insert_str`
    /// shifts it right).
    closing_brace_at: usize,
    existing_methods: Vec<ExistingMethod>,
}

struct ExistingMethod {
    snake: String,
    /// The text of the ctx parameter's type (e.g. `EventCtx<crate::__rpc_requires::event_service::ListEvents>`).
    /// Used to template a new method's `CtxType` (and pb alias) when one is missing.
    ctx_type_text: String,
}

fn scan_rpc_service_impls(rs_files: &[PathBuf]) -> Vec<ImplSite> {
    let mut out = Vec::new();
    for path in rs_files {
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(file) = syn::parse_file(&content) else {
            continue;
        };
        visit_items_for_impls(&file.items, path, &content, &mut out);
    }
    out
}

fn visit_items_for_impls(items: &[syn::Item], path: &Path, content: &str, out: &mut Vec<ImplSite>) {
    for item in items {
        match item {
            syn::Item::Impl(item_impl) => {
                if let Some(site) = impl_to_site(item_impl, path, content) {
                    out.push(site);
                }
            }
            syn::Item::Mod(item_mod) => {
                if let Some((_, inner)) = &item_mod.content {
                    visit_items_for_impls(inner, path, content, out);
                }
            }
            _ => {}
        }
    }
}

fn impl_to_site(item_impl: &syn::ItemImpl, path: &Path, content: &str) -> Option<ImplSite> {
    if item_impl.trait_.is_some() {
        return None;
    }
    let service_arg = find_rpc_service_attr(&item_impl.attrs)?;

    // proc-macro2 on stable doesn't expose line/col for spans, so locate the
    // impl block's closing brace by scanning the source text: find the
    // `#[rpc_service(<service_arg>)]` annotation, advance to the next `impl`,
    // then walk braces to the depth-1 closing `}`.
    let closing_brace_at = locate_impl_closing_brace(content, &service_arg)?;

    let mut existing_methods = Vec::new();
    for impl_item in &item_impl.items {
        if let syn::ImplItem::Fn(method) = impl_item {
            let snake = method.sig.ident.to_string();
            let ctx_type_text = ctx_type_text(&method.sig).unwrap_or_default();
            existing_methods.push(ExistingMethod {
                snake,
                ctx_type_text,
            });
        }
    }

    Some(ImplSite {
        file: path.to_path_buf(),
        service_arg,
        closing_brace_at,
        existing_methods,
    })
}

/// Walk `src` to find the byte offset of the `}` that closes the
/// `#[rpc_service(service_arg)] impl … {` block. Naive: assumes the annotated
/// impl appears at most once per file (currently enforced by the macro — one
/// service trait per impl). Treats `{` / `}` inside a string literal or `//`
/// comment as opaque text.
fn locate_impl_closing_brace(src: &str, service_arg: &str) -> Option<usize> {
    let needle = format!("rpc_service({service_arg})");
    let mut search_from = 0;
    let attr_at = loop {
        let rest = &src[search_from..];
        let candidate = rest.find(&needle)? + search_from;
        // Make sure this looks like an attribute: scan backwards for `#[`.
        let prefix = &src[..candidate];
        if prefix.trim_end().ends_with("#[") || prefix.trim_end().ends_with("#[junius_sdk::") {
            break candidate;
        }
        // Otherwise it's some unrelated mention; keep searching.
        search_from = candidate + needle.len();
    };
    // Find the next `impl` keyword after the attribute.
    let after_attr = &src[attr_at..];
    let impl_rel = after_attr.find("impl")?;
    let impl_at = attr_at + impl_rel;
    // Walk forward to the opening brace.
    let open_rel = src[impl_at..].find('{')?;
    let open_at = impl_at + open_rel;
    // Track braces (ignoring those inside line comments and string literals).
    let bytes = src.as_bytes();
    let mut depth: i32 = 0;
    let mut i = open_at;
    let mut in_str = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_line_comment {
            if b == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }
        if in_block_comment {
            if b == b'*' && bytes.get(i + 1) == Some(&b'/') {
                in_block_comment = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if in_str {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match b {
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                in_line_comment = true;
                i += 2;
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                in_block_comment = true;
                i += 2;
            }
            b'"' => {
                in_str = true;
                i += 1;
            }
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

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

/// The verbatim text of the ctx parameter's type (e.g. `EventCtx<…>`).
fn ctx_type_text(sig: &syn::Signature) -> Option<String> {
    let arg = sig.inputs.iter().nth(1)?;
    let syn::FnArg::Typed(pat_type) = arg else {
        return None;
    };
    Some(quote::ToTokens::to_token_stream(&*pat_type.ty).to_string())
}

fn render_stub(
    proto_service: &str,
    proto_method: &str,
    method_snake: &str,
    template: Option<&ExistingMethod>,
) -> String {
    let service_module = junius_rpc_meta::service_module_name(proto_service);
    let alias = junius_rpc_meta::alias_type_name(method_snake);
    // Derive the CtxType from a sibling method; if the impl is empty, default
    // to a bare `()` witness — the author will replace it.
    let ctx_outer = template
        .and_then(|m| outer_ctor_name(&m.ctx_type_text))
        .unwrap_or_else(|| "Ctx".to_string());
    // Use the same `pb::` alias the events plugin uses; for other plugins the
    // author may need to adjust. Centralizing that detection is a follow-up
    // (the doc's "soft spot" caveat).
    let mut buf = String::new();
    let _ = writeln!(buf);
    let _ = writeln!(buf, "    async fn {method_snake}(");
    let _ = writeln!(buf, "        &self,");
    let _ = writeln!(
        buf,
        "        _ctx: {ctx_outer}<crate::__rpc_requires::{service_module}::{alias}>,"
    );
    let _ = writeln!(buf, "        _request: Owned{proto_method}RequestView,");
    let _ = writeln!(
        buf,
        "    ) -> ::connectrpc::ServiceResult<impl ::connectrpc::Encodable<pb::{proto_method}Response>> {{"
    );
    let _ = writeln!(buf, "        todo!({method_snake:?})");
    let _ = writeln!(buf, "    }}");
    buf
}

/// `EventCtx< … >` → `EventCtx`. Returns the outer ctor name from a type's
/// textual form (no parsing — we already have the tokens as text).
fn outer_ctor_name(ctx_type_text: &str) -> Option<String> {
    let trimmed = ctx_type_text.trim();
    let lt = trimmed.find('<')?;
    let head = trimmed[..lt].trim();
    if head.is_empty() {
        None
    } else {
        Some(head.into())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn tmp_plugin(label: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "junius-rpc-scaffold-{}-{label}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(p.join("plugins/x/src")).unwrap();
        std::fs::create_dir_all(p.join("plugins/x/proto/x/v1")).unwrap();
        p
    }

    #[test]
    fn render_stub_uses_naming_helpers_and_template_ctor() {
        let template = ExistingMethod {
            snake: "list_events".into(),
            ctx_type_text: "EventCtx < crate :: __rpc_requires :: event_service :: ListEvents >"
                .into(),
        };
        let stub = render_stub(
            "EventService",
            "DeleteEvent",
            "delete_event",
            Some(&template),
        );
        assert!(stub.contains("async fn delete_event("), "got: {stub}");
        assert!(
            stub.contains("EventCtx<crate::__rpc_requires::event_service::DeleteEvent>"),
            "got: {stub}"
        );
        assert!(stub.contains("OwnedDeleteEventRequestView"), "got: {stub}");
        assert!(stub.contains("pb::DeleteEventResponse"), "got: {stub}");
        assert!(stub.contains("todo!(\"delete_event\")"), "got: {stub}");
    }

    #[test]
    fn outer_ctor_name_strips_angle_brackets() {
        assert_eq!(
            outer_ctor_name("EventCtx<crate::__rpc_requires::event_service::ListEvents>")
                .as_deref(),
            Some("EventCtx")
        );
        assert_eq!(
            outer_ctor_name("EventCtx < crate :: foo >").as_deref(),
            Some("EventCtx")
        );
        assert_eq!(outer_ctor_name("()"), None);
    }

    fn write(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, body).unwrap();
    }

    // CWD is process-global; serialize tests that touch it so cargo's parallel
    // runner doesn't race them.
    fn cwd_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn run_in(root: &Path, plugin: &str, check: bool) -> i32 {
        let _guard = cwd_lock();
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(root).unwrap();
        let code = run(&RpcScaffoldArgs {
            plugin: plugin.into(),
            check,
        });
        std::env::set_current_dir(prev).unwrap();
        code
    }

    const PROTO_X: &str = "
        syntax = \"proto3\";
        package x.v1;
        service XService {
          rpc DoA(D) returns (D);
          rpc DoB(D) returns (D);
        }
    ";

    fn lib_with_one_method() -> &'static str {
        "
        struct XRpc;

        #[junius_sdk::rpc_service(XService)]
        impl XRpc {
            async fn do_a(
                &self,
                _ectx: XCtx<crate::__rpc_requires::x_service::DoA>,
                _request: OwnedDoARequestView,
            ) -> ::connectrpc::ServiceResult<impl ::connectrpc::Encodable<pb::DoAResponse>> {
                todo!(\"do_a\")
            }
        }
        "
    }

    #[test]
    fn check_mode_reports_missing_stubs_nonzero() {
        let dir = tmp_plugin("check-missing");
        write(&dir.join("plugins/x/proto/x/v1/svc.proto"), PROTO_X);
        write(&dir.join("plugins/x/src/lib.rs"), lib_with_one_method());

        let code = run_in(&dir, "x", true);
        assert_ne!(code, 0, "expected non-zero from --check with missing stubs");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scaffold_inserts_missing_method_and_is_idempotent() {
        let dir = tmp_plugin("insert");
        write(&dir.join("plugins/x/proto/x/v1/svc.proto"), PROTO_X);
        write(&dir.join("plugins/x/src/lib.rs"), lib_with_one_method());

        let code = run_in(&dir, "x", false);
        assert_eq!(code, 0, "scaffold should succeed");

        let after = std::fs::read_to_string(dir.join("plugins/x/src/lib.rs")).unwrap();
        assert!(after.contains("async fn do_b"), "got: {after}");
        assert!(after.contains("todo!(\"do_b\")"), "got: {after}");

        // Second run is a no-op: the now-present method matches the proto.
        let code2 = run_in(&dir, "x", false);
        assert_eq!(code2, 0);
        let after2 = std::fs::read_to_string(dir.join("plugins/x/src/lib.rs")).unwrap();
        assert_eq!(
            after, after2,
            "second run changed the file; scaffold is not idempotent"
        );

        // --check after the run is now clean.
        let check_code = run_in(&dir, "x", true);
        assert_eq!(check_code, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
