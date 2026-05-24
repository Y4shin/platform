//! Per-plugin Postgres role grants and role-password derivation.
//!
//! Each enabled plugin gets a dedicated Postgres login role (`role_<name>`) with
//! least privilege: full DML + CREATE on its own schema(s), specific access to
//! tables it declared a dependency on, and read-only access to `platform.user`.
//! Postgres enforces this at the connection level — defense-in-depth independent
//! of the application-level permission checks that arrive in M07.
//!
//! Everything here is pure (data in, `String`/struct out, no IO). Both the
//! migration runner (which *creates* the roles) and the host (which *connects*
//! as them) consume these functions, so the grant set and the role passwords can
//! never drift between the two.

use std::collections::{BTreeMap, BTreeSet};

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::plugin::PluginManifest;

/// A cross-plugin table grant: a `schema.table` the role may read/write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepGrant {
    pub schema: String,
    pub table: String,
}

/// The full grant set a plugin's role should hold, derived from manifests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleGrant {
    /// Postgres role name, e.g. `role_hello`.
    pub role: String,
    /// Schemas the plugin owns (full DML + CREATE).
    pub own_schemas: Vec<String>,
    /// Tables in other plugins' schemas this plugin declared a dependency on.
    pub dep_tables: Vec<DepGrant>,
}

/// The Postgres role name for a plugin. Dashes in plugin names become
/// underscores so the result is a bare (unquoted) SQL identifier; this mirrors
/// the Rust module-name convention used elsewhere in `junius`.
#[must_use]
pub fn role_name(plugin_name: &str) -> String {
    format!("role_{}", plugin_name.replace('-', "_"))
}

/// Compute the grant set for `plugin`. `all` is the map of every plugin's
/// manifest (keyed by plugin name), needed to resolve a declared dependency's
/// table to the schema that dependency exposes it under. Dependencies that don't
/// resolve to a known exposed table are skipped (under-granting is the safe
/// failure direction; cross-plugin reference completeness is enforced by
/// `junius check`).
#[must_use]
pub fn compute_grants(
    plugin: &PluginManifest,
    all: &BTreeMap<String, PluginManifest>,
) -> RoleGrant {
    let mut own: BTreeSet<String> = BTreeSet::new();
    for table in plugin.exposes.tables.values() {
        own.insert(table.schema.clone());
    }

    let mut dep_tables = Vec::new();
    for (dep_name, dep) in &plugin.dependencies {
        let Some(dep_manifest) = all.get(dep_name) else {
            continue;
        };
        for table in &dep.tables {
            if let Some(exposed) = dep_manifest.exposes.tables.get(table) {
                dep_tables.push(DepGrant {
                    schema: exposed.schema.clone(),
                    table: table.clone(),
                });
            }
        }
    }

    RoleGrant {
        role: role_name(&plugin.plugin.name),
        own_schemas: own.into_iter().collect(),
        dep_tables,
    }
}

/// Render the idempotent SQL that ensures the role exists with `password` and
/// holds exactly `grant`'s privileges. Safe to run repeatedly.
///
/// Injection safety: `password` is a 64-char hex digest (see
/// [`derive_role_password`]), and every schema/table/role identifier originates
/// from a manifest field that validation constrains to `^[a-z][a-z0-9_]*$`
/// (schemas/tables) or is derived from the validated plugin name (role) — so no
/// untrusted text reaches the generated statement.
#[must_use]
pub fn emit_grant_sql(grant: &RoleGrant, password: &str) -> String {
    use std::fmt::Write as _;

    let role = &grant.role;
    let mut sql = String::new();

    // Ensure the role exists and its password matches the derived value
    // (re-deriving on every run keeps it in sync if the host secret rotates).
    let _ = write!(
        sql,
        "DO $$\n\
         BEGIN\n\
         \x20 IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = '{role}') THEN\n\
         \x20   CREATE ROLE {role} NOINHERIT LOGIN PASSWORD '{password}';\n\
         \x20 ELSE\n\
         \x20   ALTER ROLE {role} NOINHERIT LOGIN PASSWORD '{password}';\n\
         \x20 END IF;\n\
         END$$;\n"
    );

    for schema in &grant.own_schemas {
        let _ = write!(
            sql,
            "\nGRANT USAGE, CREATE ON SCHEMA {schema} TO {role};\n\
             GRANT ALL ON ALL TABLES IN SCHEMA {schema} TO {role};\n\
             ALTER DEFAULT PRIVILEGES IN SCHEMA {schema} GRANT ALL ON TABLES TO {role};\n"
        );
    }

    // Cross-plugin grants: USAGE on the schema once, DML on each declared table.
    let dep_schemas: BTreeSet<&str> = grant.dep_tables.iter().map(|d| d.schema.as_str()).collect();
    for schema in dep_schemas {
        let _ = write!(sql, "\nGRANT USAGE ON SCHEMA {schema} TO {role};\n");
    }
    for dep in &grant.dep_tables {
        let _ = writeln!(
            sql,
            "GRANT SELECT, INSERT, UPDATE, DELETE ON {}.{} TO {role};",
            dep.schema, dep.table
        );
    }

    // Host read-only surface.
    let _ = write!(
        sql,
        "\nGRANT USAGE ON SCHEMA platform TO {role};\n\
         GRANT SELECT ON platform.user TO {role};\n"
    );

    sql
}

