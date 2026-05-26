//! A minimal gettext `.po` parser, restricted to the format M14 ships.
//!
//! Supported: `msgid "..."` / `msgstr "..."` entries with multi-line
//! continuation (quoted strings on subsequent lines are concatenated). C-style
//! escapes (`\n`, `\t`, `\\`, `\"`) are recognised inside the quoted strings.
//!
//! Out of scope (rejected or ignored as appropriate): `msgid_plural`,
//! `msgctxt`, `#~` obsolete entries, `#,` flags. The codegen also rejects
//! ICU plural/select syntax inside an msgid via [`crate::codegen`], not here.

use std::fmt;

/// One parsed `.po` entry: msgid and the corresponding (possibly empty) msgstr.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub msgid: String,
    pub msgstr: String,
    /// 1-based source line where `msgid` appeared — used in error messages.
    pub line: usize,
}

/// Errors from [`parse`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// A quoted string was malformed or unterminated.
    BadQuoted { line: usize, msg: String },
    /// An entry lacked an `msgstr` after its `msgid`.
    MissingMsgstr { line: usize },
    /// A keyword we don't support (`msgid_plural`, `msgctxt`).
    UnsupportedKeyword { line: usize, keyword: String },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadQuoted { line, msg } => write!(f, "line {line}: bad quoted string: {msg}"),
            Self::MissingMsgstr { line } => write!(f, "line {line}: msgid without msgstr"),
            Self::UnsupportedKeyword { line, keyword } => {
                write!(f, "line {line}: unsupported keyword `{keyword}`")
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// Parse a `.po` source string. The empty-msgid header entry is dropped from
/// the result.
pub fn parse(src: &str) -> Result<Vec<Entry>, ParseError> {
    let lines: Vec<(usize, &str)> = src.lines().enumerate().map(|(i, l)| (i + 1, l)).collect();
    let mut entries = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let (lineno, raw) = lines[i];
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            i += 1;
            continue;
        }

        if let Some(rest) = strip_keyword(line, "msgid_plural") {
            let _ = rest;
            return Err(ParseError::UnsupportedKeyword {
                line: lineno,
                keyword: "msgid_plural".into(),
            });
        }
        if let Some(rest) = strip_keyword(line, "msgctxt") {
            let _ = rest;
            return Err(ParseError::UnsupportedKeyword {
                line: lineno,
                keyword: "msgctxt".into(),
            });
        }

        let Some(after_msgid) = strip_keyword(line, "msgid") else {
            return Err(ParseError::BadQuoted {
                line: lineno,
                msg: format!("expected `msgid \"...\"`, got `{line}`"),
            });
        };
        let mut msgid = parse_quoted(after_msgid, lineno)?;
        i += 1;
        i = consume_continuation(&lines, i, &mut msgid)?;

        // Skip blanks/comments before msgstr.
        while i < lines.len() {
            let (_, raw) = lines[i];
            let t = raw.trim();
            if t.is_empty() || t.starts_with('#') {
                i += 1;
                continue;
            }
            break;
        }
        let Some((str_lineno, str_raw)) = lines.get(i).copied() else {
            return Err(ParseError::MissingMsgstr { line: lineno });
        };
        let str_line = str_raw.trim();
        if strip_keyword(str_line, "msgid_plural").is_some() {
            return Err(ParseError::UnsupportedKeyword {
                line: str_lineno,
                keyword: "msgid_plural".into(),
            });
        }
        if str_line.starts_with("msgstr[") {
            return Err(ParseError::UnsupportedKeyword {
                line: str_lineno,
                keyword: "msgstr[N]".into(),
            });
        }
        let Some(after_msgstr) = strip_keyword(str_line, "msgstr") else {
            return Err(ParseError::MissingMsgstr { line: str_lineno });
        };
        let mut msgstr = parse_quoted(after_msgstr, str_lineno)?;
        i += 1;
        i = consume_continuation(&lines, i, &mut msgstr)?;

        // Drop the gettext header entry (empty msgid).
        if !msgid.is_empty() {
            entries.push(Entry {
                msgid,
                msgstr,
                line: lineno,
            });
        }
    }
    Ok(entries)
}

