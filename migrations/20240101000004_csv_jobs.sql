-- CSV import / export job tracking tables
-- Depends on 20240101000003_catalogue_schema.sql (workspaces, printings, inventory_items, card_instances)

CREATE TABLE import_jobs (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id    UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    status          TEXT NOT NULL DEFAULT 'queued'
                        CHECK (status IN ('queued', 'running', 'done', 'failed')),
    filename        TEXT,
    total_rows      BIGINT,
    processed_rows  BIGINT NOT NULL DEFAULT 0,
    imported_rows   BIGINT NOT NULL DEFAULT 0,
    skipped_rows    BIGINT NOT NULL DEFAULT 0,
    error_report    JSONB,
    error_message   TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at    TIMESTAMPTZ
);

CREATE INDEX import_jobs_workspace_idx ON import_jobs (workspace_id, created_at DESC);

CREATE TABLE export_jobs (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id    UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    status          TEXT NOT NULL DEFAULT 'queued'
                        CHECK (status IN ('queued', 'running', 'done', 'failed')),
    filters         JSONB NOT NULL DEFAULT '{}',
    row_count       BIGINT,
    download_path   TEXT,
    error_message   TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at    TIMESTAMPTZ
);

CREATE INDEX export_jobs_workspace_idx ON export_jobs (workspace_id, created_at DESC);
