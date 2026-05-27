-- @requires platform:0003_groups_roles_memberships
-- @requires platform:0013_user_roles
-- M18 Stage C — OIDC group → Junius (group, role) mapping.
--
-- One row per `(oidc_group_name, group_id)` pair: when a user logs in
-- carrying `oidc_group_name` in their `groups` claim, the host upserts a
-- `group_membership(user_id, group_id, role_id, managed_by='oidc',
-- managed_source=oidc_group_name)`. Memberships not present in the current
-- claim set are reaped, but only those with `managed_by='oidc'` —
-- manual/config memberships are untouched.

CREATE TABLE platform.oidc_group_mapping (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    oidc_group_name TEXT NOT NULL,
    group_id        UUID NOT NULL REFERENCES platform."group"(id)      ON DELETE CASCADE,
    role_id         UUID NOT NULL REFERENCES platform.group_role(id) ON DELETE CASCADE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- One mapping per (claim group, target group). Different roles into the
    -- same group from the same claim group would be ambiguous; flag the
    -- conflict at write time rather than silently picking one.
    UNIQUE (oidc_group_name, group_id)
);
CREATE INDEX oidc_group_mapping_name_idx ON platform.oidc_group_mapping (oidc_group_name);
