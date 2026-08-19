-- Add encrypted_secret column for storing encrypted bot tokens / SMS credentials.
ALTER TABLE notification_channels
    ADD COLUMN IF NOT EXISTS encrypted_secret BYTEA;

-- Update kind CHECK constraint to include new channel kinds.
ALTER TABLE notification_channels
    DROP CONSTRAINT IF EXISTS chk_notification_channels_kind;

ALTER TABLE notification_channels
    ADD CONSTRAINT chk_notification_channels_kind
        CHECK (kind IN ('email', 'webhook', 'slack', 'discord', 'telegram', 'sms'));

-- Update signing_secret constraint: only webhook has signing_secret; all other kinds NULL.
ALTER TABLE notification_channels
    DROP CONSTRAINT IF EXISTS chk_notification_channels_signing_secret_webhook_only;

ALTER TABLE notification_channels
    ADD CONSTRAINT chk_notification_channels_signing_secret_webhook_only
        CHECK (
            (kind = 'webhook' AND signing_secret IS NOT NULL)
            OR (kind IN ('email', 'slack', 'discord', 'telegram', 'sms') AND signing_secret IS NULL)
        );
