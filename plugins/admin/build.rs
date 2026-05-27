//! Compile-time codegen for the admin plugin's proto types + Connect services.
//!
//! `connectrpc-build` emits the buffa message types and the Connect service
//! traits for `admin.v1.*`; `junius-rpc-meta` emits the per-method permission
//! witness aliases (M15). `../../proto` stays on the include path for
//! `platform/v1/annotations.proto`.

use std::path::PathBuf;

const PROTO_FILES: &[&str] = &["proto/admin/v1/admin.proto"];

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

    let proto_paths: Vec<PathBuf> = PROTO_FILES.iter().map(PathBuf::from).collect();
    if let Err(e) = junius_rpc_meta::emit_rpc_requires(&proto_paths) {
        panic!("junius-rpc-meta codegen failed: {e}");
    }
}
