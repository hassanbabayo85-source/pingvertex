CREATE TABLE IF NOT EXISTS proxy_entries (
    id UUID PRIMARY KEY,
    job_id UUID NOT NULL REFERENCES proxy_check_jobs(id) ON DELETE CASCADE,
    host TEXT NOT NULL,
    port INT NOT NULL,
    scheme TEXT NOT NULL,

    -- SECURITY: proxy credentials (if any) are encrypted at the
    -- application layer (see domain::ProxyCredentialCipher) before ever
    -- reaching this column. This column stores ciphertext only — BYTEA,
    -- never TEXT — so nothing that looks like a plaintext
    -- password/secret column exists in this table.
    encrypted_credentials BYTEA,

    result_json JSONB
);

CREATE INDEX IF NOT EXISTS idx_proxy_entries_job_id
    ON proxy_entries (job_id);
