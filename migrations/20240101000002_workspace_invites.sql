-- Invite-teammates stream: workspace_invites

CREATE TABLE workspace_invites (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    email        TEXT NOT NULL,
    role         member_role NOT NULL,
    token        TEXT NOT NULL UNIQUE,
    invited_by   UUID NOT NULL REFERENCES users(id),
    expires_at   TIMESTAMPTZ NOT NULL DEFAULT (NOW() + INTERVAL '7 days'),
    accepted_at  TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ,
    resent_at    TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX ON workspace_invites (token);
CREATE INDEX ON workspace_invites (workspace_id, email) WHERE accepted_at IS NULL AND cancelled_at IS NULL;
