//! End-to-end shape test for `junius_i18n_build::generate_at`: write fixture
//! `.po` files, run the codegen, and inspect the resulting Rust source for the
//! constructs the rest of the platform depends on.
//!
//! The compile-and-link-and-execute path for the generated module lives in
//! `crates/junius-sdk/tests/i18n_macro.rs` (a leaf crate that actually
//! `include!`s the produced file and asserts on the runtime behaviour).

#![allow(clippy::unwrap_used, reason = "tests panic on unexpected failures")]

use std::fs;
use std::path::Path;

fn write_fixture(dir: &Path, locale: &str, body: &str) {
    fs::write(dir.join(format!("{locale}.po")), body).unwrap();
}

#[test]
fn emits_typed_structs_per_msgid_and_per_locale_catalogs() {
    let tmp = tempdir();
    let i18n_dir = tmp.path().join("i18n");
    fs::create_dir_all(&i18n_dir).unwrap();

    write_fixture(
        &i18n_dir,
        "en",
        r#"
msgid "hello.world"
msgstr "Hello, world"

msgid "hello.greeted"
msgstr "Hi, {name}"
"#,
    );
    write_fixture(
        &i18n_dir,
        "de",
        r#"
msgid "hello.world"
msgstr "Hallo, Welt"
"#,
    );
    // No pseudo.po → all-None row expected.

    let out = tmp.path().join("i18n_messages.rs");
    junius_i18n_build::generate_at("hello", &["en", "de", "pseudo"], &i18n_dir, &out).unwrap();
    let src = fs::read_to_string(&out).unwrap();

    // Schema (one struct per msgid, fields per placeholder).
    assert!(src.contains("pub struct HelloWorld"), "src: {src}");
    assert!(
        src.contains("pub struct HelloGreeted<'a>"),
        "src missing typed struct with lifetime: {src}"
    );
    assert!(
        src.contains("pub name: &'a str,"),
        "src missing `name` field: {src}"
    );

    // Message trait constants — DOMAIN bound to the generated __DOMAIN const.
    assert!(src.contains("const DOMAIN: ::junius_sdk::Domain = super::__DOMAIN;"));
    // KEYs intact, IDs assigned by sorted-msgid order.
    assert!(src.contains("const KEY: &'static str = \"hello.greeted\";"));
    assert!(src.contains("const KEY: &'static str = \"hello.world\";"));

    // Per-locale catalog arrays.
    assert!(src.contains("pub static CATALOG_EN"));
    assert!(src.contains("pub static CATALOG_DE"));
    assert!(src.contains("pub static CATALOG_PSEUDO"));
    // de has one row (hello.world) and one None (hello.greeted, missing).
    let de_start = src.find("CATALOG_DE").unwrap();
    let de_end = src[de_start..].find("];").unwrap();
    let de_block = &src[de_start..de_start + de_end];
    let none_count = de_block.matches("None,").count();
    let some_count = de_block.matches("Some(").count();
    assert_eq!(none_count, 1, "de should have 1 None: {de_block}");
    assert_eq!(some_count, 1, "de should have 1 Some: {de_block}");

    // pseudo is entirely None.
    let pseudo_start = src.find("CATALOG_PSEUDO").unwrap();
    let pseudo_end = src[pseudo_start..].find("];").unwrap();
    let pseudo_block = &src[pseudo_start..pseudo_start + pseudo_end];
    assert_eq!(pseudo_block.matches("None,").count(), 2);
    assert_eq!(pseudo_block.matches("Some(").count(), 0);

    // register() forwards the host-supplied domain into add_domain.
    assert!(src.contains("pub fn register(b: &mut LocalizerBuilder)"));
    assert!(src.contains("b.add_domain(super::__DOMAIN, row);"));
}

#[test]
fn rejects_translator_introduced_placeholder() {
    let tmp = tempdir();
    let i18n_dir = tmp.path().join("i18n");
    fs::create_dir_all(&i18n_dir).unwrap();

    write_fixture(
        &i18n_dir,
        "en",
        r#"
msgid "hello.greeted"
msgstr "Hi, {name}"
"#,
    );
    write_fixture(
        &i18n_dir,
        "de",
        r#"
msgid "hello.greeted"
msgstr "Hallo, {titel}"
"#,
    );

    let out = tmp.path().join("i18n_messages.rs");
    let err = junius_i18n_build::generate_at("hello", &["en", "de"], &i18n_dir, &out).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("titel"), "msg: {msg}");
}

#[test]
fn rejects_icu_plural_in_template() {
    let tmp = tempdir();
    let i18n_dir = tmp.path().join("i18n");
    fs::create_dir_all(&i18n_dir).unwrap();
    write_fixture(
        &i18n_dir,
        "en",
        r#"
msgid "event.guests"
msgstr "{count, plural, one {one guest} other {many guests}}"
"#,
    );
    let out = tmp.path().join("i18n_messages.rs");
    let err = junius_i18n_build::generate_at("hello", &["en"], &i18n_dir, &out).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("ICU plural"));
}

// Minimal home-grown tempdir helper to keep this crate dep-free.
fn tempdir() -> TempDir {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "junius-i18n-build-test-{}-{}",
        std::process::id(),
        id
    ));
    std::fs::create_dir_all(&path).unwrap();
    TempDir(path)
}

struct TempDir(std::path::PathBuf);
impl TempDir {
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
