-- @requires platform:0008_authz_functions
-- M18 Stage A — user-roles + admin override + membership provenance.
--
-- A `user_role` is the global-scope counterpart to `group_role`: it grants
-- permissions to the user across every group/resource, not within one group.
-- The built-in `admin` role holds the literal `'*'` permission; the
-- `user_can_access` definer function short-circuits on it before touching
-- `resource_principal`/`resource_share`, so the admin fast-path automatically
-- covers every plugin's ACL queries without per-plugin changes.
--
-- `group_membership` also grows a `managed_by` provenance column (Stage C
-- writes 'oidc'; Stage D writes 'config'; the admin UI in Stage B respects
-- the discriminator when deciding whether to allow edits).

CREATE TABLE platform.user_role (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name          TEXT NOT NULL UNIQUE,                  -- 'admin', 'support', …
    description   TEXT,
    is_builtin    BOOLEAN NOT NULL DEFAULT false,        -- protects 'admin' from accidental delete
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE platform.user_role_permission (
    role_id       UUID NOT NULL REFERENCES platform.user_role(id) ON DELETE CASCADE,
    permission    TEXT NOT NULL,                          -- '<plugin>:<perm>' or '*' (admin wildcard)
    PRIMARY KEY (role_id, permission)
);

CREATE TABLE platform.user_role_assignment (
    user_id       UUID NOT NULL REFERENCES platform.user(id)      ON DELETE CASCADE,
    role_id       UUID NOT NULL REFERENCES platform.user_role(id) ON DELETE CASCADE,
    granted_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    granted_by    UUID REFERENCES platform.user(id),    -- the admin who granted (NULL = system/config)
    PRIMARY KEY (user_id, role_id)
);
CREATE INDEX user_role_assignment_role_idx ON platform.user_role_assignment(role_id);

-- Provenance on per-group membership rows. Existing rows stay 'manual' (the
-- default), so a fresh apply is a no-op for them.
ALTER TABLE platform.group_membership
    ADD COLUMN managed_by      TEXT NOT NULL DEFAULT 'manual'
        CHECK (managed_by IN ('manual', 'oidc', 'config')),
    ADD COLUMN managed_source  TEXT;

-- Seed the built-in admin role. Idempotent so re-running the migration in
-- tests is safe; `is_builtin` is re-asserted on the conflict path because
-- the column was added by this migration.
INSERT INTO platform.user_role (name, description, is_builtin)
VALUES ('admin', 'Full access to every permission in every group.', true)
ON CONFLICT (name) DO UPDATE SET
    description = EXCLUDED.description,
    is_builtin  = true;

INSERT INTO platform.user_role_permission (role_id, permission)
SELECT id, '*' FROM platform.user_role WHERE name = 'admin'
ON CONFLICT DO NOTHING;

-- Replace `user_can_access` with an admin fast-path on top of the M08 body.
-- The fast-path is a single EXISTS over user_role_assignment + user_role_permission;
-- it short-circuits before the existing ownership/membership/share/public checks.
-- SECURITY DEFINER + the migrator's grants on the new tables make this safe to
-- expose via the existing PUBLIC EXECUTE.
CREATE OR REPLACE FUNCTION platform.user_can_access(
    p_resource_kind TEXT,
    p_resource_id   UUID,
    p_user_id       UUID,
    p_permission    TEXT
) RETURNS BOOLEAN
LANGUAGE plpgsql STABLE
SECURITY DEFINER
SET search_path = platform, pg_temp
AS $$
DECLARE
    v_owner_user_id  UUID;
    v_owner_group_id UUID;
BEGIN
    -- Step 0: admin fast-path. Any user-role assignment whose role holds '*'
    -- bypasses ACL evaluation entirely.
    IF EXISTS (
        SELECT 1
        FROM platform.user_role_assignment ura
        JOIN platform.user_role_permission urp ON urp.role_id = ura.role_id
        WHERE ura.user_id    = p_user_id
          AND urp.permission = '*'
    ) THEN
        RETURN true;
    END IF;

    -- Step 1: resolve ownership.
    SELECT owner_user_id, owner_group_id
    INTO v_owner_user_id, v_owner_group_id
    FROM platform.resource_principal
    WHERE resource_kind = p_resource_kind AND resource_id = p_resource_id;

    IF v_owner_user_id = p_user_id THEN RETURN true; END IF;

    IF v_owner_group_id IS NOT NULL AND EXISTS (
        SELECT 1
        FROM platform.group_membership gm
        JOIN platform.role_permission rp ON rp.role_id = gm.role_id
        WHERE gm.user_id    = p_user_id
          AND gm.group_id   = v_owner_group_id
          AND rp.permission = p_permission
    ) THEN RETURN true; END IF;

    IF EXISTS (
        SELECT 1 FROM platform.resource_share rs
        WHERE rs.resource_kind = p_resource_kind
          AND rs.resource_id   = p_resource_id
          AND rs.permission    = p_permission
          AND rs.principal_user_id = p_user_id
          AND (rs.expires_at IS NULL OR rs.expires_at > now())
    ) THEN RETURN true; END IF;

    IF EXISTS (
        SELECT 1
        FROM platform.resource_share rs
        JOIN platform.group_membership gm ON gm.group_id = rs.principal_group_id
        WHERE rs.resource_kind = p_resource_kind
          AND rs.resource_id   = p_resource_id
          AND rs.permission    = p_permission
          AND gm.user_id       = p_user_id
          AND (rs.expires_at IS NULL OR rs.expires_at > now())
    ) THEN RETURN true; END IF;

    IF EXISTS (
        SELECT 1 FROM platform.resource_share rs
        WHERE rs.resource_kind = p_resource_kind
          AND rs.resource_id   = p_resource_id
          AND rs.permission    = p_permission
          AND rs.principal_kind = 'public'
          AND (rs.expires_at IS NULL OR rs.expires_at > now())
    ) THEN RETURN true; END IF;

    RETURN false;
END;
$$;
