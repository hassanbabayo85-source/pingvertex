CREATE TABLE IF NOT EXISTS notification_channels (
    id UUID PRIMARY KEY,
    monitor_id UUID NOT NULL REFERENCES monitors(id) ON DELETE CASCADE,

    -- Denormalized copy of the owning monitor's user_id, kept in sync by
    -- the application layer (a channel is only ever created after
    -- confirming the monitor belongs to this user — see
    -- CreateNotificationChannelUseCase). This lets ownership be enforced
    -- with a single indexed `WHERE id = $1 AND user_id = $2`, matching the
    -- pattern already used for monitors/checks/incidents in this project,
    -- without requiring a join through `monitors` on every read.
    --
    -- REFERENCES users(id) ON DELETE CASCADE is added here even though
    -- `monitors.user_id` itself has no such constraint (see
    -- 0001_create_monitors.sql), because this column exists specifically
    -- as an ownership-check shortcut and should never be allowed to
    -- silently disagree with its source of truth.
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    kind TEXT NOT NULL,

    -- Email address (kind = 'email') or webhook URL (kind = 'webhook'),
    -- already validated by the Email/Url domain value objects before
    -- being persisted.
    target TEXT NOT NULL,

    -- HMAC-SHA256 signing secret used to sign outgoing webhook payloads
    -- (X-Webhook-Signature header). Populated for webhook channels only —
    -- see the CHECK constraint below, which enforces the same invariant
    -- the domain entity already enforces in Rust
    -- (NotificationChannel::create_email/create_webhook).
    signing_secret TEXT,

    enabled BOOLEAN NOT NULL DEFAULT TRUE,

    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,

    CONSTRAINT chk_notification_channels_kind
        CHECK (kind IN ('email', 'webhook')),

    CONSTRAINT chk_notification_channels_signing_secret_webhook_only
        CHECK (
            (kind = 'webhook' AND signing_secret IS NOT NULL)
            OR (kind = 'email' AND signing_secret IS NULL)
        )
);

CREATE INDEX IF NOT EXISTS idx_notification_channels_monitor_id
    ON notification_channels (monitor_id);

CREATE INDEX IF NOT EXISTS idx_notification_channels_user_id
    ON notification_channels (user_id);

-- Used by IncidentManager::notify_channels on every incident open/resolve,
-- so it needs to be fast and cover exactly what that query filters on.
CREATE INDEX IF NOT EXISTS idx_notification_channels_monitor_enabled
    ON notification_channels (monitor_id, enabled)
    WHERE enabled = TRUE;
