-- Webhook HMAC signing secrets are now encrypted at rest (AES-256-GCM, the
-- same cipher already used for Telegram bot tokens / SMS credentials) —
-- see CreateNotificationChannelUseCase and WebhookNotificationService.
-- New webhook channels persist the secret in `encrypted_secret` with the
-- legacy plaintext `signing_secret` column left NULL. Channels created
-- before this migration keep their plaintext `signing_secret` until
-- rotated (delete + recreate the channel), so existing webhook
-- integrations don't break the moment this ships.
--
-- The CHECK constraint is relaxed accordingly: a webhook channel must have
-- AT LEAST ONE of `signing_secret` / `encrypted_secret` populated, instead
-- of requiring `signing_secret` specifically. Every non-webhook kind is
-- unchanged — signing_secret must still be NULL for them (they use
-- encrypted_secret exclusively, as already enforced).
ALTER TABLE notification_channels
    DROP CONSTRAINT IF EXISTS chk_notification_channels_signing_secret_webhook_only;

ALTER TABLE notification_channels
    ADD CONSTRAINT chk_notification_channels_signing_secret_webhook_only
        CHECK (
            (kind = 'webhook' AND (signing_secret IS NOT NULL OR encrypted_secret IS NOT NULL))
            OR (kind IN ('email', 'slack', 'discord', 'telegram', 'sms') AND signing_secret IS NULL)
        );
