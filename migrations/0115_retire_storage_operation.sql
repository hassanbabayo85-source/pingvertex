-- ============================================================================
-- GAP 3 — Kashi A4: Retire the `storage` PAYG operation.
--
-- Storage is no longer billed as a separate PAYG line item. The per-check
-- price already covers the storage footprint of check results, and logo
-- uploads on status pages are now free (see
-- `upload_status_page_logo.rs` — metering removed).
--
-- The historical `payg_pricing_configs` rows for `storage` inserted by
-- migrations 0079 / 0091 / 0093 / 0114 remain queryable (the price book
-- is append-only for audit). Only the retirement filter row is added, so
-- `PostgresPAYGPricingRepository::list_active()` stops advertising the
-- operation.
BEGIN;

INSERT INTO payg_retired_operations (operation, reason)
VALUES (
    'storage',
    'Retired in GAP 3: storage is included in the per-check price; logo uploads are free.'
)
ON CONFLICT (operation) DO NOTHING;

COMMIT;
