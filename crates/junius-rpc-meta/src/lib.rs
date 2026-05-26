//! Shared scanner + naming helpers for RPC metadata (M15).
//!
//! Single source for:
//! - `(platform.v1.requires)` proto annotations,
//! - permission-key → marker Pascal-casing,
//! - connectrpc-build's proto-method → snake fn ident convention,
//! - the `__rpc_requires::<service>::<alias>` path that links proto annotations
//!   to the witness alias the author writes in a handler.
//!
//! Consumed by `junius sync`, `junius check`, the `#[rpc_service]` proc-macro,
//! `junius rpc scaffold`, and plugin `build.rs` (via `emit_rpc_requires`). See
//! `docs/impl/17-M15-rpc-service-macro.md`, "the naming contract".

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

// -- proto scanning ---------------------------------------------------------

/// Recursively collect `*.proto` files under `dir`.
pub fn collect_proto_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_proto_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "proto") {
            out.push(path);
        }
    }
}

/// Extract `(service_fqn, method, perms)` for every method carrying
/// `option (platform.requires)` (also accepts `platform.v1.requires`). Associates
/// each option with the nearest preceding `rpc`, and each `rpc` with the
/// nearest preceding `service`, scoped by the file's `package` — robust for
/// conventionally-formatted protos.
#[allow(
    clippy::unwrap_used,
    reason = "compile-constant regexes are known-valid"
)]
pub fn scan_proto_requires(content: &str) -> Vec<(String, String, Vec<String>)> {
    let package = regex::Regex::new(r"(?m)^\s*package\s+([\w.]+)\s*;")
        .unwrap()
        .captures(content)
        .map(|c| c[1].to_string())
        .unwrap_or_default();
    let svc_re = regex::Regex::new(r"\bservice\s+(\w+)").unwrap();
    let rpc_re = regex::Regex::new(r"\brpc\s+(\w+)").unwrap();
    let req_re =
        regex::Regex::new(r#"option\s*\(\s*platform(?:\.v1)?\.requires\s*\)\s*=\s*"([^"]*)""#)
            .unwrap();

    let services: Vec<(usize, String)> = svc_re
        .captures_iter(content)
        .map(|c| (c.get(0).unwrap().start(), c[1].to_string()))
        .collect();
    let rpcs: Vec<(usize, String)> = rpc_re
        .captures_iter(content)
        .map(|c| (c.get(0).unwrap().start(), c[1].to_string()))
        .collect();

    let mut out = Vec::new();
    for cap in req_re.captures_iter(content) {
        let at = cap.get(0).unwrap().start();
        let Some((rpc_at, method)) = rpcs.iter().filter(|(o, _)| *o < at).next_back() else {
            continue;
        };
        let service = services
            .iter()
            .filter(|(o, _)| *o < *rpc_at)
            .next_back()
            .map(|(_, n)| n.clone())
            .unwrap_or_default();
        let fqn = if package.is_empty() {
            service
        } else {
            format!("{package}.{service}")
        };
        let perms: Vec<String> = cap[1]
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        out.push((fqn, method.clone(), perms));
    }
    out
}

/// Extract `(service_simple_name, [method_name])` for every service in a proto,
/// associating each `rpc` with the nearest preceding `service`. Method names
/// are the proto (`PascalCase`) spelling.
#[allow(
    clippy::unwrap_used,
    reason = "compile-constant regexes are known-valid"
)]
pub fn scan_proto_service_methods(content: &str) -> Vec<(String, Vec<String>)> {
    let svc_re = regex::Regex::new(r"\bservice\s+(\w+)").unwrap();
    let rpc_re = regex::Regex::new(r"\brpc\s+(\w+)").unwrap();

    let services: Vec<(usize, String)> = svc_re
        .captures_iter(content)
        .map(|c| (c.get(0).unwrap().start(), c[1].to_string()))
        .collect();
    let mut out: Vec<(String, Vec<String>)> = services
        .iter()
        .map(|(_, n)| (n.clone(), Vec::new()))
        .collect();

    for cap in rpc_re.captures_iter(content) {
        let at = cap.get(0).unwrap().start();
        let method = cap[1].to_string();
        if let Some(idx) = services
            .iter()
            .enumerate()
            .filter(|(_, (o, _))| *o < at)
            .map(|(i, _)| i)
            .next_back()
        {
            out[idx].1.push(method);
        }
    }
    out
}

