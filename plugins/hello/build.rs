//! Compile-time codegen for the hello plugin's proto types.
//!
//! Outputs land in `OUT_DIR/hello.v1.rs` and are included via
//! `mod gen { include!(concat!(env!("OUT_DIR"), "/hello.v1.rs")); }`
//! in `src/lib.rs`. The `type_attribute` sprinkles serde derives so the
//! same types can be encoded both as protobuf (binary) and JSON
//! (Connect-RPC's two unary codecs).

fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed=proto/hello/v1/hello.proto");
    println!("cargo:rerun-if-changed=../../proto/platform/v1/annotations.proto");

    let mut config = prost_build::Config::new();
    config.type_attribute(".", "#[derive(::serde::Serialize, ::serde::Deserialize)]");
    config.compile_protos(&["proto/hello/v1/hello.proto"], &["proto", "../../proto"])?;

    Ok(())
}
