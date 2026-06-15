-- risk-dashboard stream: risk_flags, audit_log, notifications

CREATE TYPE flag_kind AS ENUM ('stolen_card', 'scalper', 'counterfeit');
CREATE TYPE flag_severity AS ENUM ('low', 'medium', 'high', 'critical');
CREATE TYPE flag_status AS ENUM ('open', 'reviewed', 'dismissed');
CREATE TYPE target_type AS ENUM ('card', 'transaction', 'buyer');

CREATE TABLE risk_flags (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id  UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    kind          flag_kind NOT NULL,
    severity      flag_severity NOT NULL,
    status        flag_status NOT NULL DEFAULT 'open',
    target_type   target_type NOT NULL,
    target_id     UUID NOT NULL,
    title         TEXT NOT NULL,
    evidence      JSONB NOT NULL DEFAULT '{}',
    reviewed_by   UUID REFERENCES users(id),
    reviewed_at   TIMESTAMPTZ,
    dismissed_by  UUID REFERENCES users(id),
    dismissed_at  TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX ON risk_flags (workspace_id, status, created_at DESC);
CREATE INDEX ON risk_flags (workspace_id, kind, status);
CREATE INDEX ON risk_flags (target_type, target_id);

CREATE TABLE audit_log (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id  UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    actor_id      UUID NOT NULL REFERENCES users(id),
    action        TEXT NOT NULL,
    entity_type   TEXT NOT NULL,
    entity_id     UUID NOT NULL,
    meta          JSONB NOT NULL DEFAULT '{}',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX ON audit_log (workspace_id, entity_id);

CREATE TYPE notification_kind AS ENUM ('new_risk_flag', 'flag_escalated');

CREATE TABLE notifications (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id  UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id       UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind          notification_kind NOT NULL,
    title         TEXT NOT NULL,
    body          TEXT NOT NULL,
    deep_link     TEXT,
    read_at       TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX ON notifications (user_id, read_at) WHERE read_at IS NULL;
CREATE INDEX ON notifications (workspace_id, created_at DESC);
