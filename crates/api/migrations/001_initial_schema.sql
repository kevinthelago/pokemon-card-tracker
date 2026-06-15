-- Foundation migration — all tables.
-- This file is owned by the foundation stream; the grading_verifications
-- and card_instances tables are included here for completeness.

CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- ── Users & auth ───────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS users (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email             TEXT NOT NULL UNIQUE,
    display_name      TEXT,
    password_hash     TEXT NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Workspaces ──────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS workspaces (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT NOT NULL,
    kind       TEXT NOT NULL CHECK (kind IN ('seller', 'collector')),
    owner_id   UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Card identity cache ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS printings (
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

-- ── Card instances — graded, unique ────────────────────────────────────────
CREATE TABLE IF NOT EXISTS card_instances (
    id                     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id           UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    printing_id            UUID REFERENCES printings(id),
    grader                 TEXT NOT NULL,
    cert_number            TEXT NOT NULL,
    grade                  TEXT,
    -- 'verified' | 'unverified' | 'mismatch' — see VerificationStatus
    verification_status    TEXT NOT NULL DEFAULT 'unverified'
                               CHECK (verification_status IN ('verified', 'unverified', 'mismatch')),
    acquisition_cost_cents INTEGER,
    notes                  TEXT,
    photos                 JSONB NOT NULL DEFAULT '[]',
    created_at             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (workspace_id, grader, cert_number)
);

CREATE INDEX IF NOT EXISTS card_instances_workspace_idx ON card_instances (workspace_id);
CREATE INDEX IF NOT EXISTS card_instances_cert_idx      ON card_instances (grader, cert_number);

-- ── Grading verifications cache ────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS grading_verifications (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Nullable: standalone verifications are not tied to an instance.
    card_instance_id    UUID REFERENCES card_instances(id) ON DELETE SET NULL,
    grader              TEXT NOT NULL,
    cert_number         TEXT NOT NULL,
    -- 'verified' | 'mismatch' | 'not_found' | 'unavailable'
    result_status       TEXT NOT NULL,
    result_grade        TEXT,
    result_card_name    TEXT,
    result_set_name     TEXT,
    result_year         TEXT,
    raw_response        JSONB,
    verified_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS grading_verifications_cert_idx
    ON grading_verifications (grader, cert_number, verified_at DESC);

-- ── Risk flags ──────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS risk_flags (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id  UUID REFERENCES workspaces(id),
    kind          TEXT NOT NULL,           -- 'counterfeit' | 'stolen' | 'scalper'
    severity      TEXT NOT NULL DEFAULT 'medium',
    status        TEXT NOT NULL DEFAULT 'open',
    subject_id    UUID,
    subject_type  TEXT,
    detail        JSONB,
    resolved_at   TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS risk_flags_workspace_idx ON risk_flags (workspace_id, status);
