-- Blueprint Part 1, Section 10: absolute pricing overlap enforcement.
--
-- The application-level check in
-- `postgres_payg_pricing_repository.rs::create_version_with_context` only
-- rejects equal-specificity overlapping rules when the NEW rule is already
-- effective (`effective_from <= now()`) AND only against rules that are
-- themselves already effective. That leaves two gaps:
--
--   1. A future-effective rule (`effective_from > now()`) is never checked.
--   2. A rule whose effective_from is in the future is never considered as
--      the existing side of a check.
--
-- Both gaps let an operator queue up two future rules that would become
-- ambiguous the moment the earlier one activates. The resolver in
-- `find_active_for_context` would then have to silently pick one by
-- (specificity, effective_from, version) ordering, which is exactly the
-- silent ambiguity the blueprint wants to prevent at creation time.
--
-- A plain `EXCLUDE USING gist` cannot express this rule:
--   * Every row currently has `effective_until IS NULL`, so every row's
--     time window is `[effective_from, infinity)`. Two rows with the same
--     context therefore always overlap in time. An EXCLUDE constraint over
--     the time range would forbid ALL same-context rows, which is exactly
--     the versioning mechanism the schema relies on (blueprint Section 2:
--     "new pricing = new row, old rows remain immutable").
--   * NULL-as-wildcard semantics (NULL account_type = "any account type")
--     cannot be expressed in a single EXCLUDE column list without COALESCE
--     tricks that break wildcard overlap in the other direction.
--
-- A BEFORE INSERT trigger can express the exact application-level rule
-- (specificity, wildcard context, quantity range, time overlap) and also
-- close the two gaps above by dropping the `effective_from <= now()` filters.
--
-- Scope of this trigger:
--   * Fires on INSERT only. The table's existing
--     `trg_payg_pricing_immutable` trigger already rejects all UPDATE and
--     DELETE, so an UPDATE path is unreachable.
--   * Rows with identical context are allowed (that is how a new version
--     is appended for the same price context). The comparison against the
--     full context tuple is what distinguishes "new version" from
--     "ambiguous overlap".
--   * Equal specificity is required for a rejection. A rule with strictly
--     greater specificity is intentionally allowed to coexist and win the
--     resolver's `ORDER BY specificity DESC` tie-break.
--
-- Historical seed migrations (0079, 0087, 0091, 0093) run BEFORE this
-- migration creates the trigger, so they are unaffected even on a fresh
-- database. Migrations that append new pricing rows after 0104 must be
-- aware that the trigger is now active.

BEGIN;

CREATE OR REPLACE FUNCTION prevent_ambiguous_payg_pricing_overlap()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    new_specificity INT;
    ambiguous_id    UUID;
BEGIN
    new_specificity :=
        (CASE WHEN NEW.account_type IS NOT NULL THEN 1 ELSE 0 END) +
        (CASE WHEN NEW.region IS NOT NULL THEN 1 ELSE 0 END) +
        (CASE WHEN NEW.monitor_kind IS NOT NULL THEN 1 ELSE 0 END) +
        (CASE WHEN NEW.min_quantity IS NOT NULL OR NEW.max_quantity IS NOT NULL THEN 1 ELSE 0 END);

    SELECT p.id
      INTO ambiguous_id
      FROM payg_pricing_configs p
     WHERE p.operation = NEW.operation
       AND p.active = TRUE
       AND p.id <> NEW.id
       -- same specificity as the new rule
       AND (
            (CASE WHEN p.account_type IS NOT NULL THEN 1 ELSE 0 END) +
            (CASE WHEN p.region IS NOT NULL THEN 1 ELSE 0 END) +
            (CASE WHEN p.monitor_kind IS NOT NULL THEN 1 ELSE 0 END) +
            (CASE WHEN p.min_quantity IS NOT NULL OR p.max_quantity IS NOT NULL THEN 1 ELSE 0 END)
           ) = new_specificity
       -- different exact context (same exact context = intentional new version)
       AND NOT (
            p.account_type IS NOT DISTINCT FROM NEW.account_type
        AND p.region       IS NOT DISTINCT FROM NEW.region
        AND p.monitor_kind IS NOT DISTINCT FROM NEW.monitor_kind
        AND p.min_quantity IS NOT DISTINCT FROM NEW.min_quantity
        AND p.max_quantity IS NOT DISTINCT FROM NEW.max_quantity
           )
       -- wildcard-aware context overlap
       AND (p.account_type IS NULL OR NEW.account_type IS NULL OR p.account_type = NEW.account_type)
       AND (p.region       IS NULL OR NEW.region       IS NULL OR p.region       = NEW.region)
       AND (p.monitor_kind IS NULL OR NEW.monitor_kind IS NULL OR p.monitor_kind = NEW.monitor_kind)
       -- quantity range overlap
       AND COALESCE(p.min_quantity,   0) <= COALESCE(NEW.max_quantity, 9223372036854775807)
       AND COALESCE(NEW.min_quantity, 0) <= COALESCE(p.max_quantity,   9223372036854775807)
       -- time window overlap (missing effective_until = open-ended)
       AND tstzrange(p.effective_from,   COALESCE(p.effective_until,   'infinity'::timestamptz), '[)')
        && tstzrange(NEW.effective_from, COALESCE(NEW.effective_until, 'infinity'::timestamptz), '[)')
     LIMIT 1;

    IF ambiguous_id IS NOT NULL THEN
        RAISE EXCEPTION
            'ambiguous PAYG pricing rule: overlaps existing rule % at equal specificity',
            ambiguous_id
            USING ERRCODE = 'check_violation',
                  HINT = 'Deactivate the existing rule or add a more specific context dimension';
    END IF;

    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_payg_pricing_no_ambiguous_overlap ON payg_pricing_configs;
CREATE TRIGGER trg_payg_pricing_no_ambiguous_overlap
BEFORE INSERT ON payg_pricing_configs
FOR EACH ROW
EXECUTE FUNCTION prevent_ambiguous_payg_pricing_overlap();

COMMIT;
