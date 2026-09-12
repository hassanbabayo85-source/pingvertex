-- Tenants are the top-level isolation boundary for multi-user resources.
-- Every existing user will receive exactly one tenant during the backfill
-- in 0030; new tenants are created explicitly by the application layer.
CREATE TABLE tenants (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    owner_user_id UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
