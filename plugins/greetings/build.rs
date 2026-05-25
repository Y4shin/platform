//! Compile-time codegen for the greetings plugin's proto types + Connect service.
//! Mirrors hello's build.rs: `connectrpc-build` emits the buffa message types +
//! the Connect service trait into `OUT_DIR`, `include!`d from `src/lib.rs`. The
//! repo's root `../../proto` stays on the include path for
//! `platform/v1/annotations.proto`.

fn main() {
    println!("cargo:rerun-if-changed=proto/greetings/v1/greetings.proto");
    println!("cargo:rerun-if-changed=../../proto/platform/v1/annotations.proto");

    if let Err(e) = connectrpc_build::Config::new()
        .files(&["proto/greetings/v1/greetings.proto"])
        .includes(&["proto", "../../proto"])
        .include_file("_connectrpc.rs")
        .compile()
    {
        panic!("connectrpc-build codegen failed: {e}");
    }
}
