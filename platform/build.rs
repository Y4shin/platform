//! Compile-time codegen for the host's own Connect-RPC services (M18).
//!
//! The platform hosts `user.v1.UserService` directly — the first Connect
//! service that isn't a plugin's. `connectrpc-build` emits the buffa message
//! types + the Connect service trait; `junius-rpc-meta` emits the per-method
//! permission-witness aliases (M15). The repo-root `proto/` dir is the include
//! path (same tree the plugins compile against).

use std::path::PathBuf;

const PROTO_FILES: &[&str] = &["../proto/user/v1/user.proto"];

fn main() {
    for proto in PROTO_FILES {
        println!("cargo:rerun-if-changed={proto}");
    }

    if let Err(e) = connectrpc_build::Config::new()
        .files(PROTO_FILES)
        .includes(&["../proto"])
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
