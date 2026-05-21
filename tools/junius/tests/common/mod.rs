//! Shared helpers for `junius` integration tests. Each integration test file
//! pulls these in via `mod common;`.

#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use assert_cmd::Command;

/// Build a `junius` binary invocation rooted at `repo`. Sets `NO_COLOR` so
/// snapshot output is deterministic.
pub fn cmd_in(repo: &Path) -> Command {
    let mut c = Command::cargo_bin("junius").expect("junius binary not built");
    c.current_dir(repo).env("NO_COLOR", "1");
    c
}

/// Self-cleaning temp directory.
pub struct TempDir {
    path: PathBuf,
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

impl TempDir {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Allocate a tempdir under `$TMPDIR/junius-test-<pid>-<counter>/`.
pub fn tempdir() -> TempDir {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let pid = std::process::id();
    let path = std::env::temp_dir().join(format!("junius-test-{pid}-{n}"));
    fs::create_dir_all(&path).unwrap();
    TempDir { path }
}

/// Set up a minimal pseudo-repo inside `tmp` with the same structure
/// `junius sync` expects: a workspace `Cargo.toml`, a `platform/Cargo.toml`
/// with managed-region markers, a deployment `platform.toml`, and one
/// `plugins/<name>/plugin.toml` per entry in `enabled`.
pub fn make_repo(tmp: &Path, enabled: &[&str]) {
    fs::create_dir_all(tmp.join("platform/src")).unwrap();
    fs::create_dir_all(tmp.join("plugins")).unwrap();

    fs::write(
        tmp.join("platform/Cargo.toml"),
        "[package]\nname = \"platform\"\n# >>> junius managed start\n# <<< junius managed end\n",
    )
    .unwrap();

    fs::write(
        tmp.join("Cargo.toml"),
        "[workspace]\nmembers = [\n    \"platform\",\n    # >>> junius managed start\n    # <<< junius managed end\n]\n",
    )
    .unwrap();

    let mut deployment = String::from("[source]\npath = \".\"\n\n[plugins]\nenabled = [");
    for (i, name) in enabled.iter().enumerate() {
        if i > 0 {
            deployment.push_str(", ");
        }
        deployment.push('"');
        deployment.push_str(name);
        deployment.push('"');
    }
    deployment.push_str("]\n");
    fs::write(tmp.join("platform.toml"), deployment).unwrap();

    for name in enabled {
        write_basic_plugin(tmp, name);
    }
}

/// Write a minimal `plugins/<name>/plugin.toml` to `repo`. The display name
/// is the input with first letter uppercased.
pub fn write_basic_plugin(repo: &Path, name: &str) {
    let dir = repo.join("plugins").join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("plugin.toml"),
        format!(
            "[plugin]\nname = \"{name}\"\ndisplay_name = \"{display}\"\nmanifest_schema = 1\n",
            display = capitalize(name)
        ),
    )
    .unwrap();
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(x) => x.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}
