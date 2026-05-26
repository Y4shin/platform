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
//! `junius rpc scaffold`, and plugin `build.rs` (via `emit_rpc_requires` in
//! M15 Stage 2). See `docs/impl/17-M15-rpc-service-macro.md`, "the naming
//! contract".

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
}
