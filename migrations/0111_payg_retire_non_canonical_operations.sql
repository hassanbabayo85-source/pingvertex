-- PAYG blueprint: retire non-canonical historical pricing rows.
--
-- Migrations 0079 and 0087 seeded operations that do not exist in the
-- current canonical `PaygOperation::ALL` set (crates/domain/src/payg.rs):
--
--   0079: proxy_check, monitor_check, SMS
--   0087: heartbeat_ping
--
-- The historical pricing rows must remain (audit/history) — the
-- `payg_pricing_configs` immutability trigger forbids UPDATE/DELETE, and
-- deleting them would destroy historical billing references. Instead,
-- this migration records them in `payg_retired_operations`, which is what
-- `PostgresPAYGPricingRepository::list_active()` already filters against.
--
-- Note on `sms` vs `SMS`:
--   The current canonical operation is the lowercase `sms` (PaygOperation::Sms).
--   Uppercase `SMS` from migration 0079 is a stale, non-canonical spelling
--   that no current code path can bill. It is retired here; lowercase `sms`
--   remains active and billable.
--
-- Note on `heartbeat_ping` vs `heartbeat_processing`:
--   Current canonical is `heartbeat_processing` (PaygOperation::HeartbeatProcessing).
--   `heartbeat_ping` from migration 0087 is retired; `heartbeat_processing`
--   remains active and billable.
BEGIN;

INSERT INTO payg_retired_operations (operation, reason)
VALUES
    ('proxy_check',    'Replaced by proxy_check.one_time and proxy_check.monitoring'),
    ('monitor_check',  'Replaced by the *_check.monitoring operation family'),
    ('SMS',            'Non-canonical spelling; current operation is lowercase "sms"'),
    ('heartbeat_ping', 'Replaced by heartbeat_processing')
ON CONFLICT (operation) DO NOTHING;

COMMIT;
