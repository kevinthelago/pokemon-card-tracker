-- Base schema shared across all streams.
-- Owner: foundation stream. Reproduced here so this stream can run migrations
-- independently; merge resolves duplicates at the DB layer with IF NOT EXISTS.

CREATE TABLE IF NOT EXISTS workspaces (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT NOT NULL,
    kind       TEXT NOT NULL DEFAULT 'dealer' CHECK (kind IN ('dealer','collector')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS users (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    email        TEXT NOT NULL,
    role         TEXT NOT NULL DEFAULT 'staff' CHECK (role IN ('owner','staff')),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (workspace_id, email)
);

-- Stubs for streams that own these tables (catalogue, pos, reconcile-pos).
-- buyer_hash is a salted SHA-256 of the POS buyer identifier; NULL for cash/anonymous.
CREATE TABLE IF NOT EXISTS transactions (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id         UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    buyer_hash           TEXT,
    occurred_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS transaction_lines (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    transaction_id UUID NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
    printing_id    TEXT,
    quantity       INTEGER NOT NULL DEFAULT 1,
    unit_price_cents BIGINT NOT NULL DEFAULT 0,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_transactions_workspace ON transactions(workspace_id);
CREATE INDEX IF NOT EXISTS idx_transactions_buyer    ON transactions(workspace_id, buyer_hash, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_txn_lines_printing    ON transaction_lines(printing_id);

-- risk-dashboard tables
CREATE TYPE IF NOT EXISTS flag_kind     AS ENUM ('stolen_card', 'scalper', 'counterfeit');
CREATE TYPE IF NOT EXISTS flag_severity AS ENUM ('low', 'medium', 'high', 'critical');
CREATE TYPE IF NOT EXISTS flag_status   AS ENUM ('open', 'reviewed', 'dismissed', 'escalated');
CREATE TYPE IF NOT EXISTS target_type   AS ENUM ('card', 'transaction', 'buyer');

CREATE TABLE IF NOT EXISTS risk_flags (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID          NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    kind         flag_kind     NOT NULL,
    severity     flag_severity NOT NULL,
    status       flag_status   NOT NULL DEFAULT 'open',
    target_type  target_type   NOT NULL,
    target_id    UUID          NOT NULL,
    trigger_info TEXT          NOT NULL,
    evidence     JSONB         NOT NULL DEFAULT '{}',
    created_at   TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ   NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS risk_flags_workspace ON risk_flags(workspace_id);
CREATE INDEX IF NOT EXISTS risk_flags_cursor    ON risk_flags(workspace_id, created_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS audit_log (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id      UUID        NOT NULL REFERENCES users(id),
    entity_type  TEXT        NOT NULL,
    entity_id    UUID        NOT NULL,
    action       TEXT        NOT NULL,
    details      JSONB       NOT NULL DEFAULT '{}',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS notifications (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id      UUID        NOT NULL REFERENCES users(id),
    title        TEXT        NOT NULL,
    body         TEXT        NOT NULL,
    deep_link    TEXT,
    is_read      BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS scalper_tuning_feedback (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID        NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    flag_id      UUID        NOT NULL REFERENCES risk_flags(id),
    dismissed_by UUID        NOT NULL REFERENCES users(id),
    evidence     JSONB       NOT NULL DEFAULT '{}',
    dismissed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
