-- Grading verification cache and counterfeit risk flags
-- Owned by: verify-graded-card stream

CREATE TABLE grading_verifications (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- card_instance_id FK added by catalogue-a-card migration when that table exists
    card_instance_id UUID,
    grader           TEXT NOT NULL,
    cert_number      TEXT NOT NULL,
    result_status    TEXT NOT NULL CHECK (result_status IN ('verified', 'mismatch', 'not_found', 'unavailable')),
    result_grade     TEXT,
    result_card_name TEXT,
    result_set_name  TEXT,
    result_year      TEXT,
    raw_response     JSONB,
    verified_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX grading_verifications_lookup
    ON grading_verifications (grader, cert_number, verified_at DESC);

CREATE TABLE risk_flags (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID REFERENCES workspaces(id) ON DELETE SET NULL,
    kind         TEXT NOT NULL CHECK (kind IN ('counterfeit', 'stolen', 'fraud')),
    severity     TEXT NOT NULL CHECK (severity IN ('low', 'medium', 'high')),
    status       TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'resolved', 'dismissed')),
    subject_id   UUID NOT NULL,
    subject_type TEXT NOT NULL,
    detail       JSONB,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX risk_flags_workspace ON risk_flags (workspace_id, status);