/// Derive a role's password as `HMAC-SHA256(role_password_secret, role_name)`,
/// hex-encoded. Deterministic: `junius migrate` creates the role with this
/// password and the host re-derives the same value to connect, so nothing is
/// stored per role — the single shared `role_password_secret` is the only
/// coupling between the two processes.
#[must_use]
pub fn derive_role_password(role_password_secret: &str, role: &str) -> String {
    type HmacSha256 = Hmac<Sha256>;

    #[allow(
        clippy::expect_used,
        reason = "HMAC accepts a key of any length; new_from_slice is infallible here"
    )]
    let mut mac = HmacSha256::new_from_slice(role_password_secret.as_bytes())
        .expect("HMAC accepts keys of any length");
    mac.update(role.as_bytes());
    let bytes = mac.finalize().into_bytes();

    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "tests panic on unexpected parse failures"
)]
mod tests {
    use super::*;

    fn manifest(name: &str, schema: &str) -> PluginManifest {
        let toml = format!(
            "[plugin]\nname = \"{name}\"\ndisplay_name = \"{name}\"\nmanifest_schema = 1\n\
             [exposes.tables.thing]\nschema = \"{schema}\"\n"
        );
        PluginManifest::parse(&toml).unwrap()
    }

    #[test]
    fn role_name_sanitizes_dashes() {
        assert_eq!(role_name("hello"), "role_hello");
        assert_eq!(role_name("my-plugin"), "role_my_plugin");
    }

    #[test]
    fn compute_grants_owns_its_schema() {
        let hello = manifest("hello", "hello");
        let mut all = BTreeMap::new();
        all.insert("hello".to_string(), hello.clone());
        let grant = compute_grants(&hello, &all);
        assert_eq!(grant.role, "role_hello");
        assert_eq!(grant.own_schemas, vec!["hello".to_string()]);
        assert!(grant.dep_tables.is_empty());
    }

    #[test]
    fn compute_grants_resolves_dep_table_schema() {
        let speakers = manifest("speakers", "speakers");
        let events_toml = "[plugin]\nname = \"events\"\ndisplay_name = \"events\"\nmanifest_schema = 1\n\
             [exposes.tables.event]\nschema = \"events\"\n\
             [dependencies.speakers]\ntables = [\"thing\"]\n";
        let events = PluginManifest::parse(events_toml).unwrap();
        let mut all = BTreeMap::new();
        all.insert("speakers".to_string(), speakers);
        all.insert("events".to_string(), events.clone());

        let grant = compute_grants(&events, &all);
        assert_eq!(
            grant.dep_tables,
            vec![DepGrant {
                schema: "speakers".to_string(),
                table: "thing".to_string()
            }]
        );
    }

    #[test]
    fn derive_role_password_is_deterministic_and_key_sensitive() {
        let a = derive_role_password("secret-1", "role_hello");
        let b = derive_role_password("secret-1", "role_hello");
        let c = derive_role_password("secret-2", "role_hello");
        let d = derive_role_password("secret-1", "role_other");
        assert_eq!(a, b, "same inputs must yield the same password");
        assert_ne!(a, c, "a different secret must change the password");
        assert_ne!(a, d, "a different role must change the password");
        assert_eq!(a.len(), 64, "HMAC-SHA256 hex is 64 chars");
        assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn emit_grant_sql_contains_least_privilege_surface() {
        let grant = RoleGrant {
            role: "role_hello".to_string(),
            own_schemas: vec!["hello".to_string()],
            dep_tables: vec![],
        };
        let sql = emit_grant_sql(&grant, "deadbeef");
        assert!(sql.contains("CREATE ROLE role_hello NOINHERIT LOGIN PASSWORD 'deadbeef'"));
        assert!(sql.contains("GRANT USAGE, CREATE ON SCHEMA hello TO role_hello;"));
        assert!(sql.contains("GRANT SELECT ON platform.user TO role_hello;"));
        // Least privilege: no blanket access to the platform schema's tables.
        assert!(!sql.contains("ALL TABLES IN SCHEMA platform"));
        assert!(!sql.contains("platform.session"));
    }
}
