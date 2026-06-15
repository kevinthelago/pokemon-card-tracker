-- Owned by the reconcile-pos stream.
-- Stubs for tables owned by connect-pos and manage-inventory streams; these
-- will already exist (or be a no-op) once those streams land.

CREATE TABLE IF NOT EXISTS pos_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id),
    provider TEXT NOT NULL,
    display_name TEXT NOT NULL,
    sync_direction TEXT NOT NULL DEFAULT 'pull_only'
        CHECK (sync_direction IN ('pull_only', 'push_only', 'bidirectional')),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    last_synced_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS printings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id),
    card_name TEXT NOT NULL,
    set_code TEXT NOT NULL,
    set_number TEXT NOT NULL,
    quantity INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS pos_product_mappings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id),
    connection_id UUID NOT NULL REFERENCES pos_connections(id) ON DELETE CASCADE,
    pos_sku TEXT NOT NULL,
    printing_id UUID NOT NULL REFERENCES printings(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (connection_id, pos_sku)
);

CREATE TABLE IF NOT EXISTS unmapped_pos_skus (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id),
    connection_id UUID NOT NULL REFERENCES pos_connections(id) ON DELETE CASCADE,
    pos_sku TEXT NOT NULL,
    pos_product_name TEXT,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (connection_id, pos_sku)
);

CREATE TABLE IF NOT EXISTS transactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id),
    connection_id UUID NOT NULL REFERENCES pos_connections(id) ON DELETE CASCADE,
    external_id TEXT NOT NULL,
    transaction_type TEXT NOT NULL CHECK (transaction_type IN ('sale', 'refund', 'return')),
    pos_transaction_date TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (connection_id, external_id)
);

CREATE TABLE IF NOT EXISTS transaction_lines (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    transaction_id UUID NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
    pos_sku TEXT NOT NULL,
    printing_id UUID REFERENCES printings(id),
    pos_product_mapping_id UUID REFERENCES pos_product_mappings(id),
    quantity INTEGER NOT NULL,
    unit_price_cents INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_transaction_lines_printing ON transaction_lines(printing_id);
CREATE INDEX IF NOT EXISTS idx_transaction_lines_sku ON transaction_lines(pos_sku);

CREATE TABLE IF NOT EXISTS reconciliation_reports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id),
    connection_id UUID NOT NULL REFERENCES pos_connections(id) ON DELETE CASCADE,
    report_date DATE NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'syncing', 'completed', 'failed', 'stale')),
    discrepancy_count INTEGER NOT NULL DEFAULT 0,
    unresolved_count INTEGER NOT NULL DEFAULT 0,
    synced_at TIMESTAMPTZ,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (connection_id, report_date)
);

CREATE INDEX IF NOT EXISTS idx_reconciliation_reports_workspace ON reconciliation_reports(workspace_id, report_date DESC);

CREATE TABLE IF NOT EXISTS reconciliation_discrepancies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    report_id UUID NOT NULL REFERENCES reconciliation_reports(id) ON DELETE CASCADE,
    printing_id UUID REFERENCES printings(id),
    pos_sku TEXT NOT NULL,
    discrepancy_type TEXT NOT NULL
        CHECK (discrepancy_type IN ('missing', 'extra', 'quantity_mismatch', 'negative_quantity')),
    catalogue_qty INTEGER,
    pos_qty INTEGER,
    resolution TEXT NOT NULL DEFAULT 'pending'
        CHECK (resolution IN ('pending', 'accept_pos', 'accept_catalogue', 'manual_adjust', 'investigate')),
    resolved_at TIMESTAMPTZ,
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_discrepancies_report ON reconciliation_discrepancies(report_id);
CREATE INDEX IF NOT EXISTS idx_discrepancies_resolution ON reconciliation_discrepancies(resolution) WHERE resolution = 'pending';
