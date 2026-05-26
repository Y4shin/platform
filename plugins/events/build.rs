//! Compile-time codegen for the events plugin's proto types + Connect services.
//!
//! `connectrpc-build` emits, into `OUT_DIR`, the buffa message types and the
//! Connect service traits for `events.v1.*`. The single `_connectrpc.rs` entry
//! file is `include!`d in `src/lib.rs`; no generated files land in the source
//! tree. (`../../proto` stays on the include path for `platform/v1/annotations.proto`.)
//!
//! M15: also emit `_rpc_requires.rs` — a per-method type-alias module derived
//! from each method's `option (platform.v1.requires)`. Author handler ctx
//! parameters reference these aliases by path.

use std::path::PathBuf;

const PROTO_FILES: &[&str] = &[
    "proto/events/v1/events.proto",
    "proto/events/v1/invite.proto",
    "proto/events/v1/calendar.proto",
];

fn main() {
    for proto in PROTO_FILES {
        println!("cargo:rerun-if-changed={proto}");
    }
    println!("cargo:rerun-if-changed=../../proto/platform/v1/annotations.proto");

    if let Err(e) = connectrpc_build::Config::new()
        .files(PROTO_FILES)
        .includes(&["proto", "../../proto"])
        .include_file("_connectrpc.rs")
        .compile()
    {
        panic!("connectrpc-build codegen failed: {e}");
    }

    // M15: emit `__rpc_requires::<service>::<Method>` aliases from each proto
    // method's `(platform.v1.requires)` annotation. The alias is what the
    // handler's ctx parameter is typed on.
    let proto_paths: Vec<PathBuf> = PROTO_FILES.iter().map(PathBuf::from).collect();
    if let Err(e) = junius_rpc_meta::emit_rpc_requires(&proto_paths) {
        panic!("junius-rpc-meta codegen failed: {e}");
    }

    // M14: parse `i18n/*.po` into typed message structs + per-locale catalog
    // arrays. `junius_sdk::i18n_catalog!()` in lib.rs `include!`s the output.
    if let Err(e) = junius_i18n_build::generate(junius_i18n_build::Options::new("events")) {
        panic!("junius-i18n-build codegen failed: {e}");
    }
}
