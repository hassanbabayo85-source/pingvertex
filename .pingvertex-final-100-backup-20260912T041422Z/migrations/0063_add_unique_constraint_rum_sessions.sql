-- Task C (RUM ingestion) needs to find-or-create a session by
-- (site_id, session_key) as a single atomic upsert (`INSERT ... ON
-- CONFLICT (site_id, session_key) DO UPDATE`), so a beacon arriving twice
-- for the same session (e.g. a page view beacon and an error beacon fired
-- close together) can't race and create two session rows.
--
-- This requires a UNIQUE constraint on those columns, which
-- 0062_create_rum_tables.sql only created as a plain (non-unique) index.
-- Shipped as its own migration rather than editing 0062 in place, since
-- 0062 may already have been applied to a running database by the time
-- this lands (migrations here are treated as append-only, per existing
-- project convention).

DROP INDEX IF EXISTS idx_rum_sessions_site_key;

ALTER TABLE rum_sessions
    ADD CONSTRAINT uq_rum_sessions_site_id_session_key UNIQUE (site_id, session_key);
