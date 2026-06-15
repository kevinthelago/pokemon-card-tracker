-- Printings: card reference data from the Pokémon TCG API.
-- Owned by catalogue-a-card; valuation depends on printings.tcg_prices_json.
CREATE TABLE IF NOT EXISTS printings (
    id               TEXT PRIMARY KEY,           -- Pokémon TCG API card id (e.g. "xy1-1")
    name             TEXT NOT NULL,
    set_code         TEXT NOT NULL,
    collector_number TEXT NOT NULL,
    rarity           TEXT,
    variant          TEXT,
    language         TEXT NOT NULL DEFAULT 'en',
    edition          TEXT,
    image_url        TEXT,
    tcg_prices_json  JSONB,                      -- TCGplayer embedded price blob
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_printings_set ON printings (set_code);

-- Inventory items: quantity-tracked raw/sealed stock per workspace.
CREATE TABLE IF NOT EXISTS inventory_items (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id     UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    printing_id      TEXT NOT NULL REFERENCES printings(id),
    condition        TEXT NOT NULL,
    quantity         INT NOT NULL DEFAULT 1 CHECK (quantity >= 0),
    acquisition_cost NUMERIC(10,2),
    notes            TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (workspace_id, printing_id, condition)
);
CREATE INDEX IF NOT EXISTS idx_inventory_workspace ON inventory_items (workspace_id);

-- Card instances: uniquely-tracked graded cards per workspace.
CREATE TABLE IF NOT EXISTS card_instances (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id        UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    printing_id         TEXT NOT NULL REFERENCES printings(id),
    grader              TEXT NOT NULL,   -- 'PSA' | 'CGC' | 'BGS'
    cert_number         TEXT NOT NULL,
    grade               TEXT NOT NULL,
    verification_status TEXT NOT NULL DEFAULT 'unverified',
    acquisition_cost    NUMERIC(10,2),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (grader, cert_number)
);
CREATE INDEX IF NOT EXISTS idx_card_instances_workspace ON card_instances (workspace_id);

-- Valuations: cached latest market price per printing (one row per printing).
CREATE TABLE IF NOT EXISTS valuations (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    printing_id TEXT NOT NULL REFERENCES printings(id),
    source      TEXT NOT NULL,           -- 'tcgplayer' | 'pricecharting'
    price       NUMERIC(12,4) NOT NULL,
    currency    TEXT NOT NULL DEFAULT 'USD',
    fetched_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (printing_id)
);

-- Valuation snapshots: historical price data points per printing.
CREATE TABLE IF NOT EXISTS valuation_snapshots (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    printing_id TEXT NOT NULL REFERENCES printings(id),
    price       NUMERIC(12,4) NOT NULL,
    currency    TEXT NOT NULL DEFAULT 'USD',
    source      TEXT NOT NULL,
    captured_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_snapshots_printing_time
    ON valuation_snapshots (printing_id, captured_at DESC);
