-- @requires events:0001_event
-- One invite page per event (v1): a shareable, unguessable `slug` URL that
-- optionally collects sign-ups, with per-field visibility toggles, a manual
-- open/close, and an optional slot limit. The intra-plugin cascade stays inside
-- `events` (satisfies junius check's FK.CROSS.CASCADE).
CREATE TABLE events.invite (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id         UUID NOT NULL REFERENCES events.event(id) ON DELETE CASCADE,
    slug             TEXT NOT NULL UNIQUE,            -- unguessable token; the public URL
    signup_enabled   BOOLEAN NOT NULL DEFAULT true,   -- does this invite collect sign-ups at all?
    signup_open      BOOLEAN NOT NULL DEFAULT true,   -- manual open/close (independent of slots)
    slot_limit       INTEGER,                         -- NULL = unlimited
    -- Per-field visibility toggles for the public page.
    show_title       BOOLEAN NOT NULL DEFAULT true,
    show_datetime    BOOLEAN NOT NULL DEFAULT true,
    show_location    BOOLEAN NOT NULL DEFAULT true,
    show_description BOOLEAN NOT NULL DEFAULT true,
    show_remaining   BOOLEAN NOT NULL DEFAULT true,   -- show remaining slots
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (event_id),                                -- one invite per event (v1)
    CONSTRAINT invite_slug_nonempty  CHECK (length(slug) > 0),
    CONSTRAINT invite_slot_limit_pos CHECK (slot_limit IS NULL OR slot_limit > 0)
);
