-- @requires platform:0001_users
-- @requires platform:0003_groups_roles_memberships
-- Backs the keyed private calendar feeds (/ics/u/<key>, /ics/g/<name>/<key>) and
-- the per-group public toggle. Keys live only in the URL; we store their SHA-256.
-- Cross-schema FKs into platform.user/group are nullable / non-cascading.
CREATE TYPE events.feed_kind AS ENUM ('personal', 'group');

CREATE TABLE events.calendar_token (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    token_hash       TEXT NOT NULL UNIQUE,         -- SHA-256 of <key>; the secret lives only in the URL
    kind             events.feed_kind NOT NULL,
    subject_user_id  UUID REFERENCES platform.user(id),
    subject_group_id UUID REFERENCES platform.group(id),
    created_by       UUID NOT NULL REFERENCES platform.user(id),
    label            TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at       TIMESTAMPTZ,                  -- soft-revoke; a revoked key stops serving
    CONSTRAINT token_subject_matches_kind CHECK (
        (kind = 'personal' AND subject_user_id  IS NOT NULL AND subject_group_id IS NULL)
     OR (kind = 'group'    AND subject_group_id IS NOT NULL AND subject_user_id  IS NULL)
    )
);

CREATE INDEX calendar_token_created_by_idx ON events.calendar_token (created_by);

-- Per-group opt-in: when present + public, /ics/g/<name> serves with no key.
CREATE TABLE events.group_calendar (
    group_id   UUID PRIMARY KEY REFERENCES platform.group(id),
    public     BOOLEAN NOT NULL DEFAULT false,
    updated_by UUID NOT NULL REFERENCES platform.user(id),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
