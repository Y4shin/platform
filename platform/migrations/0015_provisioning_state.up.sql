-- @requires platform:0013_user_roles
-- M18 Stage D — provisioning state.
--
-- One row per deployment. Carries the blake3 hash of the last-applied
-- `[provisioning]` block plus the timestamp at which it was applied; the
-- `junius provision apply` CLI (and the M18 auto_apply_on_boot pass)
-- compares the current block's hash against this row and short-circuits
-- when they match, so a server restart with an unchanged config is a
-- single audit event instead of a full diff sweep.
--
-- The `id = 1` singleton convention mirrors `platform.meta_migrations`'s
-- pattern of a tiny bookkeeping row, kept deliberately simple so the
-- apply path's failure modes are obvious.

CREATE TABLE platform.provisioning_state (
    id              INT  PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    last_hash       TEXT NOT NULL,
    applied_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
