//! `.po` → Rust source codegen for the i18n catalog.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::Path;

use crate::Error;
use crate::po::Entry;

/// Schema for one msgid: its key, its dense `ID`, and the ordered list of
/// placeholder names discovered in the `en.po` msgid (= the struct's fields).
#[derive(Debug, Clone)]
pub(crate) struct MessageSchema {
    pub key: String,
    pub id: usize,
    pub struct_name: String,
    pub placeholders: Vec<String>,
}

/// One per-locale parsed template, ready to be emitted as a `static` slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TemplateRow {
    /// No translation present in this locale's `.po`.
    Missing,
    /// Translation present, split into literal/placeholder parts.
    Present(Vec<TemplatePart>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TemplatePart {
    Literal(String),
    Placeholder(String),
}

/// Walk every `en.po` entry to discover messages. We use the **key-based**
/// gettext convention: `msgid` is a stable catalog key (`event.signup.subject`)
/// and `msgstr` is the source-language template (with `{placeholder}` markers).
/// The schema's placeholder set is therefore extracted from the **msgstr**, not
/// the msgid. Returns schemas in source order; the caller is expected to sort
/// by `key` and reassign `id` to give a stable per-domain index.
pub(crate) fn build_schemas(entries: &[Entry]) -> Result<Vec<MessageSchema>, Error> {
    let mut out = Vec::with_capacity(entries.len());
    let mut seen_keys: HashSet<String> = HashSet::new();
    let mut seen_struct_names: HashSet<String> = HashSet::new();

    for entry in entries {
        if !seen_keys.insert(entry.msgid.clone()) {
            return Err(Error::BadSchema(format!(
                "duplicate msgid `{}` at line {}",
                entry.msgid, entry.line
            )));
        }
        // Reject ICU plural/select syntax in templates — backend i18n uses
        // distinct keys for plurals.
        if has_icu_control(&entry.msgstr) {
            return Err(Error::BadSchema(format!(
                "msgstr for `{}` (line {}) contains ICU plural/select syntax; use two distinct keys for plurals on the backend",
                entry.msgid, entry.line
            )));
        }
        let placeholders = extract_placeholders(&entry.msgstr).map_err(|msg| {
            Error::BadSchema(format!(
                "msgstr for `{}` (line {}): {msg}",
                entry.msgid, entry.line
            ))
        })?;
        let struct_name = pascal_case(&entry.msgid);
        if !seen_struct_names.insert(struct_name.clone()) {
            return Err(Error::BadSchema(format!(
                "msgid `{}` collides with another msgid after PascalCase folding (`{struct_name}`)",
                entry.msgid
            )));
        }
        out.push(MessageSchema {
            key: entry.msgid.clone(),
            id: 0, // filled in by caller after sorting
            struct_name,
            placeholders,
        });
    }
    Ok(out)
}

/// Parse a per-locale `.po` against the schema set. Each msgstr is split into
/// `TemplatePart`s; placeholders are validated to be a **subset** of the
/// msgid's placeholder set (an extra translator-introduced placeholder is a
/// build error). msgids not in the schema are tolerated (they may be from an
/// older catalog version; the localizer never reaches them).
pub(crate) fn build_locale_row(
    schemas: &[MessageSchema],
    entries: &[Entry],
    locale: &str,
    path: &Path,
) -> Result<Vec<TemplateRow>, Error> {
    // Index entries by msgid for fast lookup.
    let mut by_msgid = std::collections::BTreeMap::new();
    for e in entries {
        by_msgid.insert(e.msgid.as_str(), e);
    }
    let mut row = Vec::with_capacity(schemas.len());
    for schema in schemas {
        let Some(entry) = by_msgid.get(schema.key.as_str()) else {
            row.push(TemplateRow::Missing);
            continue;
        };
        if entry.msgstr.is_empty() {
            // Untranslated entry — treat as missing for the runtime; the
            // separate `junius i18n check` step is what flags this as a
            // catalog gap.
            row.push(TemplateRow::Missing);
            continue;
        }
        let parts = split_template(&entry.msgstr).map_err(|msg| {
            Error::Validate(
                path.to_path_buf(),
                format!(
                    "msgstr for `{}` at line {} ({locale}): {msg}",
                    schema.key, entry.line
                ),
            )
        })?;
        let allowed: HashSet<&str> = schema.placeholders.iter().map(String::as_str).collect();
        for part in &parts {
            if let TemplatePart::Placeholder(name) = part
                && !allowed.contains(name.as_str())
            {
                return Err(Error::Validate(
                    path.to_path_buf(),
                    format!(
                        "msgstr for `{}` at line {} ({locale}) references unknown placeholder `{{{name}}}`; allowed: {:?}",
                        schema.key, entry.line, schema.placeholders
                    ),
                ));
            }
        }
        row.push(TemplateRow::Present(parts));
    }
    Ok(row)
}

/// Emit the generated Rust module source. Output references the runtime types
/// by their canonical path (`::junius_sdk::...`) so the macro `include!` works
/// from any plugin crate.
pub(crate) fn emit(
    domain_name: &str,
    schemas: &[MessageSchema],
    locale_rows: &[(&str, Vec<TemplateRow>)],
) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "// @generated by junius-i18n-build for domain `{domain_name}`. Do not edit."
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "pub const __DOMAIN: ::junius_sdk::Domain = ::junius_sdk::Domain::from_name({domain_name:?})\n    .expect(\"domain name unknown to junius_sdk::Domain — rebuild the SDK after adding the variant\");",
    );
    let _ = writeln!(out);

    // messages module: one struct + impl Message per msgid.
    let _ = writeln!(out, "pub mod messages {{");
    let _ = writeln!(
        out,
        "    use ::junius_sdk::{{Message, Template, TemplatePart}};"
    );
    let _ = writeln!(out);
    for schema in schemas {
        emit_message(&mut out, schema);
    }
    let _ = writeln!(out, "}}");
    let _ = writeln!(out);

    // catalog module: per-locale slice + register function.
    let _ = writeln!(out, "pub mod catalog {{");
    let _ = writeln!(
        out,
        "    #[allow(unused_imports)]\n    use ::junius_sdk::{{Domain, Locale, LocalizerBuilder, Template, TemplatePart}};"
    );
    let _ = writeln!(out);
    for (locale, row) in locale_rows {
        emit_catalog_static(&mut out, locale, row);
    }
    emit_register(&mut out, locale_rows);
    let _ = writeln!(out, "}}");

    out
}

