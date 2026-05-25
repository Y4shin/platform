-- @requires hello:0003_greeting_template
-- M09 cross-plugin demo. `template_id` is a NOT NULL FK into hello's exposed
-- `greeting_template` table (a required dependency) — no ON DELETE CASCADE
-- across the plugin boundary. `venue` is a nullable free-text column (the
-- optional widgets dep only contributes the picker UI, not a table). role_greetings
-- gets SELECT on hello.greeting_template from the migration runner's grants step.
CREATE SCHEMA IF NOT EXISTS greetings;

CREATE TABLE greetings.greeting (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    template_id UUID NOT NULL REFERENCES hello.greeting_template (id),
    recipient   TEXT NOT NULL,
    venue       TEXT,
    created     TIMESTAMPTZ NOT NULL DEFAULT now()
);
