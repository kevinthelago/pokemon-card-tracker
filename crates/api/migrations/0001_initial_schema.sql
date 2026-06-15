-- Core schema for CardGuard

CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Users & auth
CREATE TABLE users (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email       TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Workspaces (seller or collector)
CREATE TABLE workspaces (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name            TEXT NOT NULL,
    kind            TEXT NOT NULL CHECK (kind IN ('seller', 'collector')),
    owner_user_id   UUID NOT NULL REFERENCES users(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE memberships (
    user_id         UUID NOT NULL REFERENCES users(id),
    workspace_id    UUID NOT NULL REFERENCES workspaces(id),
    role            TEXT NOT NULL CHECK (role IN ('owner', 'staff')),
    PRIMARY KEY (user_id, workspace_id)
);

-- Card identity (shared reference data from Pokémon TCG API)
CREATE TABLE printings (
    id               TEXT PRIMARY KEY,  -- TCG API id e.g. "base1-4"
    set_code         TEXT NOT NULL,
    collector_number TEXT NOT NULL,
    name             TEXT NOT NULL,
    rarity           TEXT NOT NULL,
    variant          TEXT,
    finish           TEXT,
    language         TEXT NOT NULL DEFAULT 'EN',
    edition          TEXT,
    image_url        TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_printings_set_number ON printings (set_code, collector_number);
CREATE INDEX idx_printings_name ON printings (name);

-- Raw/sealed stock (quantity-tracked)
CREATE TABLE inventory_items (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id         UUID NOT NULL REFERENCES workspaces(id),
    printing_id          TEXT NOT NULL REFERENCES printings(id),
    condition            TEXT NOT NULL CHECK (condition IN (
                             'MINT', 'NEAR_MINT', 'LIGHTLY_PLAYED',
                             'MODERATELY_PLAYED', 'HEAVILY_PLAYED', 'DAMAGED')),
    quantity             INTEGER NOT NULL DEFAULT 1 CHECK (quantity >= 0),
    acquisition_cost_cents BIGINT,
    notes                TEXT,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, printing_id, condition)
);

CREATE INDEX idx_inventory_items_workspace ON inventory_items (workspace_id);
CREATE INDEX idx_inventory_items_printing ON inventory_items (printing_id);

-- Graded card instances (uniquely tracked)
CREATE TABLE card_instances (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id         UUID NOT NULL REFERENCES workspaces(id),
    printing_id          TEXT NOT NULL REFERENCES printings(id),
    grader               TEXT NOT NULL CHECK (grader IN ('PSA', 'CGC', 'BGS')),
    cert_number          TEXT NOT NULL,
    grade                TEXT NOT NULL,
    verification_status  TEXT NOT NULL DEFAULT 'unverified'
                             CHECK (verification_status IN ('unverified', 'verified', 'mismatch', 'failed')),
    acquisition_cost_cents BIGINT,
    notes                TEXT,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (grader, cert_number)
);

CREATE INDEX idx_card_instances_workspace ON card_instances (workspace_id);
CREATE INDEX idx_card_instances_printing ON card_instances (printing_id);

-- Market value cache
CREATE TABLE valuations (
    printing_id     TEXT NOT NULL REFERENCES printings(id),
    source          TEXT NOT NULL CHECK (source IN ('tcgplayer', 'pricecharting')),
    condition       TEXT,
    price_cents     BIGINT NOT NULL,
    currency        TEXT NOT NULL DEFAULT 'USD',
    fetched_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (printing_id, source, COALESCE(condition, ''))
);

-- Grader cert verification cache
CREATE TABLE grading_verifications (
    grader          TEXT NOT NULL,
    cert_number     TEXT NOT NULL,
    result          JSONB NOT NULL,
    verified_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (grader, cert_number)
);
