-- ============================================================================
-- PART 5 — AGENT TRUST BOUNDARY: server-side agent registry
-- ============================================================================
--
-- Prior to this migration, the ONLY authoritative identity an agent had
-- was the CN in its client certificate; its region was read from the
-- client-supplied `X-Agent-Region` header. An agent (or anyone who
-- obtained a valid client certificate) could therefore claim ANY region
-- by simply setting that header, which is the security-hole PART 5
-- closes.
--
-- This table makes the mapping `agent_id -> region` a first-class,
-- server-side fact. The middleware refuses any request whose
-- `X-Agent-Region` header does not EXACTLY match the registered region.
-- Identity is now:
--
--   certificate CN == registered agent_id == path {agent_id}
--   registered region == X-Agent-Region (if header present)
--
-- Invariant preserved:
--
--   An agent must NEVER be able to choose the security-sensitive region
--   it claims to represent.
-- ============================================================================

CREATE TABLE IF NOT EXISTS agents (
    agent_id TEXT PRIMARY KEY,
    region TEXT NOT NULL,
    capabilities JSONB NOT NULL DEFAULT '[]'::jsonb,
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'revoked')),
    certificate_fingerprint TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_agents_region ON agents (region);
CREATE INDEX IF NOT EXISTS idx_agents_status
    ON agents (status)
    WHERE status = 'active';

-- Seed the existing deployment's one known agent so the migration is
-- non-breaking for a live install. New agents are registered by the
-- operator via SQL or an admin tool.
INSERT INTO agents (agent_id, region, capabilities)
VALUES ('agent-us-east-1', 'us-east', '["http_check"]'::jsonb)
ON CONFLICT (agent_id) DO NOTHING;

-- The agent_id must never be rewritten — it is the identity anchor
-- everything else (cert CN, path segment, DB region) hangs off of.
CREATE OR REPLACE FUNCTION prevent_agent_id_change()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.agent_id <> OLD.agent_id THEN
        RAISE EXCEPTION 'agents.agent_id is immutable';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_agents_id_immutable ON agents;
CREATE TRIGGER trg_agents_id_immutable
BEFORE UPDATE ON agents
FOR EACH ROW EXECUTE FUNCTION prevent_agent_id_change();

-- A revoked agent must never be reactivated by a simple UPDATE. If an
-- operator needs to re-enable an agent they must delete and re-register
-- (with a fresh certificate). This is intentional: reactivating a
-- revoked identity silently would defeat the whole point of revocation.
CREATE OR REPLACE FUNCTION prevent_agent_reactivation()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.status = 'revoked' AND NEW.status <> 'revoked' THEN
        RAISE EXCEPTION 'agents: revoked status is terminal; re-register instead';
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_agents_revoked_terminal ON agents;
CREATE TRIGGER trg_agents_revoked_terminal
BEFORE UPDATE ON agents
FOR EACH ROW EXECUTE FUNCTION prevent_agent_reactivation();
