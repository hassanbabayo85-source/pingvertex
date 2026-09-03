-- AUDIT ISSUE #2 (2026-09 remediation): "Diagnostic reports still assume
-- check_id alone is unique".
--
-- Migration 0015 created a UNIQUE INDEX on diagnostic_reports(check_id)
-- alone, back when checks.id was itself the sole PRIMARY KEY. Migrations
-- 0060/0061 later moved checks' canonical identity to the composite
-- (id, checked_at) — including repointing diagnostic_reports' own FK at
-- that composite key — but never touched this uniqueness constraint,
-- leaving it as the one remaining place in the schema still assuming
-- check_id alone identifies a Check.
--
-- Given migration 0060/0061 already established (CheckId, checked_at) as
-- the canonical identity (see also the Rust-side fix to
-- `CheckRepository::find_by_id`, application code audit issue #1), the
-- "one diagnostic report per Check" invariant this index exists to
-- enforce must be expressed as uniqueness on the full composite identity:
-- (check_id, check_checked_at). This is NOT a behavior change for any
-- row created through the normal write path (a given Check has exactly
-- one (check_id, check_checked_at) pair), but it does close the identity
-- ambiguity flagged by the audit and keeps this constraint consistent
-- with the composite FK diagnostic_reports already carries.
DROP INDEX IF EXISTS idx_diagnostic_reports_check_id;

CREATE UNIQUE INDEX IF NOT EXISTS idx_diagnostic_reports_check_id_checked_at
    ON diagnostic_reports (check_id, check_checked_at);
