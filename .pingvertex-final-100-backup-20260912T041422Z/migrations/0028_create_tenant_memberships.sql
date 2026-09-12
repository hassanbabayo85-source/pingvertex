-- Maps users into tenants with an explicit role string.
-- The PRIMARY KEY enforces that a user can be in a given tenant only once.
CREATE TABLE tenant_memberships (
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    user_id UUID NOT NULL REFERENCES users(id),
    role TEXT NOT NULL,
    invited_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    joined_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, user_id)
);
