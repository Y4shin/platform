-- @requires
-- M08: a per-instance access-controlled resource. Ownership + shares live in
-- platform.resource_principal / resource_share; this table holds only the note
-- data. role_hello gets DML via the migration runner's emit_grants step.
CREATE TABLE hello.note (
    id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title   TEXT NOT NULL,
    body    TEXT NOT NULL,
    created TIMESTAMPTZ NOT NULL DEFAULT now()
);