// -- casing helpers ---------------------------------------------------------

/// Convert a permission key like `hello:read` or `speakers:book_slot` into a
/// Rust marker type name (`HelloRead`, `SpeakersBookSlot`) by upper-camel-casing
/// each `:`/`_` segment. Inputs are pre-validated by the manifest's
/// `PERM.NAME.FORMAT` rule. Single source so the `plugin_metadata!` codegen,
/// the generated `__rpc_requires` aliases, and `junius check` all agree on the
/// marker names.
pub fn permission_marker_pascal(key: &str) -> String {
    key.split([':', '_'])
        .filter(|s| !s.is_empty())
        .map(|seg| {
            let mut chars = seg.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + chars.as_str()
            })
        })
        .collect()
}

/// connectrpc-build's proto-method → rust-fn-ident conversion: `PascalCase` →
/// `snake_case` (e.g. `ListEvents` → `list_events`). Centralized here so
/// `junius check` and `junius rpc scaffold` agree with the trait method names
/// connectrpc-build emits, without depending on its source.
pub fn rust_method_ident(proto_method: &str) -> String {
    pascal_to_snake(proto_method)
}

/// The `__rpc_requires` module identifier for a proto service:
/// `EventService` → `event_service`. Snake-cases the proto service name.
pub fn service_module_name(proto_service: &str) -> String {
    pascal_to_snake(proto_service)
}

/// The `__rpc_requires::<service>::<alias>` alias-type name for a rust fn
/// ident: `list_events` → `ListEvents`. For conventionally-named RPCs this
/// round-trips the proto method name, so `junius check` can reconstruct the
/// expected alias path from a handler's method name without keeping a separate
/// proto-name → alias-name map.
pub fn alias_type_name(rust_fn_ident: &str) -> String {
    snake_to_pascal(rust_fn_ident)
}

fn pascal_to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn snake_to_pascal(s: &str) -> String {
    s.split('_')
        .filter(|seg| !seg.is_empty())
        .map(|seg| {
            let mut chars = seg.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + chars.as_str()
            })
        })
        .collect()
}

// -- build-time codegen -----------------------------------------------------

/// Emit a `__rpc_requires` Rust module to `<out_dir>/_rpc_requires.rs`. Plugin
/// `build.rs` files `include!` it from `lib.rs`; the resulting type aliases are
/// what authors write in handler ctx parameter types.
///
/// Shape:
///
/// ```ignore
/// pub mod __rpc_requires {
///     pub mod event_service {
///         pub type ListEvents =
///             ::junius_sdk::permissions::And<crate::permissions::EventsRead, ()>;
///         pub type Signup = ();  // unannotated method
///         // …
///     }
/// }
/// ```
///
/// Each alias is the right-nested `And` chain corresponding to the proto
/// method's `(platform.v1.requires)` annotation, or `()` if the method has
/// none. The alias is intended to be used **directly** as the `P` type
/// parameter on `PluginContext`/`<X>Ctx`; wrapping it in `permissions!(…)` would
/// produce `And<And<…>, ()>` and silently break `Has<X>` resolution (the chain
/// head must be a `Permission`, not an `And`).
///
/// Cross-plugin permission requirements are out of scope: an annotation segment
/// that doesn't exist in the calling crate's `permissions::*` becomes an
/// unresolved-path build error by construction.
pub fn emit_rpc_requires(proto_files: &[PathBuf]) -> std::io::Result<()> {
    #[allow(
        clippy::disallowed_methods,
        reason = "OUT_DIR is the Cargo-supplied build-script path of the caller's crate, not deployment config"
    )]
    let out_dir = std::env::var("OUT_DIR").map(PathBuf::from).map_err(|_| {
        std::io::Error::other("OUT_DIR not set; emit_rpc_requires must be called from a build.rs")
    })?;
    emit_rpc_requires_to(proto_files, &out_dir)
}

