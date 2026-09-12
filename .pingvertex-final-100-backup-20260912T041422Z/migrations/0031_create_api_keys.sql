-- Feature 10: API Keys & Scoped Permissions.
--
-- Tenant-wide (Model A): ownership is tracked via `user_id` purely as an
-- audit trail of who created the key (see `ApiKey`'s doc comment in
-- domain), NOT as an access-control gate — every application-layer query
-- against this table filters by `tenant_id`, never by `user_id`.
CREATE TABLE api_keys (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,

    -- Audit trail only (who created this key) — NOT used for access
    -- control. See the table-level comment above.
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    name TEXT NOT NULL,

    -- Plaintext lookup portion of the raw key (e.g. "pv_3f9a2c1e...").
    -- Argon2 hashes are salted and therefore cannot be looked up by
    -- equality, so `ApiKeyAuthMiddleware` finds the row via this indexed,
    -- unique, non-secret prefix first, then verifies the caller's FULL raw
    -- key against `hashed_key` below.
    key_prefix TEXT NOT NULL,

    -- Argon2 hash (via PasswordHasher/Argon2PasswordHasher) of the full
    -- raw key. The raw key itself is never stored anywhere and is shown
    -- to the caller exactly once, at creation time.
    hashed_key TEXT NOT NULL,

    -- Scope strings, e.g. 'monitor:read', 'monitor:write' — see
    -- `domain::enums::ApiKeyScope`. Stored as TEXT[] rather than a join
    -- table since scopes are small, fixed-cardinality, and always read/
    -- written together with the rest of the key.
    scopes TEXT[] NOT NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- NULL means the key never expires.
    expires_at TIMESTAMPTZ,

    -- Updated best-effort by `ApiKeyAuthMiddleware` on every successful
    -- authentication. NULL until first use.
    last_used_at TIMESTAMPTZ,

    CONSTRAINT chk_api_keys_scopes_not_empty CHECK (array_length(scopes, 1) > 0)
);

-- Enforces global prefix uniqueness (required — this is the column
-- `find_by_prefix` looks up on) and doubles as the lookup index itself.
CREATE UNIQUE INDEX idx_api_keys_key_prefix ON api_keys (key_prefix);

-- Used by `ListApiKeysUseCase` (`list_by_tenant`) and by
-- `DeleteApiKeyUseCase`'s same-tenant authorization check.
CREATE INDEX idx_api_keys_tenant_id ON api_keys (tenant_id);