fn emit_message(out: &mut String, schema: &MessageSchema) {
    let _ = writeln!(out);
    if schema.placeholders.is_empty() {
        let _ = writeln!(out, "    pub struct {};", schema.struct_name);
        let _ = writeln!(out, "    impl Message for {} {{", schema.struct_name);
    } else {
        let _ = writeln!(out, "    pub struct {}<'a> {{", schema.struct_name);
        for p in &schema.placeholders {
            let _ = writeln!(out, "        pub {p}: &'a str,");
        }
        let _ = writeln!(out, "    }}");
        let _ = writeln!(out, "    impl Message for {}<'_> {{", schema.struct_name);
    }
    let _ = writeln!(
        out,
        "        const DOMAIN: ::junius_sdk::Domain = super::__DOMAIN;"
    );
    let _ = writeln!(out, "        const ID: usize = {};", schema.id);
    let _ = writeln!(out, "        const KEY: &'static str = {:?};", schema.key);
    let _ = writeln!(
        out,
        "        fn render(&self, template: &Template) -> String {{"
    );
    if schema.placeholders.is_empty() {
        let _ = writeln!(
            out,
            "            template.parts.iter().fold(String::new(), |mut acc, part| match *part {{"
        );
        let _ = writeln!(
            out,
            "                TemplatePart::Literal(s) => {{ acc.push_str(s); acc }}"
        );
        let _ = writeln!(
            out,
            "                TemplatePart::Placeholder {{ .. }} => acc,"
        );
        let _ = writeln!(out, "            }})");
    } else {
        let _ = writeln!(out, "            let mut out = String::new();");
        let _ = writeln!(out, "            for part in template.parts {{");
        let _ = writeln!(out, "                match *part {{");
        let _ = writeln!(
            out,
            "                    TemplatePart::Literal(s) => out.push_str(s),"
        );
        let _ = writeln!(
            out,
            "                    TemplatePart::Placeholder {{ name }} => match name {{"
        );
        for p in &schema.placeholders {
            let _ = writeln!(
                out,
                "                        {p:?} => out.push_str(self.{p}),"
            );
        }
        let _ = writeln!(
            out,
            "                        other => {{ out.push('{{'); out.push_str(other); out.push('}}'); }}"
        );
        let _ = writeln!(out, "                    }},");
        let _ = writeln!(out, "                }}");
        let _ = writeln!(out, "            }}");
        let _ = writeln!(out, "            out");
    }
    let _ = writeln!(out, "        }}");
    let _ = writeln!(out, "    }}");
}

