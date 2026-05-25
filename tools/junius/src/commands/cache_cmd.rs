//! `junius cache prune` — evict cached git sources older than a duration.

use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::cli::{CacheCmd, OutputFormat};
use crate::{cache, exit};

pub fn run(cmd: &CacheCmd, _format: OutputFormat) -> i32 {
    match cmd {
        CacheCmd::Prune { older_than } => match parse_duration(older_than) {
            Ok(d) => prune(&cache::sources_dir(), d),
            Err(e) => {
                eprintln!("junius: {e}");
                exit::PARSE_ERROR
            }
        },
    }
}

/// Remove immediate subdirectories of `sources_dir` whose mtime is at least
/// `older_than` in the past. `Duration::ZERO` evicts everything.
fn prune(sources_dir: &Path, older_than: Duration) -> i32 {
    let Some(cutoff) = SystemTime::now().checked_sub(older_than) else {
        // A window longer than the clock — nothing is that old.
        println!("junius: cache: pruned 0 source(s)");
        return exit::OK;
    };
    let Ok(entries) = std::fs::read_dir(sources_dir) else {
        println!(
            "junius: cache: nothing to prune ({} absent)",
            sources_dir.display()
        );
        return exit::OK;
    };

    let mut removed = 0_u32;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let old = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .is_some_and(|mtime| mtime <= cutoff);
        if old {
            if let Err(e) = std::fs::remove_dir_all(&path) {
                eprintln!("junius: cache: removing {}: {e}", path.display());
            } else {
                removed += 1;
            }
        }
    }
    println!("junius: cache: pruned {removed} source(s)");
    exit::OK
}

/// Parse a duration like `30d`, `12h`, `0d`. Units: `s`, `m`, `h`, `d`.
fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    let split = s
        .find(|c: char| c.is_ascii_alphabetic())
        .ok_or_else(|| format!("invalid duration {s:?} (expected e.g. 30d)"))?;
    let (num, unit) = s.split_at(split);
    let n: u64 = num
        .parse()
        .map_err(|_| format!("invalid duration {s:?} (expected e.g. 30d)"))?;
    let secs = match unit {
        "s" => n,
        "m" => n * 60,
        "h" => n * 3600,
        "d" => n * 86_400,
        other => return Err(format!("unknown duration unit {other:?} (use s/m/h/d)")),
    };
    Ok(Duration::from_secs(secs))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests panic on unexpected failures")]
mod tests {
    use super::*;

    #[test]
    fn parses_durations() {
        assert_eq!(
            parse_duration("30d").unwrap(),
            Duration::from_secs(30 * 86_400)
        );
        assert_eq!(
            parse_duration("12h").unwrap(),
            Duration::from_secs(12 * 3600)
        );
        assert_eq!(parse_duration("0d").unwrap(), Duration::ZERO);
        assert!(parse_duration("nope").is_err());
        assert!(parse_duration("5y").is_err());
    }

    #[test]
    fn prunes_old_sources_only() {
        let tmp = tempfile::tempdir().unwrap();
        let sources = tmp.path();
        let old = sources.join("old");
        std::fs::create_dir_all(&old).unwrap();
        // Ensure `new` is at least a second younger than `old`.
        std::thread::sleep(Duration::from_millis(1100));
        let new = sources.join("new");
        std::fs::create_dir_all(&new).unwrap();

        prune(sources, Duration::from_secs(1));
        assert!(!old.exists(), "old source pruned");
        assert!(new.exists(), "fresh source kept");

        prune(sources, Duration::ZERO);
        assert!(!new.exists(), "0d prunes everything");
    }
}
