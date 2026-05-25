//! `junius build` — compose + build a deployment binary from a deployment dir.
//!
//! Run from a deployment directory (holding only `platform.toml` + secrets +
//! `platform.lock`). Steps:
//!   1. Resolve `[source]` to a source tree (local path or a cached git clone).
//!   2. Enforce/refresh `platform.lock` (blake3 of the resolved source).
//!   3. `junius sync` the composition glue **into** the resolved source.
//!   4. `pnpm --filter @junius/shell build` then
//!      `cargo build -p platform --release --features embed-frontend`, run in the
//!      source tree.
//!   5. Copy the `juniusd` binary to `<deployment>/platform`.
//!
//! `--check-config` stops after a structural validation (steps 1 + manifest
//! checks) — no compile, no database.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use junius_manifest::{
    DeploymentLock, LockSource, PlatformManifest, PluginManifest, ValidationReport,
};

use crate::cli::{BuildArgs, OutputFormat};
use crate::{cache, exit, hash, source};

const VITE_PNPM_FILTER: &str = "@junius/shell";
const ARTIFACT_NAME: &str = "platform";

pub fn run(args: &BuildArgs) -> i32 {
    let config_path = absolutize(
        args.config
            .clone()
            .unwrap_or_else(|| PathBuf::from("platform.toml")),
    );
    let deployment_dir = config_path.parent().unwrap_or(Path::new(".")).to_path_buf();

    let manifest = match load_manifest(&config_path) {
        Ok(m) => m,
        Err(code) => return code,
    };

    // Step 1: resolve the platform source.
    let resolved = match source::resolve(
        &manifest.source,
        &deployment_dir,
        &cache::sources_dir(),
        &source::ResolveOpts {
            offline: args.offline,
        },
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("junius: build: resolving [source]: {e:#}");
            return exit::VALIDATION;
        }
    };

    if args.check_config {
        return check_config(&manifest, &resolved.root, &config_path);
    }

    // Step 2: lock enforcement / refresh.
    if let Err(code) =
        enforce_or_write_lock(&deployment_dir, &manifest, &resolved, args.update_lock)
    {
        return code;
    }

    // Step 3: composition glue, generated into the resolved source.
    let sync_code = super::sync::run_in(&resolved.root, &config_path, false, OutputFormat::Plain);
    if sync_code != exit::OK {
        eprintln!("junius: build: sync failed; aborting");
        return sync_code;
    }

    // Step 4: frontend bundle + host binary, built in the source tree.
    if let Some(code) = run_step(
        &resolved.root,
        "pnpm",
        &["--filter", VITE_PNPM_FILTER, "build"],
    ) {
        return code;
    }
    if let Some(code) = run_step(
        &resolved.root,
        "cargo",
        &[
            "build",
            "-p",
            "platform",
            "--release",
            "--features",
            "embed-frontend",
        ],
    ) {
        return code;
    }

    // Step 5: copy the artifact into the deployment dir.
    let built = resolved.root.join("target/release/juniusd");
    let artifact = deployment_dir.join(ARTIFACT_NAME);
    if artifact.exists() && !args.force {
        eprintln!(
            "junius: build: {} already exists (use --force to overwrite)",
            artifact.display()
        );
        return exit::VALIDATION;
    }
    if let Err(e) = std::fs::copy(&built, &artifact) {
        eprintln!(
            "junius: build: copying {} -> {}: {e}",
            built.display(),
            artifact.display()
        );
        return exit::PARSE_ERROR;
    }
    println!("junius: build complete — {}", artifact.display());
    exit::OK
}

/// Resolve, then structurally validate the deployment against the resolved
/// source's plugin manifests. No compile, no DB.
fn check_config(manifest: &PlatformManifest, source_root: &Path, config_path: &Path) -> i32 {
    let mut report = manifest.validate();
    let plugins = match load_plugin_manifests(manifest, source_root) {
        Ok(p) => p,
        Err(code) => return code,
    };
    junius_manifest::validate::deployment(manifest, &plugins, &mut report);
    if report.is_ok() {
        println!("junius: check-config OK — {}", config_path.display());
        exit::OK
    } else {
        for issue in report.errors() {
            eprintln!(
                "junius: [error] {} at {}: {}",
                issue.code, issue.path, issue.message
            );
        }
        exit::VALIDATION
    }
}

