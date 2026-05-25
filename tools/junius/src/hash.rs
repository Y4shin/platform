//! Deterministic blake3 content hashing of a source tree, for `platform.lock`.
//! A hash is order-independent (files are sorted by relative path) and sensitive
//! to renames + content; build/VCS dirs are ignored.
//!
//! Wired into `build` (lock generation/enforcement) in a later M11 stage.
#![allow(
    dead_code,
    reason = "consumed by build's lock generation/enforcement in a later M11 stage"
)]

use std::io;
use std::path::{Path, PathBuf};

/// Directory names excluded from hashing (build artifacts + VCS + JS deps).
pub const IGNORE_DIRS: &[&str] = &[".git", "target", "node_modules", "dist"];

/// `blake3:<hex>` of the whole tree under `root`.
pub fn hash_tree(root: &Path) -> io::Result<String> {
    hash_at(root, root)
}

/// `blake3:<hex>` of the subtree at `root/rel` (paths framed relative to `root`,
/// so a plugin's hash is stable regardless of where the source is checked out).
pub fn hash_subtree(root: &Path, rel: &Path) -> io::Result<String> {
    hash_at(root, &root.join(rel))
}

fn hash_at(base_for_rel: &Path, walk_root: &Path) -> io::Result<String> {
    let mut files = Vec::new();
    walk(walk_root, &mut files)?;
    files.sort();
    let mut hasher = blake3::Hasher::new();
    for path in &files {
        let rel = path.strip_prefix(base_for_rel).unwrap_or(path);
        let data = std::fs::read(path)?;
        // Framed record so renames + content both change the hash.
        hasher.update(rel.to_string_lossy().as_bytes());
        hasher.update(&[0]);
        hasher.update(&(data.len() as u64).to_le_bytes());
        hasher.update(&data);
    }
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let skip = entry
                .file_name()
                .to_str()
                .is_some_and(|n| IGNORE_DIRS.contains(&n));
            if !skip {
                walk(&path, out)?;
            }
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests panic on unexpected failures")]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, contents: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, contents).unwrap();
    }

    #[test]
    fn order_independent_and_content_sensitive() {
        let a = tempfile::tempdir().unwrap();
        write(a.path(), "src/lib.rs", "fn a() {}");
        write(a.path(), "plugins/hello/x.rs", "hello");
        let b = tempfile::tempdir().unwrap();
        // Same content, created in the other order.
        write(b.path(), "plugins/hello/x.rs", "hello");
        write(b.path(), "src/lib.rs", "fn a() {}");
        assert_eq!(hash_tree(a.path()).unwrap(), hash_tree(b.path()).unwrap());

        // A one-byte change flips the hash.
        write(b.path(), "src/lib.rs", "fn b() {}");
        assert_ne!(hash_tree(a.path()).unwrap(), hash_tree(b.path()).unwrap());
    }

    #[test]
    fn ignores_build_dirs_and_tracks_renames() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "src/lib.rs", "x");
        let before = hash_tree(d.path()).unwrap();
        // A file under target/ doesn't affect the hash.
        write(d.path(), "target/junk", "lots of bytes");
        assert_eq!(before, hash_tree(d.path()).unwrap());
        // A rename does.
        std::fs::rename(d.path().join("src/lib.rs"), d.path().join("src/main.rs")).unwrap();
        assert_ne!(before, hash_tree(d.path()).unwrap());
    }

    #[test]
    fn subtree_hash_scopes_to_a_plugin() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "plugins/hello/x.rs", "h");
        write(d.path(), "plugins/widgets/y.rs", "w");
        let hello = hash_subtree(d.path(), Path::new("plugins/hello")).unwrap();
        // Changing widgets doesn't change hello's subtree hash.
        write(d.path(), "plugins/widgets/y.rs", "w2");
        assert_eq!(
            hello,
            hash_subtree(d.path(), Path::new("plugins/hello")).unwrap()
        );
    }
}