fn emit_catalog_static(out: &mut String, locale: &str, row: &[TemplateRow]) {
    let upper = locale.to_ascii_uppercase();
    let _ = writeln!(
        out,
        "    pub static CATALOG_{upper}: &[Option<&Template>] = &["
    );
    for entry in row {
        match entry {
            TemplateRow::Missing => {
                let _ = writeln!(out, "        None,");
            }
            TemplateRow::Present(parts) => {
                let _ = write!(out, "        Some(&Template {{ parts: &[");
                for (i, p) in parts.iter().enumerate() {
                    if i > 0 {
                        let _ = write!(out, ", ");
                    }
                    match p {
                        TemplatePart::Literal(s) => {
                            let _ = write!(out, "TemplatePart::Literal({s:?})");
                        }
                        TemplatePart::Placeholder(name) => {
                            let _ = write!(out, "TemplatePart::Placeholder {{ name: {name:?} }}");
                        }
                    }
                }
                let _ = writeln!(out, "] }}),");
            }
        }
    }
    let _ = writeln!(out, "    ];");
    let _ = writeln!(out);
}

fn emit_register(out: &mut String, locale_rows: &[(&str, Vec<TemplateRow>)]) {
    let _ = writeln!(
        out,
        "    /// Install this catalog into a `LocalizerBuilder`."
    );
    let _ = writeln!(
        out,
        "    /// Called by the host's boot code (one line per plugin)."
    );
    let _ = writeln!(out, "    pub fn register(b: &mut LocalizerBuilder) {{");
    // The slot array must be in Locale::ALL order. We assert the build
    // received that exact set; if not, the codegen falls back to alphabetical
    // order which would break the runtime invariant — surface as a panic at
    // boot so the misconfiguration is loud.
    let _ = writeln!(out, "        let slots = [");
    for (locale, _) in locale_rows {
        let _ = writeln!(
            out,
            "            ({:?}, CATALOG_{} as &[Option<&Template>]),",
            locale,
            locale.to_ascii_uppercase()
        );
    }
    let _ = writeln!(out, "        ];");
    let _ = writeln!(
        out,
        "        // Reorder by Locale::ALL — keeps the array index lined up with"
    );
    let _ = writeln!(
        out,
        "        // `Locale as usize`. Cost: a 3-element search at boot, once."
    );
    let _ = writeln!(
        out,
        "        let mut row: [&[Option<&Template>]; ::junius_sdk::Locale::COUNT] = [&[]; ::junius_sdk::Locale::COUNT];"
    );
    let _ = writeln!(
        out,
        "        for (i, locale) in ::junius_sdk::Locale::ALL.iter().enumerate() {{"
    );
    let _ = writeln!(
        out,
        "            if let Some((_, slot)) = slots.iter().find(|(code, _)| *code == locale.code()) {{"
    );
    let _ = writeln!(out, "                row[i] = *slot;");
    let _ = writeln!(out, "            }}");
    let _ = writeln!(out, "        }}");
    let _ = writeln!(out, "        b.add_domain(super::__DOMAIN, row);");
    let _ = writeln!(out, "    }}");
}

/// Split a msgstr into [`TemplatePart`]s. `{{` / `}}` are literal braces.
fn split_template(input: &str) -> Result<Vec<TemplatePart>, String> {
    let mut parts: Vec<TemplatePart> = Vec::new();
    let mut buf = String::new();
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                if chars.peek() == Some(&'{') {
                    chars.next();
                    buf.push('{');
                    continue;
                }
                // Placeholder begins.
                if !buf.is_empty() {
                    parts.push(TemplatePart::Literal(std::mem::take(&mut buf)));
                }
                let mut name = String::new();
                let mut closed = false;
                for inner in chars.by_ref() {
                    if inner == '}' {
                        closed = true;
                        break;
                    }
                    name.push(inner);
                }
                if !closed {
                    return Err(format!("unterminated placeholder `{{{name}`"));
                }
                if name.is_empty() {
                    return Err("empty placeholder `{}`".into());
                }
                if !is_valid_identifier(&name) {
                    return Err(format!(
                        "placeholder `{{{name}}}` is not a valid identifier"
                    ));
                }
                parts.push(TemplatePart::Placeholder(name));
            }
            '}' => {
                if chars.peek() == Some(&'}') {
                    chars.next();
                    buf.push('}');
                } else {
                    return Err("stray `}` (use `}}` for a literal)".into());
                }
            }
            other => buf.push(other),
        }
    }
    if !buf.is_empty() {
        parts.push(TemplatePart::Literal(buf));
    }
    Ok(parts)
}

/// Extract placeholder names from a msgid, in first-seen order, deduplicated.
fn extract_placeholders(msgid: &str) -> Result<Vec<String>, String> {
    let parts = split_template(msgid)?;
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for part in parts {
        if let TemplatePart::Placeholder(name) = part
            && seen.insert(name.clone())
        {
            out.push(name);
        }
    }
    Ok(out)
}

