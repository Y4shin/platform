//! Discovery of migration files on disk: host migrations under
//! `platform/migrations/` plus each enabled plugin's `plugins/<name>/migrations/`.

use std::path::{Path, PathBuf};

/// The conventional plugin name recorded for host migrations.
pub const HOST_PLUGIN: &str = "platform";

const SUFFIX: &str = ".up.sql";

/// One migration file located on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationFile {
    /// Owning plugin, or [`HOST_PLUGIN`] for host migrations.
    pub plugin: String,
    /// Filename stem without the `.up.sql` suffix, e.g. `0001_users`.
    pub name: String,
    pub path: PathBuf,
}

/// Discover all migration files: host first, then each enabled plugin in order.
/// Within a directory, files are returned sorted by filename (so zero-padded
/// `NNNN_` prefixes give a deterministic apply order).
pub fn discover(
    repo_root: &Path,
    enabled_plugins: &[String],
) -> std::io::Result<Vec<MigrationFile>> {
    let mut files = Vec::new();
    collect_dir(
        &repo_root.join("platform/migrations"),
        HOST_PLUGIN,
        &mut files,
    )?;
    for name in enabled_plugins {
        let dir = repo_root.join("plugins").join(name).join("migrations");
        collect_dir(&dir, name, &mut files)?;
    }
    Ok(files)
}

fn collect_dir(dir: &Path, plugin: &str, out: &mut Vec<MigrationFile>) -> std::io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(SUFFIX))
        {
            paths.push(path);
        }
    }
    paths.sort();
    for path in paths {
        // Safe: filtered on the suffix above.
        let stem = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(SUFFIX))
            .unwrap_or_default()
            .to_string();
        out.push(MigrationFile {
            plugin: plugin.to_string(),
            name: stem,
            path,
        });
    }
    Ok(())
}
