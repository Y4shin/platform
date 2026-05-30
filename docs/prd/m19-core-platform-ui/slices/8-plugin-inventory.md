# Slice #8 — Plugin inventory

**PRD:** ../prd.md · **kind:** feature · **mode:** hitl

Full design: [`21-M19-platform-ui.md` §Tier 2.C](../../../impl/21-M19-platform-ui.md#tier-2c--plugin-inventory-padminplugins).

## What to build

End-to-end (admin plugin): an admin sees a read-only inventory of every enabled plugin at
`/p/admin/plugins`, sourced from the host's **compiled-in registry** (not on-disk manifests).

- Proto `PluginInventoryService.List(Empty) → ListPluginsResponse`, guarded
  `requires = "admin:plugins.read"`.
- `src/repo/plugin_inv.rs` + `src/service/plugins.rs`, reading the same registry data as
  M18's `PermissionCatalogService`.
- `frontend/src/routes/pages/PluginsPage.tsx` — read-only table. Columns: name + display
  name; version (from the plugin's `Cargo.toml`); declared permissions + a per-permission
  **holder count** (`COUNT(DISTINCT user_id)` joins through
  `role_permission` / `user_role_permission`); required capabilities; exposed/private tables;
  frontend present? (boolean).
- New permission `admin:plugins.read`.
- **Explicit non-goal:** no enable/disable toggle (a deploy-time concern owned by
  `junius sync` + a `platform.toml` edit); the page links to the file path instead.

## Acceptance criteria

- [ ] Lists every enabled plugin (events, hello, greetings, widgets, admin, …) with correct
      versions, declared permissions, and required capabilities.
- [ ] Per-permission holder counts are accurate (alice = 1 for `admin:*`; the events perms
      count = members of the Organisers group).
- [ ] No enable/disable toggle; the page links to the `platform.toml` path to edit.
- [ ] All new strings ship through Lingui.
- [ ] Playwright spec under `plugins/admin/frontend/e2e/`.

## Blocked by

- None — can start immediately.
