-- Host migration: resource ownership (exactly one of user/group) and explicit
-- per-resource shares (to a user, a group, or the platform-wide public).

-- Ownership: exactly one of user_id / group_id is set.
CREATE TABLE platform.resource_principal (
    resource_kind   TEXT NOT NULL,               -- '<plugin>:<table>', e.g. 'speakers:speaker'
    resource_id     UUID NOT NULL,
    owner_user_id   UUID REFERENCES platform.user(id)  ON DELETE CASCADE,
    owner_group_id  UUID REFERENCES platform.group(id) ON DELETE CASCADE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (resource_kind, resource_id),
    CHECK ((owner_user_id IS NULL) <> (owner_group_id IS NULL))
);

-- Explicit per-resource grants.
CREATE TABLE platform.resource_share (
    resource_kind         TEXT NOT NULL,
    resource_id           UUID NOT NULL,
    principal_kind        TEXT NOT NULL,         -- 'user' | 'group' | 'public'
    principal_user_id     UUID REFERENCES platform.user(id)  ON DELETE CASCADE,
    principal_group_id    UUID REFERENCES platform.group(id) ON DELETE CASCADE,
    permission            TEXT NOT NULL,
    granted_by_user_id    UUID NOT NULL REFERENCES platform.user(id),
    granted_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at            TIMESTAMPTZ,
    CHECK (
        (principal_kind = 'user'   AND principal_user_id  IS NOT NULL AND principal_group_id IS NULL) OR
        (principal_kind = 'group'  AND principal_group_id IS NOT NULL AND principal_user_id  IS NULL) OR
        (principal_kind = 'public' AND principal_user_id  IS NULL     AND principal_group_id IS NULL)
    )
);
