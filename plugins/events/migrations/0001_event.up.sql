-- @requires platform:0001_users
-- @requires platform:0003_groups_roles_memberships
-- The events plugin's own schema + the core `event` table. Runs as
-- platform_migrator; role_events is granted DML on this schema by the migration
-- runner's emit_grants step (events.event is declared in [exposes.tables]).
CREATE SCHEMA IF NOT EXISTS events;

-- Enum-typed columns (not free-text + CHECK IN) for a self-documenting schema.
CREATE TYPE events.visibility AS ENUM ('private', 'public');
CREATE TYPE events.owner_kind AS ENUM ('user', 'group');

CREATE TABLE events.event (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title           TEXT NOT NULL CHECK (length(btrim(title)) > 0),
    description     TEXT,
    location        TEXT,
    starts_at       TIMESTAMPTZ NOT NULL,
    ends_at         TIMESTAMPTZ,                       -- NULL = single instant; set for a ranged / multi-day event
    all_day         BOOLEAN NOT NULL DEFAULT false,
    visibility      events.visibility NOT NULL DEFAULT 'private',
    owner_kind      events.owner_kind NOT NULL,
    -- Real FKs, exactly one set per owner_kind; the authoritative ACL entry is
    -- also written to platform.resource_principal via record_owner(). No cross-
    -- schema cascade (RESTRICT) — satisfies junius check's FK.CROSS.CASCADE.
    owning_user_id  UUID REFERENCES platform.user(id),
    owning_group_id UUID REFERENCES platform.group(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT event_time_range CHECK (ends_at IS NULL OR ends_at >= starts_at),
    CONSTRAINT event_owner_matches_kind CHECK (
        (owner_kind = 'user'  AND owning_user_id  IS NOT NULL AND owning_group_id IS NULL)
     OR (owner_kind = 'group' AND owning_group_id IS NOT NULL AND owning_user_id  IS NULL)
    )
);

CREATE INDEX event_owner_user_idx  ON events.event (owning_user_id)  WHERE owning_user_id  IS NOT NULL;
CREATE INDEX event_owner_group_idx ON events.event (owning_group_id) WHERE owning_group_id IS NOT NULL;
CREATE INDEX event_visibility_idx  ON events.event (visibility);
CREATE INDEX event_starts_at_idx   ON events.event (starts_at);
