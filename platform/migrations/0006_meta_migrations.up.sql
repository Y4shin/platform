-- Host migration: the migration bookkeeping table. The runner BOOTSTRAPS this
-- one first (before any other migration) when meta.migrations is absent, then
-- records it like any other migration. id BIGSERIAL gives true apply-order;
-- checksum is the SHA-256 of the .up.sql contents so post-apply edits are caught.

CREATE SCHEMA meta;

CREATE TABLE meta.migrations (
    id              BIGSERIAL    PRIMARY KEY,
    plugin          TEXT         NOT NULL,               -- 'platform' for host migrations
    migration_name  TEXT         NOT NULL,               -- e.g. '0007_audit_event'
    checksum        TEXT         NOT NULL,               -- SHA-256 of .up.sql contents
    applied_at      TIMESTAMPTZ  NOT NULL DEFAULT now(),
    UNIQUE (plugin, migration_name)
);
