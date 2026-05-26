//! Build-time codegen for plugin i18n catalogs.
//!
//! Invoked from a plugin's `build.rs`:
//!
//! ```ignore
//! fn main() {
//!     junius_i18n_build::generate(junius_i18n_build::Options {
//!         domain: "events",
//!         i18n_dir: "i18n",
//!         out_file: "i18n_messages.rs",
//!         locales: junius_i18n_build::ALL_LOCALES,
//!     })
//!     .expect("i18n codegen");
//! }
//! ```
//!
//! The plugin's `lib.rs` then invokes `junius_sdk::i18n_catalog!();`, which
//! `include!`s the generated file. The plugin gets:
//!
//! - one `messages::<MsgidPascalCase>` struct per msgid (with one `&str` field
//!   per `{placeholder}` discovered in the source `en.po`),
//! - an `impl junius_sdk::Message` for each (resolving the per-locale `Template`
//!   walk + typed-field substitution),
//! - a per-locale `static CATALOG_<LOCALE>: &[Option<&Template>]` array
//!   (already in `Message::ID` order),
//! - a `pub fn register(b: &mut LocalizerBuilder)` that installs the catalog
//!   under the plugin's [`Domain`](junius_sdk::Domain) variant.
//!
//! The `en.po` is the **schema of record**: every msgid + its placeholder set
//! is discovered there. Other locales' msgstrs are validated to use only that
//! placeholder set — a translator-introduced extra placeholder is a build error.

mod codegen;
pub mod po;

use std::path::{Path, PathBuf};

pub use po::ParseError;

/// The locales this build of the platform ships catalogs for. Mirrors
/// `junius_sdk::Locale::ALL.map(|l| l.code())`. The codegen emits one
/// `static CATALOG_<LOC>` per entry in this slice; missing `.po` files are
/// tolerated and produce all-`None` rows (the localizer falls through to the
/// default locale).
pub const ALL_LOCALES: &[&str] = &["en", "de", "pseudo"];

/// Options for [`generate`].
pub struct Options<'a> {
    /// The plugin's domain name, matching a variant of `junius_sdk::Domain`
    /// (`"hello"`, `"events"`, …) or `"platform"` for the host catalog.
    pub domain: &'a str,
    /// Path (relative to `CARGO_MANIFEST_DIR`) of the `.po` catalog directory.
    /// The build helper looks for `<i18n_dir>/<locale>.po`.
    pub i18n_dir: &'a str,
    /// File name (relative to `OUT_DIR`) of the generated Rust file. The
    /// `i18n_catalog!()` macro `include!`s `OUT_DIR/<out_file>`.
    pub out_file: &'a str,
    /// Locales to emit `static CATALOG_<LOCALE>` arrays for. Defaults to
    /// [`ALL_LOCALES`]; tests may override.
    pub locales: &'a [&'a str],
}

impl<'a> Options<'a> {
    /// Convenience: the common case (locales = [`ALL_LOCALES`]).
    pub fn new(domain: &'a str) -> Self {
        Self {
            domain,
            i18n_dir: "i18n",
            out_file: "i18n_messages.rs",
            locales: ALL_LOCALES,
        }
    }
}

/// Parse the plugin's `.po` files and emit the generated module. Returns the
/// absolute path of the written file (handy for tests).
///
/// Emits `cargo:rerun-if-changed` for every `.po` file that exists. Plugins
/// that add a new locale by creating a new `.po` file still need a `cargo
/// clean` (because the missing-then-present file isn't tracked); this is the
/// same trade-off as `protoc-build` and acceptable for the closed-locale-set
/// model M14 ships with.
#[allow(
    clippy::needless_pass_by_value,
    reason = "build-script ergonomics: callers write `generate(Options::new(...))` and don't keep the Options around"
)]
pub fn generate(opts: Options<'_>) -> Result<PathBuf, Error> {
    #[allow(
        clippy::disallowed_methods,
        reason = "CARGO_MANIFEST_DIR / OUT_DIR are the Cargo-supplied build-script paths of the caller's crate, not deployment config"
    )]
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|_| Error::Env("CARGO_MANIFEST_DIR not set"))?;
    #[allow(
        clippy::disallowed_methods,
        reason = "CARGO_MANIFEST_DIR / OUT_DIR are the Cargo-supplied build-script paths of the caller's crate, not deployment config"
    )]
    let out_dir = std::env::var("OUT_DIR").map_err(|_| Error::Env("OUT_DIR not set"))?;
    let i18n_dir = Path::new(&manifest_dir).join(opts.i18n_dir);
    let out_path = Path::new(&out_dir).join(opts.out_file);
    // Build-script context: emit cargo:rerun-if-changed for every shipped
    // locale that exists, so cargo picks up `.po` edits.
    println!(
        "cargo:rerun-if-changed={}",
        i18n_dir.join("en.po").display()
    );
    for locale in opts.locales {
        let path = i18n_dir.join(format!("{locale}.po"));
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    generate_at(opts.domain, opts.locales, &i18n_dir, &out_path)
}

