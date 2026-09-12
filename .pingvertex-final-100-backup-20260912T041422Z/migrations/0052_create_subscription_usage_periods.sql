CREATE TABLE subscription_usage_periods (
    id UUID PRIMARY KEY,
    subscription_id UUID NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
    start_at TIMESTAMPTZ NOT NULL,
    end_at TIMESTAMPTZ,
    monitors BIGINT NOT NULL,
    interval_seconds INTEGER NOT NULL,
    retention_days INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_subscription_usage_periods_subscription_id ON subscription_usage_periods (subscription_id);
