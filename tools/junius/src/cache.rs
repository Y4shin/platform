//! The junius source cache (`~/.cache/junius`). Git sources are cloned under
//! `sources/`; `junius cache prune` evicts them. The root honors
//! `$XDG_CACHE_HOME`, then `$HOME`.
//!
//! Wired into `build`/`plugin`/`cache` commands in later M11 stages.
#![allow(
    dead_code,
    reason = "consumed by build + plugin enable/disable + cache prune in later M11 stages"
)]

use std::path::PathBuf;

/// Resolve the cache root from an env lookup (pure; the testable seam).
pub fn cache_root_from(env: &impl Fn(&str) -> Option<String>) -> PathBuf {
    if let Some(xdg) = env("XDG_CACHE_HOME").filter(|s| !s.is_empty()) {
        return PathBuf::from(xdg).join("junius");
    }
    if let Some(home) = env("HOME").filter(|s| !s.is_empty()) {
        return PathBuf::from(home).join(".cache").join("junius");
    }
    // Last resort (no HOME): a cwd-relative cache so we never write to `/`.
    PathBuf::from(".junius-cache")
}

/// The junius cache root, reading the real environment.
#[allow(
    clippy::disallowed_methods,
    reason = "resolving the junius source-cache root from XDG_CACHE_HOME/HOME"
)]
#[must_use]
pub fn cache_root() -> PathBuf {
    cache_root_from(&|k| std::env::var(k).ok())
}

/// Where git sources are cloned (`<root>/sources`).
#[must_use]
pub fn sources_dir() -> PathBuf {
    cache_root().join("sources")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_xdg_then_home() {
        let xdg = |k: &str| match k {
            "XDG_CACHE_HOME" => Some("/x/cache".to_string()),
            "HOME" => Some("/home/u".to_string()),
            _ => None,
        };
        assert_eq!(cache_root_from(&xdg), PathBuf::from("/x/cache/junius"));

        let home_only = |k: &str| (k == "HOME").then(|| "/home/u".to_string());
        assert_eq!(
            cache_root_from(&home_only),
            PathBuf::from("/home/u/.cache/junius")
        );

        let none = |_: &str| None;
        assert_eq!(cache_root_from(&none), PathBuf::from(".junius-cache"));
    }

    #[test]
    fn empty_env_values_fall_through() {
        let empty_xdg = |k: &str| match k {
            "XDG_CACHE_HOME" => Some(String::new()),
            "HOME" => Some("/home/u".to_string()),
            _ => None,
        };
        assert_eq!(
            cache_root_from(&empty_xdg),
            PathBuf::from("/home/u/.cache/junius")
        );
    }
}
