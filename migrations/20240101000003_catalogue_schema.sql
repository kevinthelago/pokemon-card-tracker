-- Catalogue schema: card identity cache, inventory, graded instances, valuations,
-- POS integration tables, reconciliation, fraud detection, notifications, audit log.

-- ── Card identity cache ─────────────────────────────────────────────────────

CREATE TABLE printings (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tcg_api_id      TEXT NOT NULL UNIQUE,
    name            TEXT NOT NULL,
    set_id          TEXT NOT NULL,
    set_name        TEXT NOT NULL,
    number          TEXT NOT NULL,
    variant         TEXT,
    language        TEXT NOT NULL DEFAULT 'en',
    edition         TEXT,
    image_url       TEXT,
    image_url_large TEXT,
    supertype       TEXT,
    rarity          TEXT,
    raw_data        JSONB NOT NULL DEFAULT '{}',
    cached_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX printings_name_fts_idx  ON printings USING gin (to_tsvector('english', name));
CREATE INDEX printings_set_id_idx    ON printings (set_id);
CREATE INDEX printings_cached_at_idx ON printings (cached_at);

CREATE TABLE sealed_products (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    upc          TEXT NOT NULL UNIQUE,
    name         TEXT NOT NULL,
    set_id       TEXT,
    product_type TEXT NOT NULL,
    raw_data     JSONB NOT NULL DEFAULT '{}',
    cached_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Inventory — raw and sealed stock ───────────────────────────────────────

CREATE TABLE inventory_items (
    id                     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id           UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    printing_id            UUID REFERENCES printings(id),
    sealed_product_id      UUID REFERENCES sealed_products(id),
    condition              TEXT,
    quantity               INTEGER NOT NULL DEFAULT 1 CHECK (quantity > 0),
    acquisition_cost_cents INTEGER,
    notes                  TEXT,
    photos                 JSONB NOT NULL DEFAULT '[]',
    created_at             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT has_product CHECK (printing_id IS NOT NULL OR sealed_product_id IS NOT NULL),
    UNIQUE (workspace_id, printing_id, condition)
);

CREATE INDEX inventory_items_workspace_idx ON inventory_items (workspace_id);

-- ── Card instances — graded, unique ────────────────────────────────────────

CREATE TABLE card_instances (
    id                     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id           UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    printing_id            UUID REFERENCES printings(id),
    grader                 TEXT NOT NULL,
    cert_number            TEXT NOT NULL,
    grade                  TEXT,
    grade_raw              NUMERIC(4, 1),
    verification_status    TEXT NOT NULL DEFAULT 'unverified'
                               CHECK (verification_status IN ('verified', 'unverified', 'failed')),
    acquisition_cost_cents INTEGER,
    notes                  TEXT,
    photos                 JSONB NOT NULL DEFAULT '[]',
    created_at             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (workspace_id, grader, cert_number)
);

CREATE INDEX card_instances_workspace_idx ON card_instances (workspace_id);
CREATE INDEX card_instances_cert_idx      ON card_instances (grader, cert_number);

-- ── Grading verifications ──────────────────────────────────────────────────

CREATE TABLE grading_verifications (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    card_instance_id UUID NOT NULL REFERENCES card_instances(id) ON DELETE CASCADE,
    grader           TEXT NOT NULL,
    cert_number      TEXT NOT NULL,
    result_status    TEXT NOT NULL,
    result_grade     TEXT,
    result_card_name TEXT,
    raw_response     JSONB,
    verified_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Valuations ─────────────────────────────────────────────────────────────

CREATE TABLE valuations (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    printing_id       UUID REFERENCES printings(id),
    card_instance_id  UUID REFERENCES card_instances(id),
    inventory_item_id UUID REFERENCES inventory_items(id),
    source            TEXT NOT NULL,
    market_price_cents INTEGER,
    low_price_cents   INTEGER,
    high_price_cents  INTEGER,
    raw_data          JSONB,
    fetched_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE valuation_snapshots (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id),
    total_cents  BIGINT NOT NULL,
    item_count   INTEGER NOT NULL,
    snapped_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── POS integrations ────────────────────────────────────────────────────────

CREATE TABLE pos_connections (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id      UUID NOT NULL REFERENCES workspaces(id),
    provider          TEXT NOT NULL,
    access_token_enc  BYTEA NOT NULL,
    refresh_token_enc BYTEA,
    token_expires_at  TIMESTAMPTZ,
    merchant_id       TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (workspace_id, provider)
);

CREATE TABLE pos_product_mappings (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pos_connection_id UUID NOT NULL REFERENCES pos_connections(id) ON DELETE CASCADE,
    pos_product_id    TEXT NOT NULL,
    printing_id       UUID REFERENCES printings(id),
    sealed_product_id UUID REFERENCES sealed_products(id),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (pos_connection_id, pos_product_id)
);

-- ── Transactions ────────────────────────────────────────────────────────────

CREATE TABLE transactions (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id      UUID NOT NULL REFERENCES workspaces(id),
    pos_connection_id UUID REFERENCES pos_connections(id),
    pos_transaction_id TEXT,
    total_cents       INTEGER NOT NULL,
    status            TEXT NOT NULL DEFAULT 'completed',
    transacted_at     TIMESTAMPTZ NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE transaction_lines (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    transaction_id    UUID NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
    printing_id       UUID REFERENCES printings(id),
    sealed_product_id UUID REFERENCES sealed_products(id),
    card_instance_id  UUID REFERENCES card_instances(id),
    quantity          INTEGER NOT NULL DEFAULT 1,
    unit_price_cents  INTEGER NOT NULL,
    condition         TEXT
);

-- ── Reconciliation ──────────────────────────────────────────────────────────

CREATE TABLE reconciliation_discrepancies (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id   UUID NOT NULL REFERENCES workspaces(id),
    transaction_id UUID REFERENCES transactions(id),
    printing_id    UUID REFERENCES printings(id),
    kind           TEXT NOT NULL,
    detail         JSONB,
    resolved_at    TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Fraud / risk ────────────────────────────────────────────────────────────

CREATE TABLE risk_flags (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id),
    kind         TEXT NOT NULL,
    severity     TEXT NOT NULL DEFAULT 'medium',
    status       TEXT NOT NULL DEFAULT 'open',
    subject_id   UUID,
    subject_type TEXT,
    detail       JSONB,
    resolved_at  TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE stolen_reports (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reporter_id      UUID NOT NULL REFERENCES users(id),
    card_instance_id UUID REFERENCES card_instances(id),
    grader           TEXT,
    cert_number      TEXT,
    description      TEXT NOT NULL,
    status           TEXT NOT NULL DEFAULT 'open',
    resolved_at      TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE buyer_allowlists (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id       UUID NOT NULL REFERENCES workspaces(id),
    buyer_email_hash   TEXT NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (workspace_id, buyer_email_hash)
);

CREATE TABLE detection_configs (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) UNIQUE,
    config       JSONB NOT NULL DEFAULT '{}',
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Notifications ────────────────────────────────────────────────────────────

CREATE TABLE notifications (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      UUID NOT NULL REFERENCES users(id),
    workspace_id UUID REFERENCES workspaces(id),
    kind         TEXT NOT NULL,
    payload      JSONB,
    read_at      TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Audit log ────────────────────────────────────────────────────────────────

CREATE TABLE audit_logs (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID REFERENCES workspaces(id),
    actor_id     UUID REFERENCES users(id),
    action       TEXT NOT NULL,
    subject_id   UUID,
    subject_type TEXT,
    detail       JSONB,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
