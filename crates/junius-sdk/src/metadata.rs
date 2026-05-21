//! `'static` view of a plugin's manifest, emitted by the `plugin_metadata!()`
//! proc-macro at compile time. The host reads this to decide how to mount the
//! plugin; plugin authors can also reach it via `Plugin::metadata()`.

#[derive(Debug)]
pub struct PluginMetadata {
    pub name: &'static str,
    pub display_name: &'static str,
    pub description: Option<&'static str>,
    pub manifest_schema: u32,
    pub mount: MountPoints,
    pub dependencies: &'static [DependencyDecl],
    pub exposed_components: &'static [ExposedComponentDecl],
    pub exposed_tables: &'static [ExposedTableDecl],
    pub permissions: &'static [PermissionDecl],
    pub capabilities: &'static [&'static str],
}

#[derive(Debug)]
pub struct MountPoints {
    pub route_prefix: &'static str,
    pub rpc_prefix: &'static str,
    pub http_prefix: &'static str,
}

#[derive(Debug)]
pub struct DependencyDecl {
    pub name: &'static str,
    pub optional: bool,
    pub tables: &'static [&'static str],
    pub rpc_methods: &'static [&'static str],
}

#[derive(Debug)]
pub struct ExposedComponentDecl {
    pub name: &'static str,
    pub module: &'static str,
    pub description: Option<&'static str>,
}

#[derive(Debug)]
pub struct ExposedTableDecl {
    pub name: &'static str,
    pub schema: &'static str,
    pub description: Option<&'static str>,
}

#[derive(Debug)]
pub struct PermissionDecl {
    pub name: &'static str,
    pub description: &'static str,
}
