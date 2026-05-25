# 20. M18 — Group & Role Provisioning

> **Status:** 🚧 planned.

> **Note:** Needs to be fleshed out.

Currently there is no real way of creating groups without going into the SQL console. This milestone should facilitate:
- Creating user-roles (not group-roles), including a special admin role, that grants access to everything.
- Admins can create groups and edit the role-permission-tables for group and user roles
- Group provisioning should also be possible by looking at the user's OIDC groups (i.e. oidc group `a` grants membership in junius group `b` using role `c`)
- There should be some way of provisioning groups from configurations together with all their configuration, roles, and role-permission-tables