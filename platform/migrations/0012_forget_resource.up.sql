-- M16 item B: delete-time counterpart to `record_owner` (M08).
--
-- `platform.resource_principal` and `platform.resource_share` are written by
-- definer functions because plugin roles have no direct INSERT/DELETE on the
-- host tables. M08 shipped `record_owner` for create; this migration ships
-- `forget_resource` for delete, so the ACL rows can be cleared atomically
-- with the plugin's own row delete (one transaction, no orphaned rows).
--
-- Tables to clean: resource_share first, then resource_principal (no FK
-- between them, but matches the natural ownership-after-shares order).

CREATE FUNCTION platform.forget_resource(
    p_resource_kind TEXT,
    p_resource_id   UUID
) RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = platform, pg_temp
AS $$
BEGIN
    DELETE FROM platform.resource_share
    WHERE resource_kind = p_resource_kind AND resource_id = p_resource_id;
    DELETE FROM platform.resource_principal
    WHERE resource_kind = p_resource_kind AND resource_id = p_resource_id;
END;
$$;
