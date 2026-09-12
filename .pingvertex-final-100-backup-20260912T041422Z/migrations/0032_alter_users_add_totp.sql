ALTER TABLE users
    ADD COLUMN totp_secret BYTEA NULL,
    ADD COLUMN totp_enabled BOOLEAN NOT NULL DEFAULT false;
