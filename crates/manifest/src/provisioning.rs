//! `[provisioning]` block of `platform.toml` (M18 Stage D).
//!
//! Declarative replacement for `dev/dev-seed.sql`: a deployment declares
//! its starting groups, roles, role permissions, user-roles, user-role
//! assignments, and OIDC group mappings. The `junius provision apply` CLI
//! reconciles the live DB against this declaration; re-running is cheap
//! (hash-guarded) and side-effect-free when nothing has changed.
//!
//! The block can be either inline (`[provisioning]` + `[[provisioning.groups]]`
//! tables in `platform.toml` directly) or a pointer to a sibling file
//! (`[provisioning] file = "provisioning.toml"`), so deployments with large
//! group/role inventories don't bloat the host config.

use std::path::Path;

use serde::Deserialize;

use crate::error::ManifestError;

/// The `[provisioning]` block.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisioningConfig {
    /// Optional path (relative to the deployment dir) to a separate
    /// provisioning file. When set, the inline `groups`/`user_roles`/…
    /// fields below must be empty — the pointer model exists precisely
    /// to keep large lists out of the host config.
    #[serde(default)]
    pub file: Option<String>,

    /// Auto-apply at host boot, after migrations. Default `true`: the
    /// hash-guard keeps unchanged configs from spamming the audit log,
    /// so the recurring cost is one comparison per restart.
    #[serde(default = "default_true")]
    pub auto_apply_on_boot: bool,

    /// Treat `managed_by='config'` rows as **read-only at every mutation
    /// seam** (the `PlatformAdminApi`, the admin UI, and the OIDC
    /// reconciler). Default `true`. Flip to `false` for "TOML as seed"
    /// semantics (the M18 plan's v1 default before the user feedback
    /// strengthened the contract).
    #[serde(default = "default_true")]
    pub lock_managed: bool,

    /// Groups + their roles + role permissions.
    #[serde(default)]
    pub groups: Vec<GroupDecl>,

    /// User-role declarations (the global-scope role axis from Stage 1).
    /// The built-in `admin` role can be listed here to attach a custom
    /// description; its `is_builtin = true` is preserved.
    #[serde(default)]
    pub user_roles: Vec<UserRoleDecl>,

    /// User-role assignments. Identify the user by `oidc_sub` or `email`.
    #[serde(default)]
    pub user_role_assignments: Vec<UserRoleAssignmentDecl>,

    /// OIDC group → Junius (group, role) mappings.
    #[serde(default)]
    pub oidc_mappings: Vec<OidcMappingDecl>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupDecl {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub roles: Vec<GroupRoleDecl>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupRoleDecl {
    pub name: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserRoleDecl {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Defaults to `false`; set `true` on the entry for the built-in
    /// `admin` role to suppress the "won't be `is_builtin` after apply"
    /// warning that would otherwise fire on every apply against the
    /// migration-seeded admin row.
    #[serde(default)]
    pub builtin: bool,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserRoleAssignmentDecl {
    /// Identify the user by stable OIDC subject (preferred — `sub` is
    /// the stable `IdP` identifier).
    #[serde(default)]
    pub oidc_sub: Option<String>,
    /// Or by email — accepted with a warning at apply time, resolved to
    /// `oidc_sub` via the `platform.user` row (errors if the user has
    /// never logged in).
    #[serde(default)]
    pub email: Option<String>,
    pub user_role: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OidcMappingDecl {
    pub oidc_group: String,
    pub group: String,
    pub role: String,
}

const fn default_true() -> bool {
    true
}

impl ProvisioningConfig {
    /// Resolve a `[provisioning] file = "…"` pointer: if `file` is set,
    /// load the referenced TOML and replace the in-memory block with its
    /// contents (preserving `auto_apply_on_boot` + `lock_managed` from
    /// the outer block — those are deployment-level knobs).
    pub fn resolve_file(self, deployment_dir: &Path) -> Result<Self, ManifestError> {
        let Some(file) = self.file.as_ref() else {
            return Ok(self);
        };
        let path = deployment_dir.join(file);
        let src = std::fs::read_to_string(&path).map_err(ManifestError::Io)?;
        let mut nested: ProvisioningConfig = toml::from_str(&src)?;
        // Outer's deployment knobs take precedence over anything the
        // pointed-to file might (re)declare.
        nested.file = None;
        nested.auto_apply_on_boot = self.auto_apply_on_boot;
        nested.lock_managed = self.lock_managed;
        Ok(nested)
    }

    /// Stable hash of the resolved block (blake3 over a deterministic
    /// serialisation). Used by the boot-time hash-guard so unchanged
    /// configs are a single comparison.
    #[must_use]
    pub fn content_hash(&self) -> String {
        // Serialize via serde_json with sorted keys for determinism. (TOML
        // serialisation doesn't sort consistently across versions.)
        let raw = serde_json::to_value(self).unwrap_or(serde_json::Value::Null);
        let sorted = sort_json(&raw);
        let bytes = serde_json::to_vec(&sorted).unwrap_or_default();
        blake3::hash(&bytes).to_hex().to_string()
    }
}

// Manual Serialize so we can compute a stable hash without depending on
// `serde_with`'s preserve_order.
impl serde::Serialize for ProvisioningConfig {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("ProvisioningConfig", 6)?;
        s.serialize_field("auto_apply_on_boot", &self.auto_apply_on_boot)?;
        s.serialize_field("lock_managed", &self.lock_managed)?;
        s.serialize_field("groups", &self.groups)?;
        s.serialize_field("user_roles", &self.user_roles)?;
        s.serialize_field("user_role_assignments", &self.user_role_assignments)?;
        s.serialize_field("oidc_mappings", &self.oidc_mappings)?;
        s.end()
    }
}

// Stable-order serialisations of the leaf decls — derive(Serialize) would
// give the same field order, but pinning them avoids surprises if fields
// are reordered later.
impl serde::Serialize for GroupDecl {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("GroupDecl", 3)?;
        s.serialize_field("name", &self.name)?;
        s.serialize_field("description", &self.description)?;
        s.serialize_field("roles", &self.roles)?;
        s.end()
    }
}
impl serde::Serialize for GroupRoleDecl {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("GroupRoleDecl", 2)?;
        s.serialize_field("name", &self.name)?;
        s.serialize_field("permissions", &self.permissions)?;
        s.end()
    }
}
impl serde::Serialize for UserRoleDecl {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("UserRoleDecl", 4)?;
        s.serialize_field("name", &self.name)?;
        s.serialize_field("description", &self.description)?;
        s.serialize_field("builtin", &self.builtin)?;
        s.serialize_field("permissions", &self.permissions)?;
        s.end()
    }
}
impl serde::Serialize for UserRoleAssignmentDecl {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("UserRoleAssignmentDecl", 3)?;
        s.serialize_field("oidc_sub", &self.oidc_sub)?;
        s.serialize_field("email", &self.email)?;
        s.serialize_field("user_role", &self.user_role)?;
        s.end()
    }
}
impl serde::Serialize for OidcMappingDecl {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("OidcMappingDecl", 3)?;
        s.serialize_field("oidc_group", &self.oidc_group)?;
        s.serialize_field("group", &self.group)?;
        s.serialize_field("role", &self.role)?;
        s.end()
    }
}

fn sort_json(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for k in keys {
                out.insert(k.clone(), sort_json(&map[k]));
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(sort_json).collect())
        }
        other => other.clone(),
    }
}