/// If `line` is exactly `<keyword>` followed by whitespace (or empty), return
/// the rest after the keyword. Avoids matching `msgid_plural` against `msgid`.
fn strip_keyword<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(keyword)?;
    let mut chars = rest.chars();
    match chars.next() {
        None => Some(rest),
        Some(c) if c.is_whitespace() => Some(rest.trim_start()),
        _ => None,
    }
}

/// Append continuation-string lines (additional quoted strings) to `out`.
/// Returns the next index to inspect.
fn consume_continuation(
    lines: &[(usize, &str)],
    mut i: usize,
    out: &mut String,
) -> Result<usize, ParseError> {
    while i < lines.len() {
        let (lineno, raw) = lines[i];
        let line = raw.trim();
        if line.starts_with('"') {
            let part = parse_quoted(line, lineno)?;
            out.push_str(&part);
            i += 1;
        } else {
            break;
        }
    }
    Ok(i)
}

/// Parse one quoted string literal: `"..."`, with C-style escapes inside.
fn parse_quoted(s: &str, lineno: usize) -> Result<String, ParseError> {
    let s = s.trim();
    if !s.starts_with('"') {
        return Err(ParseError::BadQuoted {
            line: lineno,
            msg: format!("expected leading `\"`, got `{s}`"),
        });
    }
    let inner = &s[1..];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    let mut closed = false;
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                closed = true;
                break;
            }
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some(other) => {
                    return Err(ParseError::BadQuoted {
                        line: lineno,
                        msg: format!("unknown escape `\\{other}`"),
                    });
                }
                None => {
                    return Err(ParseError::BadQuoted {
                        line: lineno,
                        msg: "trailing backslash".into(),
                    });
                }
            },
            other => out.push(other),
        }
    }
    if !closed {
        return Err(ParseError::BadQuoted {
            line: lineno,
            msg: "unterminated string".into(),
        });
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests panic on unexpected failures")]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_entry() {
        let src = "msgid \"foo\"\nmsgstr \"bar\"\n";
        let entries = parse(src).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].msgid, "foo");
        assert_eq!(entries[0].msgstr, "bar");
    }

    #[test]
    fn skips_header_entry() {
        let src =
            "msgid \"\"\nmsgstr \"Project-Id-Version: 1\\n\"\n\nmsgid \"foo\"\nmsgstr \"bar\"\n";
        let entries = parse(src).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].msgid, "foo");
    }

    #[test]
    fn handles_continuation_lines() {
        let src = "msgid \"\"\n\"part one \"\n\"part two\"\nmsgstr \"out\"\n";
        let entries = parse(src).unwrap();
        // The msgid is empty (header) + continuation → still considered the
        // header, since it now starts non-empty. Verify the actual value.
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].msgid, "part one part two");
        assert_eq!(entries[0].msgstr, "out");
    }

    #[test]
    fn ignores_comments_and_blank_lines() {
        let src = "# a comment\n\nmsgid \"foo\"\n# inline\nmsgstr \"bar\"\n";
        let entries = parse(src).unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn parses_c_escapes() {
        let src = "msgid \"a\"\nmsgstr \"line1\\nline2\\t\\\"quoted\\\"\"\n";
        let entries = parse(src).unwrap();
        assert_eq!(entries[0].msgstr, "line1\nline2\t\"quoted\"");
    }

    #[test]
    fn rejects_unterminated_quote() {
        let src = "msgid \"oops\nmsgstr \"\"\n";
        let err = parse(src).unwrap_err();
        assert!(matches!(err, ParseError::BadQuoted { .. }));
    }

    #[test]
    fn rejects_unsupported_keywords() {
        let src = "msgid \"a\"\nmsgid_plural \"as\"\nmsgstr[0] \"\"\n";
        let err = parse(src).unwrap_err();
        assert!(matches!(err, ParseError::UnsupportedKeyword { .. }));
    }

    #[test]
    fn missing_msgstr_is_an_error() {
        let src = "msgid \"foo\"\n";
        let err = parse(src).unwrap_err();
        assert!(matches!(err, ParseError::MissingMsgstr { .. }));
    }
}