/// As [`emit_rpc_requires`], but with an explicit `out_dir`. Use this in tests
/// or to direct output to a non-`OUT_DIR` location; production `build.rs` files
/// should call [`emit_rpc_requires`] (which reads `OUT_DIR` itself).
pub fn emit_rpc_requires_to(proto_files: &[PathBuf], out_dir: &Path) -> std::io::Result<()> {
    // Collect every (service simple name) → (method → Option<perms>). `None`
    // means "method present in proto, no annotation"; `Some(vec![])` would
    // mean an empty annotation, which we treat the same.
    let mut by_service: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();

    for path in proto_files {
        let content = std::fs::read_to_string(path)?;
        // Seed every method with an empty perms list.
        for (svc, methods) in scan_proto_service_methods(&content) {
            let svc_entry = by_service.entry(svc).or_default();
            for m in methods {
                svc_entry.entry(m).or_default();
            }
        }
        // Override with declared perms where present.
        for (svc_fqn, method, perms) in scan_proto_requires(&content) {
            let svc = svc_fqn
                .rsplit('.')
                .next()
                .map(str::to_string)
                .unwrap_or(svc_fqn);
            if let Some(svc_entry) = by_service.get_mut(&svc) {
                svc_entry.insert(method, perms);
            }
        }
    }

    let mut buf = String::new();
    buf.push_str(
        "// Generated by junius_rpc_meta::emit_rpc_requires from proto annotations.\n\
         // DO NOT EDIT BY HAND. Re-runs of `cargo build` regenerate this file.\n\
         \n\
         /// Per-method permission-witness aliases derived from each proto method's\n\
         /// `(platform.v1.requires)` annotation. Author handler ctx parameters write\n\
         /// `<PluginCtx>::<crate::__rpc_requires::<service>::<Method>>` to pin the\n\
         /// witness to the proto-declared set; the `#[rpc_service]` macro then\n\
         /// rewrites the handler into the shape `connectrpc` expects.\n\
         #[allow(dead_code, non_snake_case)]\n\
         pub mod __rpc_requires {\n",
    );
    for (svc, methods) in &by_service {
        let module = service_module_name(svc);
        let _ = writeln!(buf, "    pub mod {module} {{");
        for (method, perms) in methods {
            let alias = alias_type_name(&rust_method_ident(method));
            let chain = render_witness_chain(perms);
            let _ = writeln!(buf, "        pub type {alias} = {chain};");
        }
        buf.push_str("    }\n");
    }
    buf.push_str("}\n");

    let formatted = rustfmt_str(buf);
    std::fs::write(out_dir.join("_rpc_requires.rs"), formatted)
}

/// Render `[a, b, c]` as `And<a, And<b, And<c, ()>>>`. Empty → `()`. The
/// markers resolve under the plugin's own `crate::permissions::*` — which is
/// what `plugin_metadata!()` emits.
fn render_witness_chain(perms: &[String]) -> String {
    if perms.is_empty() {
        return "()".to_string();
    }
    let mut chain = "()".to_string();
    for perm in perms.iter().rev() {
        let marker = permission_marker_pascal(perm);
        chain = format!("::junius_sdk::permissions::And<crate::permissions::{marker}, {chain}>");
    }
    chain
}

// -- rustfmt ----------------------------------------------------------------

