-- Background job tracking for CSV import and export

CREATE TABLE import_jobs (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id    UUID NOT NULL REFERENCES workspaces(id),
    status          TEXT NOT NULL DEFAULT 'queued'
                        CHECK (status IN ('queued', 'running', 'done', 'failed')),
    filename        TEXT,
    total_rows      BIGINT,
    processed_rows  BIGINT NOT NULL DEFAULT 0,
    imported_rows   BIGINT NOT NULL DEFAULT 0,
    skipped_rows    BIGINT NOT NULL DEFAULT 0,
    -- JSONB array of {row, field, reason} error objects
    error_report    JSONB,
    error_message   TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at    TIMESTAMPTZ
);

CREATE INDEX idx_import_jobs_workspace ON import_jobs (workspace_id, created_at DESC);

CREATE TABLE export_jobs (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id    UUID NOT NULL REFERENCES workspaces(id),
    status          TEXT NOT NULL DEFAULT 'queued'
                        CHECK (status IN ('queued', 'running', 'done', 'failed')),
    -- Serialised filter criteria (JSONB)
    filters         JSONB NOT NULL DEFAULT '{}',
    row_count       BIGINT,
    -- S3 / local path where the produced file lives
    download_path   TEXT,
    error_message   TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at    TIMESTAMPTZ
);

CREATE INDEX idx_export_jobs_workspace ON export_jobs (workspace_id, created_at DESC);

-- apalis job queue table (Postgres storage backend)
CREATE TABLE apalis_jobs (
    id          TEXT PRIMARY KEY,
    job         JSONB NOT NULL,
    status      TEXT NOT NULL DEFAULT 'pending',
    attempts    INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    scheduled_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    done_at     TIMESTAMPTZ,
    lock_at     TIMESTAMPTZ,
    priority    INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_apalis_jobs_status ON apalis_jobs (status, scheduled_at);
