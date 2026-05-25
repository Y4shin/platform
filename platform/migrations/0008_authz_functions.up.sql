-- @requires
-- M08: make the access-check function callable by least-privilege plugin roles,
-- add a definer function for recording ownership, and give resource_share a
-- stable id so shares can be returned + revoked individually.

-- A surface id for each share (the 0004 table had none). Empty in practice on a
-- fresh deployment, so the default backfill + PK are cheap.
ALTER TABLE platform.resource_share
    ADD COLUMN id UUID NOT NULL DEFAULT gen_random_uuid();
ALTER TABLE platform.resource_share
    ADD CONSTRAINT resource_share_pkey PRIMARY KEY (id);
--
-- `user_can_access` (from 0005) was SECURITY INVOKER, so its internal reads of
-- platform.resource_principal / resource_share / group_membership /
-- role_permission ran as the *caller*. Plugin roles (role_<name>) have no SELECT
-- on those tables, so a plugin repo calling it would fail. Recreate it
-- SECURITY DEFINER (owned by platform_migrator, which can read them). EXECUTE is
-- PUBLIC by default, so any plugin role may call it; the body is unchanged.
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

-- Record ownership of a freshly-created resource. SECURITY DEFINER so a plugin's
-- own transaction can write platform.resource_principal (which plugin roles have
-- no direct access to) atomically with the resource INSERT. The resource_principal
-- CHECK enforces exactly one of owner_user_id / owner_group_id.
CREATE FUNCTION platform.record_owner(
    p_resource_kind  TEXT,
    p_resource_id    UUID,
    p_owner_user_id  UUID,
    p_owner_group_id UUID
) RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = platform, pg_temp
AS $$
BEGIN
    INSERT INTO platform.resource_principal
        (resource_kind, resource_id, owner_user_id, owner_group_id)
    VALUES (p_resource_kind, p_resource_id, p_owner_user_id, p_owner_group_id);
END;
$$;
