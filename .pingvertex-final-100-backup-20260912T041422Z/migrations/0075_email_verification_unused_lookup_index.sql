-- Speeds the hot OTP verification query without indexing redeemed rows.
CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_user_unused_created_at
    ON email_verification_tokens (user_id, created_at DESC)
    WHERE used_at IS NULL;
