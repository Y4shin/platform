-- @requires events:0002_invite
-- @requires platform:0001_users
-- Sign-ups against an invite. A sign-up is either a logged-in 'user' (carrying a
-- user_id, deduped) or an anonymous 'guest' (carrying name + email); the CHECK
-- forbids half-filled or mixed rows. The cross-schema FK into platform.user is
-- nullable and non-cascading (RESTRICT) — only the intra-plugin invite FK
-- cascades.
CREATE TYPE events.signup_kind AS ENUM ('user', 'guest');
CREATE TYPE events.signup_status AS ENUM ('going', 'opted_out');

CREATE TABLE events.signup (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invite_id   UUID NOT NULL REFERENCES events.invite(id) ON DELETE CASCADE,
    kind        events.signup_kind NOT NULL,
    user_id     UUID REFERENCES platform.user(id),
    guest_name  TEXT,
    guest_email TEXT,
    status      events.signup_status NOT NULL DEFAULT 'going',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- 'user' sign-ups carry a user_id and no guest fields; 'guest' sign-ups carry
    -- both guest fields and no user_id. Mutually exclusive; no half-filled guests.
    CONSTRAINT signup_identity CHECK (
        (kind = 'user'  AND user_id IS NOT NULL
                        AND guest_name IS NULL AND guest_email IS NULL)
     OR (kind = 'guest' AND user_id IS NULL
                        AND guest_name  IS NOT NULL AND length(btrim(guest_name)) > 0
                        AND guest_email IS NOT NULL AND guest_email LIKE '%_@_%')
    )
);

-- One sign-up per logged-in user per invite; guests are not deduped.
CREATE UNIQUE INDEX signup_one_per_user
    ON events.signup (invite_id, user_id)
    WHERE kind = 'user';
