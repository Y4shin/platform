-- Host migration: server-side sessions. The cookie carries only the session id;
-- oidc_tokens holds the encrypted OIDC tokens (see platform/src/auth, M06 stage B).

CREATE TABLE platform.session (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       UUID NOT NULL REFERENCES platform.user(id) ON DELETE CASCADE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at    TIMESTAMPTZ NOT NULL,
    oidc_tokens   BYTEA                          -- encrypted, optional
);
CREATE INDEX session_user_idx    ON platform.session(user_id);
CREATE INDEX session_expires_idx ON platform.session(expires_at);
