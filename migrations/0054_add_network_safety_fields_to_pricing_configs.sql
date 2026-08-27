-- Add network egress and safety buffer columns to pricing_configs.
-- These are config-driven values replacing previously hardcoded constants
-- in the pricing calculation engine.
ALTER TABLE pricing_configs
    ADD COLUMN avg_billable_network_bytes_per_check BIGINT NOT NULL DEFAULT 512,
    ADD COLUMN cost_per_gb_egress NUMERIC(20,6) NOT NULL DEFAULT 0.12,
    ADD COLUMN cost_safety_buffer NUMERIC(20,6) NOT NULL DEFAULT 1.20;
