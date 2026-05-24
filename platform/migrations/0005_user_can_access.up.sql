-- Host migration: the central access-check function. Returns true when p_user_id
-- may exercise p_permission on the (p_resource_kind, p_resource_id) resource via
-- ownership, a group role, an explicit user/group share, or a public share.

CREATE OR REPLACE FUNCTION platform.user_can_access(
    p_resource_kind TEXT,
    p_resource_id   UUID,
    p_user_id       UUID,
    p_permission    TEXT
) RETURNS BOOLEAN
LANGUAGE plpgsql STABLE
AS $$
DECLARE
    v_owner_user_id  UUID;
    v_owner_group_id UUID;
BEGIN
    -- 0. Resolve ownership.
    SELECT owner_user_id, owner_group_id
    INTO v_owner_user_id, v_owner_group_id
    FROM platform.resource_principal
    WHERE resource_kind = p_resource_kind AND resource_id = p_resource_id;

    -- 1. Owner has implicit full access.
    IF v_owner_user_id = p_user_id THEN RETURN true; END IF;

    -- 2. Owned by a group where the user's role grants p_permission.
    IF v_owner_group_id IS NOT NULL AND EXISTS (
        SELECT 1
        FROM platform.group_membership gm
        JOIN platform.role_permission rp ON rp.role_id = gm.role_id
        WHERE gm.user_id    = p_user_id
          AND gm.group_id   = v_owner_group_id
          AND rp.permission = p_permission
    ) THEN RETURN true; END IF;

    -- 3. Explicit share to this user (active).
    IF EXISTS (
        SELECT 1 FROM platform.resource_share rs
        WHERE rs.resource_kind = p_resource_kind
          AND rs.resource_id   = p_resource_id
          AND rs.permission    = p_permission
          AND rs.principal_user_id = p_user_id
          AND (rs.expires_at IS NULL OR rs.expires_at > now())
    ) THEN RETURN true; END IF;

    -- 4. Explicit share to a group the user is in (active).
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

    -- 5. Public share (active).
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
