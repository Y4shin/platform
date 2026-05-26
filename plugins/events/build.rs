//! Compile-time codegen for the events plugin's proto types + Connect services.
//!
//! `connectrpc-build` emits, into `OUT_DIR`, the buffa message types and the
//! Connect service traits for `events.v1.*`. The single `_connectrpc.rs` entry
//! file is `include!`d in `src/lib.rs`; no generated files land in the source
//! tree. (`../../proto` stays on the include path for `platform/v1/annotations.proto`.)

fn main() {
    println!("cargo:rerun-if-changed=proto/events/v1/events.proto");
    println!("cargo:rerun-if-changed=proto/events/v1/invite.proto");
    println!("cargo:rerun-if-changed=proto/events/v1/calendar.proto");
    println!("cargo:rerun-if-changed=../../proto/platform/v1/annotations.proto");

    if let Err(e) = connectrpc_build::Config::new()
        .files(&[
            "proto/events/v1/events.proto",
            "proto/events/v1/invite.proto",
            "proto/events/v1/calendar.proto",
        ])
        .includes(&["proto", "../../proto"])
        .include_file("_connectrpc.rs")
        .compile()
    {
        panic!("connectrpc-build codegen failed: {e}");
    }

    // M14: parse `i18n/*.po` into typed message structs + per-locale catalog
    // arrays. `junius_sdk::i18n_catalog!()` in lib.rs `include!`s the output.
    if let Err(e) = junius_i18n_build::generate(junius_i18n_build::Options::new("events")) {
        panic!("junius-i18n-build codegen failed: {e}");
    }
}