/// Compare the resolved source against `<deployment>/platform.lock`, or write the
/// lock when absent / `--update-lock`.
fn enforce_or_write_lock(
    deployment_dir: &Path,
    manifest: &PlatformManifest,
    resolved: &source::ResolvedSource,
    update_lock: bool,
) -> Result<(), i32> {
    let lock_path = deployment_dir.join("platform.lock");
    let computed = compute_lock(manifest, resolved)?;

    if lock_path.exists() && !update_lock {
        let existing = std::fs::read_to_string(&lock_path)
            .ok()
            .and_then(|s| DeploymentLock::parse(&s).ok());
        if let Some(existing) = existing {
            if existing.source.url_or_path != computed.source.url_or_path
                || existing.source.resolved_rev != computed.source.resolved_rev
            {
                eprintln!(
                    "junius: build: the source moved — platform.lock pins {:?}{}, but the config \
                     resolves {:?}{}. Run with --update-lock if intentional.",
                    existing.source.url_or_path,
                    rev_suffix(existing.source.resolved_rev.as_deref()),
                    computed.source.url_or_path,
                    rev_suffix(computed.source.resolved_rev.as_deref()),
                );
                return Err(exit::VALIDATION);
            }
            if existing.source.content_hash != computed.source.content_hash {
                eprintln!(
                    "junius: build: platform.lock content_hash does not match the resolved source. \
                     Run with --update-lock if intentional."
                );
                return Err(exit::VALIDATION);
            }
            return Ok(());
        }
        eprintln!("junius: build: platform.lock is unreadable; rewriting");
    }

    if let Err(e) = std::fs::write(&lock_path, computed.to_toml_string()) {
        eprintln!("junius: build: writing {}: {e}", lock_path.display());
        return Err(exit::PARSE_ERROR);
    }
    Ok(())
}

fn compute_lock(
    manifest: &PlatformManifest,
    resolved: &source::ResolvedSource,
) -> Result<DeploymentLock, i32> {
    let content_hash = hash::hash_tree(&resolved.root).map_err(|e| {
        eprintln!("junius: build: hashing source: {e}");
        exit::PARSE_ERROR
    })?;
    let mut plugins = BTreeMap::new();
    for name in &manifest.plugins.enabled {
        let h = hash::hash_subtree(&resolved.root, &PathBuf::from("plugins").join(name)).map_err(
            |e| {
                eprintln!("junius: build: hashing plugin {name}: {e}");
                exit::PARSE_ERROR
            },
        )?;
        plugins.insert(name.clone(), h);
    }
    let url_or_path = manifest
        .source
        .repo
        .clone()
        .or_else(|| manifest.source.path.clone())
        .unwrap_or_default();
    Ok(DeploymentLock {
        source: LockSource {
            url_or_path,
            resolved_rev: resolved.resolved_rev.clone(),
            content_hash,
        },
        plugins,
    })
}

fn rev_suffix(rev: Option<&str>) -> String {
    rev.map_or_else(String::new, |r| format!(" @ {r}"))
}

fn load_manifest(config_path: &Path) -> Result<PlatformManifest, i32> {
    let src = std::fs::read_to_string(config_path).map_err(|e| {
        eprintln!("junius: cannot read {}: {e}", config_path.display());
        exit::PARSE_ERROR
    })?;
    let manifest = PlatformManifest::parse(&src).map_err(|e| {
        eprintln!("junius: parse error in {}: {e}", config_path.display());
        exit::PARSE_ERROR
    })?;
    let report = manifest.validate();
    if !report.is_ok() {
        for issue in report.errors() {
            eprintln!(
                "junius: [error] {} at {}: {}",
                issue.code, issue.path, issue.message
            );
        }
        return Err(exit::VALIDATION);
    }
    Ok(manifest)
}

