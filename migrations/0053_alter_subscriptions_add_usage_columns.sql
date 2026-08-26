ALTER TABLE subscriptions
    ADD COLUMN monitors BIGINT,
    ADD COLUMN interval_seconds INTEGER,
    ADD COLUMN retention_days INTEGER,
    ADD COLUMN pricing_version INTEGER;
