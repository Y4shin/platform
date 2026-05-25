-- Host migration: observability for the RabbitMQ-backed job queue. RabbitMQ owns
-- the actual queue + delivery; this table is the durable record of each enqueue
-- and its lifecycle (enqueued → running → completed | failed), updated by the
-- enqueue path and the in-process worker. Pruned by the M10 audit-retention job.

CREATE TABLE meta.job_run (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_name     TEXT NOT NULL,                       -- '<plugin>.<job>' (routing key)
    plugin       TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'enqueued',    -- enqueued|running|completed|failed
    attempt      INT  NOT NULL DEFAULT 0,
    payload      JSONB NOT NULL,
    error        TEXT,
    enqueued_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at   TIMESTAMPTZ,
    finished_at  TIMESTAMPTZ
);

CREATE INDEX job_run_status_idx      ON meta.job_run(status);
CREATE INDEX job_run_job_name_idx    ON meta.job_run(job_name);
CREATE INDEX job_run_enqueued_at_idx ON meta.job_run(enqueued_at DESC);
