-- RUM (Real User Monitoring) foundation.
--
-- Every table below carries `tenant_id` directly (denormalized from
-- `rum_sites`, not joined) so that every ingestion/dashboard query and
-- every row-level check can filter on tenant_id without a join, matching
-- the pattern used elsewhere in this schema for high-volume tables
-- (see `checks`). This is a deliberate defense-in-depth choice: even if
-- an application-layer bug fetched the wrong `site_id`, the tenant_id
-- column is still there as a second, independent filter.
--
-- `rum_sites.site_key` is the PUBLIC, non-secret identifier handed to the
-- browser SDK (Task D) to tag outgoing events. It must never be treated
-- as a credential: it identifies *which* site the data belongs to, not
-- *who* is allowed to see it. Nothing in this schema stores a secret
-- alongside it.

CREATE TABLE IF NOT EXISTS rum_sites (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,

    -- Public, non-secret identifier sent by the browser SDK on every
    -- ingested event. Safe to embed in page source.
    site_key TEXT NOT NULL,

    name TEXT NOT NULL,

    -- Registrable domain the SDK is expected to run on (e.g.
    -- "app.example.com"). Used by the ingestion handler as a sanity/
    -- allow-list check on the Origin/Referer of incoming beacons; it is
    -- not a security boundary by itself (Origin headers are
    -- client-supplied) but narrows accidental cross-site ingestion.
    domain TEXT NOT NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_rum_sites_site_key
    ON rum_sites (site_key);

CREATE INDEX IF NOT EXISTS idx_rum_sites_tenant_id
    ON rum_sites (tenant_id);

-- Sessions: one row per browser tab-session the SDK observes. Deliberately
-- thin — no cookies, no IP address, no user identity. `session_key` is a
-- random, client-generated, non-PII value used only to group page views
-- and errors together for a single visit.
CREATE TABLE IF NOT EXISTS rum_sessions (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id UUID NOT NULL REFERENCES rum_sites(id) ON DELETE CASCADE,

    session_key TEXT NOT NULL,

    browser_name TEXT,
    browser_version TEXT,
    os_name TEXT,
    device_type TEXT, -- 'desktop' | 'mobile' | 'tablet' | 'other'

    -- Optional coarse geography. Populated only if the existing
    -- geo-IP infrastructure (see data/geoip/) is wired in by the
    -- ingestion handler (Task C) — country-level only, never city/lat-lon,
    -- and never derived from anything the browser sends directly.
    country_code CHAR(2),

    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_rum_sessions_tenant_site
    ON rum_sessions (tenant_id, site_id);

CREATE INDEX IF NOT EXISTS idx_rum_sessions_site_key
    ON rum_sessions (site_id, session_key);

CREATE INDEX IF NOT EXISTS idx_rum_sessions_started_at
    ON rum_sessions (site_id, started_at);

-- Page views + performance timing + Core Web Vitals for that view.
-- One row per page load/navigation observed by the SDK.
CREATE TABLE IF NOT EXISTS rum_page_views (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id UUID NOT NULL REFERENCES rum_sites(id) ON DELETE CASCADE,
    session_id UUID NOT NULL REFERENCES rum_sessions(id) ON DELETE CASCADE,

    -- Path only (no scheme/host/query string) — see Task D: the SDK
    -- strips query strings before sending, since they can carry secrets
    -- or PII (tokens, emails, etc. are a common query-param pattern).
    path TEXT NOT NULL,
    referrer_path TEXT,

    ttfb_ms INTEGER,
    fcp_ms INTEGER,
    lcp_ms INTEGER,
    inp_ms INTEGER,
    cls DOUBLE PRECISION,
    load_time_ms INTEGER,

    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_rum_page_views_tenant_site
    ON rum_page_views (tenant_id, site_id);

-- Primary access pattern for dashboards: "this site, this time range".
CREATE INDEX IF NOT EXISTS idx_rum_page_views_site_created_at
    ON rum_page_views (site_id, created_at);

CREATE INDEX IF NOT EXISTS idx_rum_page_views_session_id
    ON rum_page_views (session_id);

-- "Slow pages" / per-path aggregation.
CREATE INDEX IF NOT EXISTS idx_rum_page_views_site_path
    ON rum_page_views (site_id, path);

CREATE TABLE IF NOT EXISTS rum_js_errors (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id UUID NOT NULL REFERENCES rum_sites(id) ON DELETE CASCADE,
    session_id UUID NOT NULL REFERENCES rum_sessions(id) ON DELETE CASCADE,
    page_view_id UUID REFERENCES rum_page_views(id) ON DELETE CASCADE,

    -- Truncated by the domain layer before persistence (see
    -- RumJsError::MAX_MESSAGE_LEN). This value originates in the
    -- visitor's browser and is NEVER trusted as safe HTML — the
    -- dashboard API (Task F) must render it as plain text only.
    message TEXT NOT NULL,
    source TEXT,
    line_number INTEGER,
    column_number INTEGER,

    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_rum_js_errors_tenant_site
    ON rum_js_errors (tenant_id, site_id);

CREATE INDEX IF NOT EXISTS idx_rum_js_errors_site_created_at
    ON rum_js_errors (site_id, created_at);

CREATE TABLE IF NOT EXISTS rum_resource_errors (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id UUID NOT NULL REFERENCES rum_sites(id) ON DELETE CASCADE,
    session_id UUID NOT NULL REFERENCES rum_sessions(id) ON DELETE CASCADE,
    page_view_id UUID REFERENCES rum_page_views(id) ON DELETE CASCADE,

    -- Also browser-controlled, untrusted, plain-text-only on render.
    resource_url TEXT NOT NULL,
    resource_type TEXT, -- 'script' | 'stylesheet' | 'image' | 'fetch' | 'other'
    status_code INTEGER,

    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_rum_resource_errors_tenant_site
    ON rum_resource_errors (tenant_id, site_id);

CREATE INDEX IF NOT EXISTS idx_rum_resource_errors_site_created_at
    ON rum_resource_errors (site_id, created_at);
