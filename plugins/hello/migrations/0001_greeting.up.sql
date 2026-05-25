-- @requires
-- The hello plugin's own schema + table. Runs as platform_migrator (which can
-- create schemas); role_hello is later granted DML on this schema by the
-- migration runner's emit_grants step (see crates/manifest grants).
CREATE SCHEMA IF NOT EXISTS hello;

CREATE TABLE hello.greeting (
    id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name    TEXT NOT NULL,
    body    TEXT NOT NULL,
    created TIMESTAMPTZ NOT NULL DEFAULT now()
);