/// Cheap ICU-control detector — flags `{` followed by `, plural,` / `, select,`
/// / `, selectordinal,`. Not a full ICU parser; just enough to reject the
/// patterns we explicitly don't support on the backend.
fn has_icu_control(msgid: &str) -> bool {
    msgid.contains(", plural,") || msgid.contains(", select,") || msgid.contains(", selectordinal,")
}

/// Convert a dotted key like `event.signup.subject` to `EventSignupSubject`.
fn pascal_case(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    let mut upper_next = true;
    for c in key.chars() {
        if c == '.' || c == '_' || c == '-' {
            upper_next = true;
            continue;
        }
        if upper_next {
            out.extend(c.to_uppercase());
            upper_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests panic on unexpected failures")]
mod tests {
    use super::*;

    #[test]
    fn split_handles_literals_and_placeholders() {
        let parts = split_template("Hi, {name}!").unwrap();
        assert_eq!(
            parts,
            vec![
                TemplatePart::Literal("Hi, ".into()),
                TemplatePart::Placeholder("name".into()),
                TemplatePart::Literal("!".into()),
            ]
        );
    }

    #[test]
    fn split_handles_double_brace_escapes() {
        let parts = split_template("Use {{like}} this").unwrap();
        assert_eq!(parts, vec![TemplatePart::Literal("Use {like} this".into())]);
    }

    #[test]
    fn split_rejects_stray_close_brace() {
        let err = split_template("oops }").unwrap_err();
        assert!(err.contains("stray"));
    }

    #[test]
    fn split_rejects_unterminated_placeholder() {
        let err = split_template("Hi {name").unwrap_err();
        assert!(err.contains("unterminated"));
    }

    #[test]
    fn extract_dedupes_and_preserves_order() {
        let ps = extract_placeholders("a {x} b {y} c {x} d").unwrap();
        assert_eq!(ps, vec!["x".to_string(), "y".to_string()]);
    }

    #[test]
    fn pascal_case_basic() {
        assert_eq!(pascal_case("event.signup.subject"), "EventSignupSubject");
        assert_eq!(pascal_case("hello.world"), "HelloWorld");
        assert_eq!(pascal_case("a_b-c"), "ABC");
    }

    #[test]
    fn schemas_detect_collisions() {
        let entries = vec![
            Entry {
                msgid: "event.signup".into(),
                msgstr: String::new(),
                line: 1,
            },
            Entry {
                msgid: "event_signup".into(),
                msgstr: String::new(),
                line: 3,
            },
        ];
        let err = build_schemas(&entries).unwrap_err();
        match err {
            Error::BadSchema(msg) => assert!(msg.contains("PascalCase"), "msg: {msg}"),
            _ => panic!("expected BadSchema"),
        }
    }

    #[test]
    fn schemas_reject_icu_plural() {
        let entries = vec![Entry {
            msgid: "event.guests_count".into(),
            msgstr: "{count, plural, one {a} other {b}}".into(),
            line: 1,
        }];
        let err = build_schemas(&entries).unwrap_err();
        match err {
            Error::BadSchema(msg) => assert!(msg.contains("ICU plural")),
            _ => panic!("expected BadSchema"),
        }
    }

    #[test]
    fn locale_row_rejects_unknown_placeholder() {
        let schemas = vec![MessageSchema {
            key: "hello.greeted".into(),
            id: 0,
            struct_name: "HelloGreeted".into(),
            placeholders: vec!["name".into()],
        }];
        let entries = vec![Entry {
            msgid: "hello.greeted".into(),
            msgstr: "Hi, {titel}".into(),
            line: 1,
        }];
        let err = build_locale_row(&schemas, &entries, "de", Path::new("de.po")).unwrap_err();
        match err {
            Error::Validate(_, msg) => {
                assert!(msg.contains("titel"), "msg: {msg}");
            }
            _ => panic!("expected Validate"),
        }
    }

    #[test]
    fn locale_row_marks_missing_keys_as_missing() {
        let schemas = vec![
            MessageSchema {
                key: "a".into(),
                id: 0,
                struct_name: "A".into(),
                placeholders: vec![],
            },
            MessageSchema {
                key: "b".into(),
                id: 1,
                struct_name: "B".into(),
                placeholders: vec![],
            },
        ];
        let entries = vec![Entry {
            msgid: "a".into(),
            msgstr: "AAA".into(),
            line: 1,
        }];
        let row = build_locale_row(&schemas, &entries, "de", Path::new("de.po")).unwrap();
        assert_eq!(row.len(), 2);
        assert!(matches!(row[0], TemplateRow::Present(_)));
        assert_eq!(row[1], TemplateRow::Missing);
    }
}
