-- Stolen-card reporting, moderation, and private workspace list.
-- Also stubs out risk_flags, card_instances, and audit_logs for cross-stream use.

-- Community stolen reports (platform-moderated)
CREATE TABLE stolen_reports (
    id               UUID        NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    grader           TEXT        NOT NULL CHECK (grader IN ('PSA', 'CGC', 'BGS')),
    cert_number      TEXT        NOT NULL,
    reporter_user_id UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status           TEXT        NOT NULL DEFAULT 'pending'
                                 CHECK (status IN ('pending', 'confirmed', 'disputed', 'rejected')),
    evidence         TEXT,
    notes            TEXT,
    moderator_notes  TEXT,
    confirmed_at     TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Hot path: cert matching
CREATE INDEX idx_stolen_reports_cert     ON stolen_reports (grader, cert_number);
CREATE INDEX idx_stolen_reports_status   ON stolen_reports (status);
CREATE INDEX idx_stolen_reports_reporter ON stolen_reports (reporter_user_id);
-- Prevent the same user filing duplicate reports for the same cert
CREATE UNIQUE INDEX idx_stolen_reports_no_dupe
    ON stolen_reports (grader, cert_number, reporter_user_id);

-- Disputes filed against a shared-list report
CREATE TABLE dispute_records (
    id               UUID        NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    stolen_report_id UUID        NOT NULL REFERENCES stolen_reports(id) ON DELETE CASCADE,
    filed_by_user_id UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    reason           TEXT        NOT NULL,
    resolution       TEXT,
    resolved_at      TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_dispute_records_report ON dispute_records (stolen_report_id);

-- Per-workspace private stolen list (auto-confirmed, no moderation required)
CREATE TABLE private_stolen_certs (
    id               UUID        NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id     UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    grader           TEXT        NOT NULL CHECK (grader IN ('PSA', 'CGC', 'BGS')),
    cert_number      TEXT        NOT NULL,
    added_by_user_id UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    notes            TEXT,
    resolved_at      TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_private_stolen_cert UNIQUE (workspace_id, grader, cert_number)
);

CREATE INDEX idx_private_stolen_workspace ON private_stolen_certs (workspace_id);

-- Platform moderators — no role column in users table, so separate table
CREATE TABLE platform_moderators (
    user_id            UUID        NOT NULL PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    granted_by_user_id UUID        REFERENCES users(id),
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Stub tables for cross-stream integration.
-- The catalogue and grading streams own these; we create them here so the
-- stolen stream can write risk_flags and query card_instances without a
-- circular migration dependency.

CREATE TABLE IF NOT EXISTS card_instances (
    id           UUID        NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    grader       TEXT        NOT NULL,
    cert_number  TEXT        NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_card_instances_cert ON card_instances (grader, cert_number);

CREATE TABLE IF NOT EXISTS risk_flags (
    id           UUID        NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    kind         TEXT        NOT NULL,
    severity     TEXT        NOT NULL DEFAULT 'high',
    target_type  TEXT        NOT NULL,
    target_id    UUID        NOT NULL,
    status       TEXT        NOT NULL DEFAULT 'open',
    detail       JSONB       NOT NULL DEFAULT '{}',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_risk_flags_workspace ON risk_flags (workspace_id, status);
CREATE INDEX IF NOT EXISTS idx_risk_flags_target    ON risk_flags (target_type, target_id);

CREATE TABLE IF NOT EXISTS audit_logs (
    id            UUID        NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_user_id UUID        REFERENCES users(id),
    action        TEXT        NOT NULL,
    entity_type   TEXT        NOT NULL,
    entity_id     UUID,
    meta          JSONB       NOT NULL DEFAULT '{}',
    at            TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_audit_logs_entity ON audit_logs (entity_type, entity_id);
