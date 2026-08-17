CREATE TABLE IF NOT EXISTS status_pages (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    name TEXT NOT NULL,

    -- The `<subdomain>.pingvertex.com` slug. Validated by the domain-layer
    -- `Subdomain` value object (DNS label rules, reserved-word denylist)
    -- before ever reaching this column; UNIQUE here is the database-level
    -- backstop against a race between two concurrent creates picking the
    -- same subdomain.
    subdomain TEXT NOT NULL,

    -- Custom domain + its DNS TXT-record ownership verification state.
    -- Exactly one of these states must hold at any time (enforced below):
    --   1. No custom domain requested:  all three columns NULL.
    --   2. Verification pending:        custom_domain + custom_domain_token
    --                                    set, custom_domain_verified_at NULL.
    --   3. Verified:                    custom_domain +
    --                                    custom_domain_verified_at set,
    --                                    custom_domain_token NULL.
    --
    -- SECURITY: application/infra code performing host-header routing
    -- MUST filter on `custom_domain_verified_at IS NOT NULL` — see
    -- idx_status_pages_verified_custom_domain below, which is the only
    -- index that exists for this purpose, specifically to make an
    -- "accidentally query the wrong column" mistake harder to write.
    custom_domain TEXT,
    custom_domain_token TEXT,
    custom_domain_requested_at TIMESTAMPTZ,
    custom_domain_verified_at TIMESTAMPTZ,

    logo_url TEXT,

    -- Stored as `#RRGGBB`, validated by the domain-layer `HexColor` value
    -- object before reaching this column.
    primary_color TEXT,

    -- Ordered list of monitor UUIDs to display on this page. Bounded to
    -- MAX_STATUS_PAGE_MONITORS (50) entries by the domain layer before
    -- persistence; the CHECK constraint below is the database-level
    -- backstop against that bound being bypassed.
    monitor_ids JSONB NOT NULL DEFAULT '[]'::jsonb,

    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,

    CONSTRAINT chk_status_pages_monitor_ids_max_50
        CHECK (jsonb_array_length(monitor_ids) <= 50),

    CONSTRAINT chk_status_pages_custom_domain_state
        CHECK (
            -- No custom domain at all.
            (custom_domain IS NULL AND custom_domain_token IS NULL AND custom_domain_requested_at IS NULL AND custom_domain_verified_at IS NULL)
            -- Pending verification: domain + token + requested_at set, not yet verified.
            OR (custom_domain IS NOT NULL AND custom_domain_token IS NOT NULL AND custom_domain_requested_at IS NOT NULL AND custom_domain_verified_at IS NULL)
            -- Verified: domain + verified_at set, token/requested_at cleared.
            OR (custom_domain IS NOT NULL AND custom_domain_token IS NULL AND custom_domain_requested_at IS NULL AND custom_domain_verified_at IS NOT NULL)
        )
);

-- Enforces subdomain uniqueness across ALL status pages (not scoped per
-- user) — `acme.pingvertex.com` can only ever belong to one page,
-- platform-wide. This is also the index the public `GET /status/:subdomain`
-- endpoint's lookup relies on.
CREATE UNIQUE INDEX IF NOT EXISTS idx_status_pages_subdomain
    ON status_pages (subdomain);

-- Enforces custom-domain uniqueness across ALL status pages, but only
-- while a domain is actually verified — a still-`Pending` domain string
-- is not reserved and does not block another user's (also pending) claim
-- on the same string, since ownership hasn't been established for either
-- yet. This is also the ONLY index usable for the host-header routing
-- lookup, which is deliberately restricted to verified domains only (see
-- the SECURITY note on `custom_domain_verified_at` above).
CREATE UNIQUE INDEX IF NOT EXISTS idx_status_pages_verified_custom_domain
    ON status_pages (custom_domain)
    WHERE custom_domain_verified_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_status_pages_user_id
    ON status_pages (user_id);
