CREATE TABLE backup_codes (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash TEXT NOT NULL,
    used_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_backup_codes_user_id ON backup_codes (user_id);

-- Speeds up VerifyTotpAndLoginUseCase's `find_unused_by_user` lookup,
-- which only ever wants the not-yet-redeemed codes for a user.
CREATE INDEX idx_backup_codes_user_id_unused ON backup_codes (user_id) WHERE used_at IS NULL;
