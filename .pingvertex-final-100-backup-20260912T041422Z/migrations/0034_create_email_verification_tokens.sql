CREATE TABLE email_verification_tokens (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_prefix TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    used_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Not UNIQUE: token_prefix is only 8 random bytes (see
-- UuidIdGenerator::next_email_verification_token). An astronomically
-- rare collision across two different users' tokens must not crash an
-- insert — VerifyEmailUseCase::execute already re-verifies the FULL
-- raw token against the stored Argon2 hash after the prefix lookup, so
-- a prefix collision alone can never verify the wrong token; it would
-- only ever cost an extra failed Argon2 verify.
CREATE INDEX idx_email_verification_tokens_prefix ON email_verification_tokens (token_prefix);
CREATE INDEX idx_email_verification_tokens_user_id ON email_verification_tokens (user_id);
