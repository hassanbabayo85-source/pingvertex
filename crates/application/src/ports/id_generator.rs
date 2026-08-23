use async_trait::async_trait;
use domain::{
    ApiFlowId, ApiKeyId, CheckId, DiagnosticReportId, EscalationPolicyId, IncidentId,
    MaintenanceWindowId, MonitorId, NotificationChannelId, ProxyCheckJobId, ProxyEntryId,
    StatusPageId, UserId,
};

/// Generates unique IDs for domain entities.
#[async_trait]
pub trait IdGenerator: Send + Sync {
    async fn next_user_id(&self) -> UserId;

    async fn next_monitor_id(&self) -> MonitorId;

    async fn next_check_id(&self) -> CheckId;

    async fn next_incident_id(&self) -> IncidentId;

    async fn next_notification_channel_id(&self) -> NotificationChannelId;

    async fn next_api_flow_id(&self) -> ApiFlowId;

    /// Generates a new high-entropy secret used to HMAC-sign outgoing
    /// webhook payloads (see `crates/infra/src/services/webhook_notification_service.rs`).
    async fn next_webhook_signing_secret(&self) -> String;

    /// Generates a new `DiagnosticReportId`.
    ///
    /// Given a default implementation (rather than a required method)
    /// because, like every other ID in this trait, it is just a fresh
    /// random UUID wrapper with no infra-specific state involved —
    /// existing implementors (e.g. `UuidIdGenerator`) do not need to be
    /// touched to pick this up.
    async fn next_diagnostic_report_id(&self) -> DiagnosticReportId {
        DiagnosticReportId::new()
    }

    /// Generates a new `EscalationPolicyId`.
    async fn next_escalation_policy_id(&self) -> EscalationPolicyId {
        EscalationPolicyId::new()
    }

    /// Generates a new `MaintenanceWindowId`.
    async fn next_maintenance_window_id(&self) -> MaintenanceWindowId {
        MaintenanceWindowId::new()
    }

    /// Generates a new `StatusPageId`.
    ///
    /// Same reasoning as the other default-impl IDs above: a fresh random
    /// UUID wrapper, no infra-specific state, so existing implementors of
    /// this trait pick it up for free.
    async fn next_status_page_id(&self) -> StatusPageId {
        StatusPageId::new()
    }

    /// Generates a new `ProxyCheckJobId`.
    ///
    /// Same reasoning as `next_status_page_id`: a fresh random UUID
    /// wrapper with no infra-specific state, so existing implementors of
    /// this trait (e.g. `UuidIdGenerator`) pick it up for free without
    /// needing to be touched.
    async fn next_proxy_check_job_id(&self) -> ProxyCheckJobId {
        ProxyCheckJobId::new()
    }

    /// Generates a new `ProxyEntryId`.
    ///
    /// Same reasoning as `next_proxy_check_job_id` above.
    async fn next_proxy_entry_id(&self) -> ProxyEntryId {
        ProxyEntryId::new()
    }

    /// Generates a new `ApiKeyId`.
    ///
    /// Same reasoning as `next_status_page_id`/`next_proxy_check_job_id`
    /// above: a fresh random UUID wrapper with no infra-specific state, so
    /// existing implementors of this trait pick it up for free.
    async fn next_api_key_id(&self) -> ApiKeyId {
        ApiKeyId::new()
    }

    /// Generates a new raw API key value, formatted as
    /// `pv_<16 lowercase-hex chars>_<64 lowercase-hex chars>`.
    ///
    /// The part before the second underscore (`pv_<16 hex chars>`) is the
    /// lookup prefix — stored in plaintext as `ApiKey::key_prefix` so
    /// `ApiKeyAuthMiddleware` can find the row with a single indexed
    /// lookup. The full string (prefix + secret) is what gets Argon2-hashed
    /// via `PasswordHasher` and is what the caller must present again on
    /// every request as `X-API-Key`. Unlike `next_diagnostic_report_id`
    /// and friends above, this is NOT a default method: generating
    /// cryptographically secure randomness is an infra concern (OS CSPRNG),
    /// mirroring `next_webhook_signing_secret`.
    async fn next_api_key_secret(&self) -> String;

    /// Generates a new `BackupCodeId`.
    ///
    /// Same reasoning as `next_status_page_id`/`next_api_key_id` above: a
    /// fresh random UUID wrapper with no infra-specific state, so
    /// existing implementors of this trait pick it up for free.
    async fn next_backup_code_id(&self) -> domain::BackupCodeId {
        domain::BackupCodeId::new()
    }

    /// Generates one raw, human-typeable TOTP backup/recovery code (e.g.
    /// `"3f9a-2c1e"`). NOT a default method: like
    /// `next_webhook_signing_secret`/`next_api_key_secret`, generating
    /// secure randomness is an infra concern (OS CSPRNG). The raw value
    /// returned here is shown to the user exactly once by
    /// `ConfirmTotpSetupUseCase` — only its `PasswordHasher` hash is ever
    /// persisted (see `BackupCode.code_hash`).
    async fn next_backup_code(&self) -> String;

    /// Generates a new opaque, high-entropy pending-2FA token. Handed
    /// back to a client by `LoginUserUseCase` in place of a session JWT
    /// when `user.totp_enabled()` is true, and stored (mapped to the
    /// pending user id) in `PendingAuthCache` with a 5-minute TTL. NOT a
    /// default method for the same reason as `next_backup_code` above.
    async fn next_pending_two_factor_token(&self) -> String;

    /// Generates a new raw email-verification token, formatted as
    /// `evt_<16 lowercase-hex chars>_<64 lowercase-hex chars>`.
    ///
    /// The part before the second underscore is the plaintext lookup
    /// prefix (`EmailVerificationToken::token_prefix`); the full raw
    /// token is Argon2-hashed and only that hash is persisted. NOT a
    /// default method for the same reason as `next_api_key_secret`.
    async fn next_email_verification_token(&self) -> String;

    /// Generates a new raw password-reset token, formatted as
    /// `rst_<16 lowercase-hex chars>_<64 lowercase-hex chars>` — same
    /// shape as `next_email_verification_token`, deliberately using a
    /// different scheme tag (see `application::use_cases::reset_password_support`)
    /// so the two token kinds can never be confused for one another. NOT
    /// a default method for the same reason as `next_email_verification_token`.
    async fn next_reset_password_token(&self) -> String;

    /// Generates a new raw tenant-invitation token, formatted as
    /// `inv_<16 lowercase-hex chars>_<64 lowercase-hex chars>`.
    ///
    /// Deliberately uses a different scheme tag from email-verification
    /// (`evt_...`) and password-reset (`rst_...`) tokens so the three can
    /// never be confused at the HTTP boundary. The raw token is returned
    /// exactly once to the inviting admin; only its Argon2id hash is
    /// persisted.
    async fn next_invitation_token(&self) -> String;
}
