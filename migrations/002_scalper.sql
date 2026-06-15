-- Owned by the detect-scalpers stream.
-- Per-workspace scalper detection configuration and buyer allowlist.

CREATE TABLE IF NOT EXISTS scalper_detection_config (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id            UUID    NOT NULL UNIQUE REFERENCES workspaces(id) ON DELETE CASCADE,
    enabled                 BOOL    NOT NULL DEFAULT true,
    -- Velocity: flag a buyer who makes more than this many purchases within the window.
    velocity_window_hours   INTEGER NOT NULL DEFAULT 24,
    velocity_threshold      INTEGER NOT NULL DEFAULT 5,
    -- Bulk: flag a single transaction that contains more than this many units of one item.
    bulk_single_item_limit  INTEGER NOT NULL DEFAULT 3,
    -- Sweep: flag a buyer who buys more than this many units of a single printing in total.
    sweep_printing_limit    INTEGER NOT NULL DEFAULT 10,
    -- Repeat: flag a buyer who makes a second purchase within this many minutes.
    repeat_window_minutes   INTEGER NOT NULL DEFAULT 30,
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS buyer_allowlist (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    buyer_hash   TEXT NOT NULL,
    notes        TEXT,
    added_by     UUID NOT NULL REFERENCES users(id),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (workspace_id, buyer_hash)
);

CREATE INDEX IF NOT EXISTS idx_buyer_allowlist_workspace ON buyer_allowlist(workspace_id);
