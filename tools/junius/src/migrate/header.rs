//! Parsing of `-- @requires <plugin>:<migration_name>` directives from the
//! leading comment block of a migration file. Anything after the first
//! non-comment, non-blank line is executable SQL and is not scanned.

/// A cross-plugin ordering edge: this migration requires `<plugin>:<name>` to
/// have been applied first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequireEdge {
    pub plugin: String,
    pub name: String,
}

/// Extract all `@requires` edges from the leading comment block of `sql`.
#[must_use]
pub fn parse_requires(sql: &str) -> Vec<RequireEdge> {
    let mut edges = Vec::new();
    for line in sql.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        let Some(comment) = trimmed.strip_prefix("--") else {
            break; // first executable SQL line ends the header block
        };
        if let Some(spec) = comment.trim().strip_prefix("@requires") {
            if let Some((plugin, name)) = spec.trim().split_once(':') {
                let (plugin, name) = (plugin.trim(), name.trim());
                if !plugin.is_empty() && !name.is_empty() {
                    edges.push(RequireEdge {
                        plugin: plugin.to_string(),
                        name: name.to_string(),
                    });
                }
            }
        }
    }
    edges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_requires_in_header() {
        let sql = "-- @requires speakers:0007_create_speaker\n\
                   -- @requires platform:0003_users\n\
                   \n\
                   CREATE TABLE events.event ();\n";
        assert_eq!(
            parse_requires(sql),
            vec![
                RequireEdge {
                    plugin: "speakers".into(),
                    name: "0007_create_speaker".into()
                },
                RequireEdge {
                    plugin: "platform".into(),
                    name: "0003_users".into()
                },
            ]
        );
    }

    #[test]
    fn stops_at_first_sql_line() {
        // A @requires that appears *after* SQL must not be picked up.
        let sql = "CREATE TABLE x ();\n-- @requires platform:0001_users\n";
        assert!(parse_requires(sql).is_empty());
    }

    #[test]
    fn tolerates_blank_lines_and_plain_comments() {
        let sql = "\n-- a plain comment\n-- @requires a:0001_x\nCREATE TABLE y ();\n";
        assert_eq!(
            parse_requires(sql),
            vec![RequireEdge {
                plugin: "a".into(),
                name: "0001_x".into()
            }]
        );
    }

    #[test]
    fn ignores_malformed_requires() {
        let sql = "-- @requires no-colon-here\n-- @requires :missing-plugin\nCREATE TABLE z ();\n";
        assert!(parse_requires(sql).is_empty());
    }
}
