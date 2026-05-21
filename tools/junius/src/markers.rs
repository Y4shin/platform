//! Marker-comment editing for co-managed files (e.g. `Cargo.toml`).
//!
//! Files like `Cargo.toml` are partly hand-edited and partly written by
//! `junius sync`. The convention: managed regions are delimited by
//!
//! ```text
//! # >>> junius managed start
//! …
//! # <<< junius managed end
//! ```
//!
//! `junius sync` only ever rewrites text **between** these markers, and
//! preserves everything else byte-for-byte. If a human edits inside a
//! managed region, `junius sync` overwrites their changes (and `junius
//! check` reports the drift — coming in a later milestone).

use std::fmt::Write as _;

const START_MARKER: &str = "# >>> junius managed start";
const END_MARKER: &str = "# <<< junius managed end";

#[derive(Debug, thiserror::Error)]
pub enum MarkerError {
    #[error("start marker {marker:?} not found")]
    StartMissing { marker: &'static str },
    #[error("end marker {marker:?} not found after start at line {start_line}")]
    EndMissing {
        marker: &'static str,
        start_line: usize,
    },
}

/// Replace the contents of the managed region inside `original` with `new_body`
/// (a sequence of lines). Returns the rewritten file as a single string,
/// preserving the line ending of the surrounding text.
///
/// `new_body` is inserted **between** the marker lines, indented to match the
/// indentation of the start marker. Each entry in `new_body` becomes one line.
/// `new_body` may be empty (the managed region collapses to just the two
/// marker lines).
pub fn replace_managed(original: &str, new_body: &[String]) -> Result<String, MarkerError> {
    let lines: Vec<&str> = original.split_inclusive('\n').collect();

    let mut start_idx = None;
    let mut start_indent = String::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(idx) = line.find(START_MARKER) {
            start_idx = Some(i);
            start_indent = line[..idx].to_string();
            break;
        }
    }
    let start_idx = start_idx.ok_or(MarkerError::StartMissing {
        marker: START_MARKER,
    })?;

    let end_idx = lines
        .iter()
        .enumerate()
        .skip(start_idx + 1)
        .find(|(_, line)| line.contains(END_MARKER))
        .map(|(i, _)| i)
        .ok_or(MarkerError::EndMissing {
            marker: END_MARKER,
            start_line: start_idx + 1,
        })?;

    let mut out = String::with_capacity(original.len() + 64);
    for line in &lines[..=start_idx] {
        out.push_str(line);
    }
    for entry in new_body {
        let _ = writeln!(out, "{start_indent}{entry}");
    }
    for line in &lines[end_idx..] {
        out.push_str(line);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    const TOML: &str = "\
[workspace]
members = [
    \"a\",
    \"b\",
    # >>> junius managed start
    # <<< junius managed end
]

[other]
foo = 1
";

    #[test]
    fn replaces_empty_region() {
        let body = vec!["\"plugins/hello\",".to_string()];
        let rewritten = replace_managed(TOML, &body).unwrap();
        assert!(rewritten.contains("    \"plugins/hello\","));
        assert!(rewritten.contains("[other]\nfoo = 1\n"));
    }

    #[test]
    fn replaces_existing_region() {
        let with_content = TOML.replace(
            "    # <<< junius managed end",
            "    \"plugins/old\",\n    # <<< junius managed end",
        );
        let body = vec![
            "\"plugins/hello\",".to_string(),
            "\"plugins/world\",".to_string(),
        ];
        let rewritten = replace_managed(&with_content, &body).unwrap();
        assert!(rewritten.contains("\"plugins/hello\","));
        assert!(rewritten.contains("\"plugins/world\","));
        assert!(!rewritten.contains("\"plugins/old\","));
    }

    #[test]
    fn empty_body_collapses_region() {
        let with_content = TOML.replace(
            "    # <<< junius managed end",
            "    \"plugins/old\",\n    # <<< junius managed end",
        );
        let rewritten = replace_managed(&with_content, &[]).unwrap();
        assert!(!rewritten.contains("plugins/old"));
        // Markers themselves remain.
        assert!(rewritten.contains(START_MARKER));
        assert!(rewritten.contains(END_MARKER));
    }

    #[test]
    fn missing_start_marker_is_error() {
        let err = replace_managed("no markers here", &[]).unwrap_err();
        assert!(matches!(err, MarkerError::StartMissing { .. }));
    }

    #[test]
    fn missing_end_marker_is_error() {
        let only_start = "# >>> junius managed start\nfoo\n";
        let err = replace_managed(only_start, &[]).unwrap_err();
        assert!(matches!(err, MarkerError::EndMissing { .. }));
    }

    #[test]
    fn preserves_content_outside_region() {
        let body = vec!["\"plugins/hello\",".to_string()];
        let rewritten = replace_managed(TOML, &body).unwrap();
        // Everything before the start marker is unchanged.
        let before = &rewritten[..rewritten.find(START_MARKER).unwrap()];
        assert_eq!(before, &TOML[..TOML.find(START_MARKER).unwrap()]);
        // Everything after the end marker is unchanged.
        let after_old = &TOML[TOML.find(END_MARKER).unwrap()..];
        let after_new = &rewritten[rewritten.find(END_MARKER).unwrap()..];
        assert_eq!(after_old, after_new);
    }
}
