//! `junius i18n check` — validate every enabled plugin's `i18n/*.po` catalogs
//! end-to-end without going through `cargo build`.
//!
//! The build helper (`junius-i18n-build`) already validates a single plugin
//! during its `build.rs`; this command runs the same validation across every
//! plugin in the deployment + the host catalog, in one go, with <file:line>
//! spans for any failure. CI runs this as a fifth job alongside the M12
//! manifest/migration/SQL/Rust checks.

use std::path::{Path, PathBuf};

use junius_i18n_build::ALL_LOCALES;
use junius_manifest::PlatformManifest;

use crate::cli::{I18nCmd, OutputFormat};
use crate::exit;

pub fn run(cmd: &I18nCmd, _format: OutputFormat) -> i32 {
    match cmd {
        I18nCmd::Check { config } => check(config.as_deref()),
    }
}

fn check(config: Option<&Path>) -> i32 {
    let config_path =
        absolutize(config.map_or_else(|| PathBuf::from("platform.toml"), Path::to_path_buf));
    let src = match std::fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("junius: i18n: read {}: {e}", config_path.display());
            return exit::PARSE_ERROR;
        }
    };
    let manifest = match PlatformManifest::parse(&src) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("junius: i18n: parse {}: {e}", config_path.display());
            return exit::PARSE_ERROR;
        }
    };

    // i18n catalogs live in the local source tree under `plugins/<name>/i18n/`
    // and `platform/i18n/`, both relative to the current working directory.
    // This matches `junius migrate up`'s discovery (see `commands/migrate.rs`),
    // which also operates on the workspace rather than a [source]-resolved root.
    let source_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Discover catalog roots for the host + every enabled plugin. Roots that
    // don't exist are tolerated (a plugin without translatable strings yet
    // just doesn't ship `.po` files); roots that exist must validate.
    let mut roots: Vec<(String, PathBuf)> = Vec::new();
    let host_root = source_root.join("platform").join("i18n");
    if host_root.exists() {
        roots.push(("platform".into(), host_root));
    }
    for name in &manifest.plugins.enabled {
        let dir = source_root.join("plugins").join(name).join("i18n");
        if dir.exists() {
            roots.push((name.clone(), dir));
        }
    }

    if roots.is_empty() {
        println!(
            "junius: i18n: no catalogs found (no plugin under `plugins/*/i18n/` and no `platform/i18n/`)"
        );
        return exit::OK;
    }

    let mut failures = 0_u32;
    for (domain, dir) in &roots {
        match validate(domain, dir) {
            Ok(count) => {
                println!("junius: i18n: {domain}: ok ({count} messages)");
            }
            Err(msg) => {
                eprintln!("junius: i18n: {domain}: FAIL\n  {msg}");
                failures += 1;
            }
        }
    }
    if failures > 0 {
        eprintln!("junius: i18n: {failures} catalog(s) failed");
        exit::VALIDATION
    } else {
        println!("junius: i18n: all {} catalog(s) ok", roots.len());
        exit::OK
    }
}

/// Run the build helper's parse + schema + per-locale validation against an
/// `i18n/` directory; return the discovered message count on success.
fn validate(domain: &str, i18n_dir: &Path) -> Result<usize, String> {
    let out_dir =
        std::env::temp_dir().join(format!("junius-i18n-check-{}-{domain}", std::process::id()));
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("create temp dir: {e}"))?;
    let out_path = out_dir.join(format!("{domain}.rs"));

    let result = junius_i18n_build::generate_at(domain, ALL_LOCALES, i18n_dir, &out_path);
    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_dir(&out_dir);

    let written = result.map_err(|e| e.to_string())?;
    let _ = written; // result owns its path; we only needed the validation side-effect
    Ok(count_messages(i18n_dir))
}

/// Count msgids in `en.po` for the success-line summary.
fn count_messages(i18n_dir: &Path) -> usize {
    std::fs::read_to_string(i18n_dir.join("en.po"))
        .map(|s| {
            s.lines()
                .filter(|l| l.trim_start().starts_with("msgid "))
                .count()
        })
        .unwrap_or(0)
        // Don't count the gettext header entry (`msgid ""`).
        .saturating_sub(
            std::fs::read_to_string(i18n_dir.join("en.po"))
                .ok()
                .map_or(0, |s| usize::from(s.contains("msgid \"\""))),
        )
}

fn absolutize(p: PathBuf) -> PathBuf {
    if p.is_absolute() {
        p
    } else {
        std::env::current_dir().map(|cwd| cwd.join(&p)).unwrap_or(p)
    }
}
