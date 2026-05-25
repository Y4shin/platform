//! Compile-time codegen for the events plugin's proto types + Connect services.
//!
//! `connectrpc-build` emits, into `OUT_DIR`, the buffa message types and the
//! Connect service traits for `events.v1.*`. The single `_connectrpc.rs` entry
//! file is `include!`d in `src/lib.rs`; no generated files land in the source
//! tree. (`../../proto` stays on the include path for `platform/v1/annotations.proto`.)

fn main() {
    println!("cargo:rerun-if-changed=proto/events/v1/events.proto");
    println!("cargo:rerun-if-changed=../../proto/platform/v1/annotations.proto");

    if let Err(e) = connectrpc_build::Config::new()
        .files(&["proto/events/v1/events.proto"])
        .includes(&["proto", "../../proto"])
        .include_file("_connectrpc.rs")
        .compile()
    {
        panic!("connectrpc-build codegen failed: {e}");
    }
}
