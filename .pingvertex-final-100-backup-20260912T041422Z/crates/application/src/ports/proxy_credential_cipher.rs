use std::sync::Arc;

use domain::value_objects::ProxyCredentials;

/// Opaque encrypted representation of `ProxyCredentials`.
///
/// The application layer never inspects or constructs the internal
/// layout of this blob — only whichever infra `ProxyCredentialCipher`
/// implementation produced/consumes it knows what's inside (nonce, tag,
/// ciphertext layout, key version, etc.). This is what keeps the
/// application layer ignorant of *how* encryption is done, per Clean
/// Architecture: infra implements a contract owned by application, not
/// the other way around.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedProxyCredentials(pub Vec<u8>);

impl EncryptedProxyCredentials {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for EncryptedProxyCredentials {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

/// Error returned by a `ProxyCredentialCipher` implementation.
///
/// Deliberately opaque (just a message) rather than exposing
/// algorithm-specific failure detail (e.g. AES-GCM tag-mismatch
/// internals) across the port boundary — the application layer only
/// needs to know encryption/decryption failed, not why in cryptographic
/// detail.
#[derive(Debug)]
pub enum ProxyCredentialCipherError {
    EncryptionFailed(String),
    DecryptionFailed(String),
}

impl std::fmt::Display for ProxyCredentialCipherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EncryptionFailed(msg) => {
                write!(f, "failed to encrypt proxy credentials: {msg}")
            }
            Self::DecryptionFailed(msg) => {
                write!(f, "failed to decrypt proxy credentials: {msg}")
            }
        }
    }
}

impl std::error::Error for ProxyCredentialCipherError {}

/// Encryption port for `ProxyCredentials`.
///
/// The application layer knows credentials must be encrypted before
/// they are persisted, but not HOW — that is an infra decision (which
/// this trait exists to let infra make, per Clean Architecture: infra
/// depends on and implements a contract owned by application/domain,
/// application never depends on infra).
///
/// NOT async: encrypting/decrypting a short credential pair (AES-GCM or
/// similar) is a synchronous, CPU-bound operation with no I/O — there is
/// nothing to `.await` on. Making this async would force every
/// implementation, and every caller, to pay for a future/executor on a
/// call that never actually yields. If a future implementation ever
/// needs I/O here (e.g. calling out to a network-backed KMS/HSM to
/// perform the encryption instead of using a local key), THAT is when
/// this trait would need to become `#[async_trait]` — it should not be
/// made async speculatively ahead of that need.
pub trait ProxyCredentialCipher: Send + Sync {
    fn encrypt(
        &self,
        credentials: &ProxyCredentials,
    ) -> Result<EncryptedProxyCredentials, ProxyCredentialCipherError>;

    fn decrypt(
        &self,
        encrypted: &EncryptedProxyCredentials,
    ) -> Result<ProxyCredentials, ProxyCredentialCipherError>;
}

impl<T> ProxyCredentialCipher for Arc<T>
where
    T: ProxyCredentialCipher + Send + Sync + ?Sized,
{
    fn encrypt(
        &self,
        credentials: &ProxyCredentials,
    ) -> Result<EncryptedProxyCredentials, ProxyCredentialCipherError> {
        (**self).encrypt(credentials)
    }

    fn decrypt(
        &self,
        encrypted: &EncryptedProxyCredentials,
    ) -> Result<ProxyCredentials, ProxyCredentialCipherError> {
        (**self).decrypt(encrypted)
    }
}
