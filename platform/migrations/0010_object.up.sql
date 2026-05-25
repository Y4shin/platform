-- Host migration: every stored object is a first-class row so plugins can FK
-- their records to it (files-as-rows). Writes go through SECURITY DEFINER
-- functions (mirroring M08 `record_owner`) owned by platform_migrator, so the
-- single privileged host path records/removes rows; plugin roles get SELECT +
-- REFERENCES for cross-plugin FKs and joins.

CREATE TABLE platform.object (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plugin          TEXT NOT NULL,
    logical_bucket  TEXT NOT NULL,
    physical_bucket TEXT NOT NULL,
    object_key      TEXT NOT NULL,                       -- '<plugin>/<key>'
    content_type    TEXT,
    size_bytes      BIGINT,
    visibility      TEXT NOT NULL DEFAULT 'private',     -- 'private' | 'public'
    owner_user_id   UUID REFERENCES platform.user(id),
    created         TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (physical_bucket, object_key)
);

CREATE INDEX object_plugin_idx ON platform.object(plugin, logical_bucket);

-- Upsert a stored object's row, returning its id. SECURITY DEFINER so the row is
-- written with the owner's privileges regardless of caller role.
CREATE FUNCTION platform.record_object(
    p_plugin          TEXT,
    p_logical_bucket  TEXT,
    p_physical_bucket TEXT,
    p_object_key      TEXT,
    p_content_type    TEXT,
    p_size_bytes      BIGINT,
    p_visibility      TEXT,
    p_owner_user_id   UUID
) RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = platform, pg_temp
AS $$
DECLARE
    v_id UUID;
BEGIN
    INSERT INTO platform.object
        (plugin, logical_bucket, physical_bucket, object_key,
         content_type, size_bytes, visibility, owner_user_id)
    VALUES
        (p_plugin, p_logical_bucket, p_physical_bucket, p_object_key,
         p_content_type, p_size_bytes, p_visibility, p_owner_user_id)
    ON CONFLICT (physical_bucket, object_key) DO UPDATE
        SET content_type = EXCLUDED.content_type,
            size_bytes   = EXCLUDED.size_bytes,
            visibility   = EXCLUDED.visibility,
            owner_user_id = EXCLUDED.owner_user_id
    RETURNING id INTO v_id;
    RETURN v_id;
END;
$$;

-- Remove an object's row by physical location. SECURITY DEFINER for the same
-- reason. A FK from a plugin row will block deletion until the plugin clears it.
CREATE FUNCTION platform.delete_object(
    p_physical_bucket TEXT,
    p_object_key      TEXT
) RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = platform, pg_temp
AS $$
BEGIN
    DELETE FROM platform.object
    WHERE physical_bucket = p_physical_bucket AND object_key = p_object_key;
END;
$$;

-- Plugin roles must read + reference platform.object (cross-plugin FK target).
GRANT SELECT, REFERENCES ON platform.object TO PUBLIC;
