//! Resolve a deployment's `[source]` to a concrete source-tree directory: a
//! local `path` (canonicalized relative to the deployment dir) or a git
//! `repo`+`rev` cloned/fetched into the source cache. Git is driven by shelling
//! out to the `git` CLI (no in-process git library).
//!
//! Wired into `build` + `plugin enable/disable` in later M11 stages.
#![allow(
    dead_code,
    reason = "consumed by build + plugin enable/disable in later M11 stages"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};
use junius_manifest::SourceConfig;
use sha2::{Digest, Sha256};

/// A resolved source tree.
pub struct ResolvedSource {
    pub root: PathBuf,
    /// The resolved commit SHA (git sources only).
    pub resolved_rev: Option<String>,
}

#[derive(Default)]
pub struct ResolveOpts {
    /// Skip `git fetch` when the cache already has the repo (use what's there).
    pub offline: bool,
}

/// Resolve `src` against `deployment_dir`, cloning git sources under
/// `sources_dir` (caller passes [`cache::sources_dir`](crate::cache::sources_dir);
/// tests pass a tempdir).
pub fn resolve(
    src: &SourceConfig,
    deployment_dir: &Path,
    sources_dir: &Path,
    opts: &ResolveOpts,
) -> anyhow::Result<ResolvedSource> {
    if let Some(path) = &src.path {
        let joined = deployment_dir.join(path);
        let root = joined
            .canonicalize()
            .with_context(|| format!("source path {} does not exist", joined.display()))?;
        return Ok(ResolvedSource {
            root,
            resolved_rev: None,
        });
    }

    let (Some(repo), Some(rev)) = (&src.repo, &src.rev) else {
        bail!("[source] must set either `path` or `repo` + `rev`");
    };
    let url = repo.strip_prefix("git+").unwrap_or(repo);
    let dir = sources_dir.join(cache_key(url, rev));
    std::fs::create_dir_all(sources_dir)
        .with_context(|| format!("creating source cache {}", sources_dir.display()))?;

    let dir_s = path_str(&dir)?;
    if dir.join(".git").is_dir() {
        if !opts.offline {
            git(&["-C", dir_s, "fetch", "--all", "--tags", "--prune"]).context("git fetch")?;
        }
    } else {
        git(&["clone", "--recurse-submodules", url, dir_s]).context("git clone")?;
    }
    git(&["-C", dir_s, "checkout", "--detach", rev])
        .with_context(|| format!("git checkout {rev}"))?;
    // Submodules are best-effort (a source without them just no-ops).
    let _ = git(&["-C", dir_s, "submodule", "update", "--init", "--recursive"]);
    let resolved_rev = git(&["-C", dir_s, "rev-parse", "HEAD"])?.trim().to_string();
    Ok(ResolvedSource {
        root: dir,
        resolved_rev: Some(resolved_rev),
    })
}

/// Stable cache directory name for a (url, rev) pair.
fn cache_key(url: &str, rev: &str) -> String {
    let mut h = Sha256::new();
    h.update(url.as_bytes());
    h.update([0]);
    h.update(rev.as_bytes());
    format!("{:x}", h.finalize())
}

fn path_str(p: &Path) -> anyhow::Result<&str> {
    p.to_str()
        .with_context(|| format!("path {} is not valid UTF-8", p.display()))
}

/// Run `git` with `args`, returning stdout; errors include stderr.
pub(crate) fn git(args: &[&str]) -> anyhow::Result<String> {
    let out = Command::new("git")
        .args(args)
        .output()
        .context("spawning git (is it installed?)")?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests panic on unexpected failures")]
mod tests {
    use super::*;

    fn git_in(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    #[test]
    fn path_source_canonicalizes_relative_to_deployment_dir() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src-root")).unwrap();
        std::fs::create_dir_all(tmp.path().join("deploy")).unwrap();
        let src = SourceConfig {
            repo: None,
            rev: None,
            path: Some("../src-root".to_string()),
        };
        let resolved = resolve(
            &src,
            &tmp.path().join("deploy"),
            &tmp.path().join("cache"),
            &ResolveOpts::default(),
        )
        .unwrap();
        assert_eq!(
            resolved.root,
            tmp.path().join("src-root").canonicalize().unwrap()
        );
        assert_eq!(resolved.resolved_rev, None);
    }

    #[test]
    fn git_source_clones_then_fetches_from_a_file_url() {
        // `git` here inherits the process CWD; hold the crate-wide CWD lock for
        // the whole test so a sibling test's `set_current_dir` (e.g.
        // rpc_scaffold) can't remove our CWD mid-clone. See crate::cwd_lock.
        let _cwd = crate::cwd_lock();
        // Network-free: a local origin repo over the git `file://` transport.
        let tmp = tempfile::tempdir().unwrap();
        let origin = tmp.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git_in(&origin, &["init", "-q", "-b", "main"]);
        std::fs::write(origin.join("README.md"), "junius source").unwrap();
        git_in(&origin, &["add", "."]);
        git_in(
            &origin,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "-m",
                "init",
            ],
        );
        let sha = String::from_utf8(
            Command::new("git")
                .args(["-C", origin.to_str().unwrap(), "rev-parse", "HEAD"])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string();

        let src = SourceConfig {
            repo: Some(format!("git+file://{}", origin.display())),
            rev: Some(sha.clone()),
            path: None,
        };
        let cache = tmp.path().join("cache");

        let r1 = resolve(&src, tmp.path(), &cache, &ResolveOpts::default()).unwrap();
        assert!(r1.root.join("README.md").exists());
        assert_eq!(r1.resolved_rev.as_deref(), Some(sha.as_str()));

        // Second resolve hits the fetch path (cache dir already exists).
        let r2 = resolve(&src, tmp.path(), &cache, &ResolveOpts::default()).unwrap();
        assert_eq!(r1.root, r2.root);
        assert_eq!(r2.resolved_rev, Some(sha));
    }
}
