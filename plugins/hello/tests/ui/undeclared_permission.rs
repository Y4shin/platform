//! Referencing a permission marker that `plugin.toml` never declared must not
//! compile: `plugin_metadata!()` only generates markers for declared keys, so
//! `HelloNope` simply doesn't exist.

type Witness = junius_sdk::permissions!(hello_plugin::permissions::HelloNope);

fn main() {
    let _: Option<Witness> = None;
}
