-- Host migration: the platform schema + the user identity table.
-- gen_random_uuid() is built into PostgreSQL 13+ core; no extension needed.

CREATE SCHEMA platform;

CREATE TABLE platform.user (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    oidc_sub      TEXT NOT NULL UNIQUE,         -- Authentik subject
    email         TEXT NOT NULL,
    display_name  TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
