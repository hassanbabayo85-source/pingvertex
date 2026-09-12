-- Add an index on user_id to support fast lookups of all tenants a user
-- belongs to. Used by current-tenant resolution in jwt_auth (list_by_user)
-- and by the upcoming invitation flow (B9).
CREATE INDEX idx_tenant_memberships_user_id ON tenant_memberships (user_id);
