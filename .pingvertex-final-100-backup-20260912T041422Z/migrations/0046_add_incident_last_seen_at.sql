-- B23: Incident State Machine — last_seen_at tracking.
--
-- `started_at` (existing column) never changes after an incident opens.
-- `last_seen_at` is advanced every time a failing check is evaluated
-- while the incident is still open, so the gap between the two reflects
-- how long the underlying problem has continued to reproduce.
ALTER TABLE incidents
    ADD COLUMN IF NOT EXISTS last_seen_at TIMESTAMPTZ;
