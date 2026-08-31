-- Task E's per-path performance breakdown (`RumAnalyticsRepository::
-- slow_pages`) filters by (site_id, created_at range) and groups by
-- path. The existing `idx_rum_page_views_site_path (site_id, path)` from
-- 0062 helps narrow to a site but doesn't cover the time-range filter,
-- and `idx_rum_page_views_site_created_at (site_id, created_at)` doesn't
-- cover the GROUP BY path. This composite index covers both parts of
-- that query in one index-only-ish scan.
CREATE INDEX IF NOT EXISTS idx_rum_page_views_site_path_created_at
    ON rum_page_views (site_id, path, created_at);
