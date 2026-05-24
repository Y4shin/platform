-- Host migration: the host-wide audit log. Plugins write here via
-- PluginResources.audit.emit (M06 stage B). Retention is NOT enforced at the DB
-- level in v1 — a nightly prune job lands in M10.

CREATE TABLE platform.audit_event (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_kind      TEXT NOT NULL,                   -- '<plugin>:<resource>.<verb>'
    actor_user_id   UUID REFERENCES platform.user(id),
    resource_kind   TEXT,                            -- '<plugin>:<table>'
    resource_id     UUID,
    details         JSONB NOT NULL DEFAULT '{}'::jsonb,
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX audit_event_actor_idx      ON platform.audit_event(actor_user_id);
CREATE INDEX audit_event_resource_idx   ON platform.audit_event(resource_kind, resource_id);
CREATE INDEX audit_event_occurred_idx   ON platform.audit_event(occurred_at DESC);