fn load_plugin_manifests(
    manifest: &PlatformManifest,
    source_root: &Path,
) -> Result<BTreeMap<String, PluginManifest>, i32> {
    let mut out = BTreeMap::new();
    for name in &manifest.plugins.enabled {
        let path = source_root.join("plugins").join(name).join("plugin.toml");
        let src = std::fs::read_to_string(&path).map_err(|e| {
            eprintln!("junius: cannot read {}: {e}", path.display());
            exit::PARSE_ERROR
        })?;
        let plugin = PluginManifest::parse(&src).map_err(|e| {
            eprintln!("junius: parse error in {}: {e}", path.display());
            exit::PARSE_ERROR
        })?;
        let report: ValidationReport = plugin.validate();
        if !report.is_ok() {
            for issue in report.errors() {
                eprintln!(
                    "junius: [error] {} at {}: {}",
                    issue.code, issue.path, issue.message
                );
            }
            return Err(exit::VALIDATION);
        }
        out.insert(name.clone(), plugin);
    }
    Ok(out)
}

fn absolutize(p: PathBuf) -> PathBuf {
    if p.is_absolute() {
        p
    } else if let Ok(cwd) = std::env::current_dir() {
        cwd.join(p)
    } else {
        p
    }
}

fn run_step(cwd: &Path, program: &str, args: &[&str]) -> Option<i32> {
    println!("junius: $ {program} {}", args.join(" "));
    match Command::new(program).args(args).current_dir(cwd).status() {
        Ok(status) if status.success() => None,
        Ok(status) => {
            eprintln!("junius: build step `{program}` failed with {status}");
            Some(status.code().unwrap_or(1))
        }
        Err(e) => {
            eprintln!("junius: build step `{program}` failed to spawn: {e}");
            Some(exit::PARSE_ERROR)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests panic on unexpected failures")]
mod tests {
    use super::*;

    fn manifest() -> PlatformManifest {
        PlatformManifest::parse("[source]\npath = \"../src\"\n\n[plugins]\nenabled = [\"foo\"]\n")
            .unwrap()
    }

    fn make_source(root: &Path) {
        std::fs::create_dir_all(root.join("plugins/foo")).unwrap();
        std::fs::write(root.join("plugins/foo/x.rs"), "fn foo() {}").unwrap();
        std::fs::write(root.join("README.md"), "src").unwrap();
    }

    #[test]
    fn lock_is_written_enforced_then_updated() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        make_source(&src);
        let deploy = tmp.path().join("deploy");
        std::fs::create_dir_all(&deploy).unwrap();
        let manifest = manifest();
        let resolved = source::ResolvedSource {
            root: src.clone(),
            resolved_rev: None,
        };

        // First build writes the lock (with the source url + a per-plugin hash).
        assert!(enforce_or_write_lock(&deploy, &manifest, &resolved, false).is_ok());
        let lock_path = deploy.join("platform.lock");
        let lock = DeploymentLock::parse(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
        assert_eq!(lock.source.url_or_path, "../src");
        assert!(lock.plugins.contains_key("foo"));

        // Unchanged source re-checks clean.
        assert!(enforce_or_write_lock(&deploy, &manifest, &resolved, false).is_ok());

        // Tampering the source fails strict mode but passes with --update-lock.
        std::fs::write(src.join("plugins/foo/x.rs"), "fn foo() { /* changed */ }").unwrap();
        assert_eq!(
            enforce_or_write_lock(&deploy, &manifest, &resolved, false),
            Err(exit::VALIDATION)
        );
        assert!(enforce_or_write_lock(&deploy, &manifest, &resolved, true).is_ok());
        // The refreshed lock now matches again.
        assert!(enforce_or_write_lock(&deploy, &manifest, &resolved, false).is_ok());
    }
}
