-- Host migration: groups, per-group roles, role permissions, and memberships.
-- A role is scoped to a group; a user holds at most one role per group.

CREATE TABLE platform.group (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name          TEXT NOT NULL,
    description   TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE platform.group_role (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    group_id      UUID NOT NULL REFERENCES platform.group(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    UNIQUE (group_id, name)
);

CREATE TABLE platform.role_permission (
    role_id       UUID NOT NULL REFERENCES platform.group_role(id) ON DELETE CASCADE,
    permission    TEXT NOT NULL,                 -- e.g. 'speakers:read'
    PRIMARY KEY (role_id, permission)
);

CREATE TABLE platform.group_membership (
    user_id       UUID NOT NULL REFERENCES platform.user(id) ON DELETE CASCADE,
    group_id      UUID NOT NULL REFERENCES platform.group(id) ON DELETE CASCADE,
    role_id       UUID NOT NULL REFERENCES platform.group_role(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, group_id)              -- one role per (user, group)
);
