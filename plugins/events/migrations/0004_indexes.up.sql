-- @requires events:0003_signup
-- Sign-up lookup + slot-count index. (The event indexes — owner/visibility/
-- starts_at — already ship in 0001_event.up.sql.)
CREATE INDEX signup_invite_idx ON events.signup (invite_id);