/// Best-effort `rustfmt` of generated Rust *source text* (edition 2024) via
/// stdin→stdout, so codegen output is fmt-clean as written. Returns the input
/// unchanged if `rustfmt` is unavailable or errors. Used by `junius sync`'s
/// derived-file pipeline and by `junius rpc scaffold`'s codemod step.
pub fn rustfmt_str(src: String) -> String {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let Ok(mut child) = Command::new("rustfmt")
        .args(["--edition", "2024", "--emit", "stdout"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return src;
    };
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(src.as_bytes()).is_err() {
            return src;
        }
    }
    match child.wait_with_output() {
        Ok(out) if out.status.success() => String::from_utf8(out.stdout).unwrap_or(src),
        _ => src,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn scan_proto_requires_extracts_annotated_methods() {
        let proto = r#"
            syntax = "proto3";
            package hello.v1;
            import "platform/v1/annotations.proto";
            service HelloService {
              rpc Greet(GreetRequest) returns (GreetResponse);
              rpc ListGreetings(L) returns (R) {
                option (platform.v1.requires) = "hello:read";
              }
              rpc CreateGreeting(C) returns (D) {
                option (platform.v1.requires) = "hello:read,hello:write";
              }
            }
        "#;
        let mut found = scan_proto_requires(proto);
        found.sort();
        assert_eq!(
            found,
            vec![
                (
                    "hello.v1.HelloService".to_string(),
                    "CreateGreeting".to_string(),
                    vec!["hello:read".to_string(), "hello:write".to_string()],
                ),
                (
                    "hello.v1.HelloService".to_string(),
                    "ListGreetings".to_string(),
                    vec!["hello:read".to_string()],
                ),
            ]
        );
    }

    #[test]
    fn permission_marker_pascal_handles_colon_and_underscore() {
        assert_eq!(permission_marker_pascal("hello:read"), "HelloRead");
        assert_eq!(permission_marker_pascal("hello:write"), "HelloWrite");
        assert_eq!(
            permission_marker_pascal("speakers:book_slot"),
            "SpeakersBookSlot"
        );
        assert_eq!(permission_marker_pascal("events:share"), "EventsShare");
    }

    #[test]
    fn naming_round_trips_for_conventional_proto_methods() {
        // For every proto method connectrpc-build emits a snake fn ident; our
        // alias_type_name reverses that. The composition must round-trip the
        // original proto method name, since the `#[rpc_service]` macro and
        // `junius check` rely on it.
        for proto_method in [
            "ListEvents",
            "GetEvent",
            "CreateEvent",
            "UpdateEvent",
            "DeleteEvent",
            "ShareEvent",
            "GetEventInvite",
            "CreateGroupKey",
        ] {
            let snake = rust_method_ident(proto_method);
            let back = alias_type_name(&snake);
            assert_eq!(
                back, proto_method,
                "round-trip failed for {proto_method} (via {snake})"
            );
        }
    }

    #[test]
    fn service_module_name_snake_cases_proto_service() {
        assert_eq!(service_module_name("EventService"), "event_service");
        assert_eq!(service_module_name("InviteService"), "invite_service");
        assert_eq!(service_module_name("CalendarService"), "calendar_service");
    }

    #[test]
    fn scan_proto_service_methods_lists_all_rpcs_in_order() {
        let proto = r"
            service HelloService {
              rpc Greet(G) returns (G);
              rpc ListGreetings(L) returns (L);
            }
            service NoteService {
              rpc CreateNote(C) returns (C);
            }
        ";
        let scanned = scan_proto_service_methods(proto);
        assert_eq!(
            scanned,
            vec![
                (
                    "HelloService".into(),
                    vec!["Greet".into(), "ListGreetings".into()]
                ),
                ("NoteService".into(), vec!["CreateNote".into()]),
            ]
        );
    }

    #[test]
    fn render_witness_chain_shapes() {
        assert_eq!(render_witness_chain(&[]), "()");
        assert_eq!(
            render_witness_chain(&["events:read".to_string()]),
            "::junius_sdk::permissions::And<crate::permissions::EventsRead, ()>"
        );
        // Right-nested, in declaration order. Verifies the chain head stays
        // a `Permission` (not an `And`), which is what `Has<X, Here>` matches
        // on — the canary against the "double-wrap bug".
        assert_eq!(
            render_witness_chain(&["events:read".to_string(), "events:write".to_string(),]),
            "::junius_sdk::permissions::And<crate::permissions::EventsRead, \
             ::junius_sdk::permissions::And<crate::permissions::EventsWrite, ()>>"
        );
    }

    #[test]
    fn emit_rpc_requires_writes_aliases_for_every_method() {
        let tmp = std::env::temp_dir().join(format!("junius-rpc-meta-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let proto_path = tmp.join("svc.proto");
        std::fs::write(
            &proto_path,
            r#"
                syntax = "proto3";
                package hello.v1;
                service HelloService {
                  rpc Greet(G) returns (G) {
                    option (platform.v1.requires) = "hello:read";
                  }
                  rpc Ping(P) returns (P);  // no annotation → ()
                }
            "#,
        )
        .unwrap();

        emit_rpc_requires_to(&[proto_path], &tmp).unwrap();
        let emitted = std::fs::read_to_string(tmp.join("_rpc_requires.rs")).unwrap();

        assert!(emitted.contains("pub mod hello_service"), "got: {emitted}");
        // Annotated → typed chain referencing `crate::permissions::HelloRead`.
        assert!(
            emitted.contains("pub type Greet =")
                && emitted.contains("crate::permissions::HelloRead"),
            "got: {emitted}"
        );
        // Unannotated → unit type.
        assert!(emitted.contains("pub type Ping = ();"), "got: {emitted}");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
