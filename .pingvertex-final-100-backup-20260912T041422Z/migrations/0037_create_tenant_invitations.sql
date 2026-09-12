CREATE TABLE tenant_invitations (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    email TEXT NOT NULL,
    role TEXT NOT NULL,
    token_prefix TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ NULL,
    accepted_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Not UNIQUE: token_prefix is only 8 random bytes. Invitation
-- acceptance re-verifies the FULL raw token against token_hash after
-- the prefix lookup, so a prefix collision alone can never redeem the
-- wrong invitation.
CREATE INDEX idx_tenant_invitations_token_prefix ON tenant_invitations (token_prefix);
CREATE INDEX idx_tenant_invitations_tenant_id ON tenant_invitations (tenant_id);
