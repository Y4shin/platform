-- Host migration: session telemetry for the /me profile page (M19, slice #3).
-- `user_agent` records the browser that created the session (captured at login)
-- so a user can recognise a session in the revoke list; `last_seen` is stamped
-- by the session middleware on each validated request so the list shows recency.
-- Existing rows default `last_seen` to now() (their next request re-stamps it).

ALTER TABLE platform.session
    ADD COLUMN user_agent TEXT,
    ADD COLUMN last_seen  TIMESTAMPTZ NOT NULL DEFAULT now();