/// Like [`generate`], but with explicit paths instead of `CARGO_MANIFEST_DIR` /
/// `OUT_DIR`. Used directly by integration tests; production build scripts go
/// through [`generate`].
pub fn generate_at(
    domain: &str,
    locales: &[&str],
    i18n_dir: &Path,
    out_path: &Path,
) -> Result<PathBuf, Error> {
    // Source-of-truth catalog: en.po. Every msgid + its placeholder set is
    // discovered from this file; the codegen emits one struct per msgid.
    let en_path = i18n_dir.join("en.po");
    let en_src = match std::fs::read_to_string(&en_path) {
        Ok(s) => s,
        Err(e) => return Err(Error::ReadSource(en_path.clone(), e.to_string())),
    };
    let en_entries = po::parse(&en_src).map_err(|e| Error::Parse(en_path.clone(), e))?;

    // Build the per-msgid schema from en: msgid, placeholders (in first-seen
    // order), and a stable index assigned by sorted-msgid position.
    let mut schemas = codegen::build_schemas(&en_entries)?;
    schemas.sort_by(|a, b| a.key.cmp(&b.key));
    for (idx, schema) in schemas.iter_mut().enumerate() {
        schema.id = idx;
    }

    // For each shipped locale, parse the file if it exists and validate every
    // msgstr's placeholder set against the schema. Missing locale files yield
    // all-`None` rows; missing msgids in a present file are tolerated (the
    // localizer falls back). Extra placeholders are a build error.
    let mut locale_catalogs = Vec::with_capacity(locales.len());
    for &locale in locales {
        let path = i18n_dir.join(format!("{locale}.po"));
        if path.exists() {
            let src = std::fs::read_to_string(&path)
                .map_err(|e| Error::ReadSource(path.clone(), e.to_string()))?;
            let entries = po::parse(&src).map_err(|e| Error::Parse(path.clone(), e))?;
            let row = codegen::build_locale_row(&schemas, &entries, locale, &path)?;
            locale_catalogs.push((locale, row));
        } else {
            // No file → empty row.
            locale_catalogs.push((locale, vec![codegen::TemplateRow::Missing; schemas.len()]));
        }
    }

    let source = codegen::emit(domain, &schemas, &locale_catalogs);
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::Write(out_path.to_path_buf(), e.to_string()))?;
    }
    std::fs::write(out_path, source)
        .map_err(|e| Error::Write(out_path.to_path_buf(), e.to_string()))?;
    Ok(out_path.to_path_buf())
}

/// Errors surfaced from [`generate`].
#[derive(Debug)]
pub enum Error {
    /// Required environment variable not set (`CARGO_MANIFEST_DIR`, `OUT_DIR`).
    Env(&'static str),
    /// Failed to read a `.po` file. Includes the path and a description.
    ReadSource(PathBuf, String),
    /// `.po` parsing failed at the given location.
    Parse(PathBuf, ParseError),
    /// Translation validation failed (extra placeholder, unknown msgid, etc).
    Validate(PathBuf, String),
    /// Failed to write the generated source.
    Write(PathBuf, String),
    /// A msgid in `en.po` violates the codegen rules (collision after
    /// `PascalCase` folding, embedded ICU plural syntax, …).
    BadSchema(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Env(var) => write!(f, "i18n codegen: {var} not set in the build environment"),
            Self::ReadSource(p, e) => write!(f, "i18n codegen: read {}: {e}", p.display()),
            Self::Parse(p, e) => write!(f, "i18n codegen: parse {}: {e}", p.display()),
            Self::Validate(p, msg) => write!(f, "i18n codegen: validate {}: {msg}", p.display()),
            Self::Write(p, e) => write!(f, "i18n codegen: write {}: {e}", p.display()),
            Self::BadSchema(msg) => write!(f, "i18n codegen: {msg}"),
        }
    }
}

impl std::error::Error for Error {}
