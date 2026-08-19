use std::env;

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};

use application::ports::{
    EncryptedProxyCredentials, ProxyCredentialCipher, ProxyCredentialCipherError,
};
use domain::value_objects::ProxyCredentials;

/// Length, in bytes, that `PROXY_CREDENTIAL_ENCRYPTION_KEY` must decode to.
/// AES-256-GCM requires a 256-bit (32-byte) key.
const KEY_LEN_BYTES: usize = 32;

/// Length, in bytes, of the AES-GCM nonce (96 bits, the standard/only
/// size supported by the `aes-gcm` crate's `Aes256Gcm`).
const NONCE_LEN_BYTES: usize = 12;

/// Error loading/validating `PROXY_CREDENTIAL_ENCRYPTION_KEY` from the
/// environment.
///
/// Deliberately never includes the raw env var value in any variant —
/// only lengths/format facts — so that a `{err}`/`{err:?}` in a log line
/// can never leak key material, even by accident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyCredentialEncryptionKeyError {
    MissingKey,
    /// The value was neither valid hex nor valid base64.
    UndecodableKey,
    /// Decoded successfully, but not to exactly 32 bytes.
    WrongKeyLength {
        decoded_len: usize,
    },
}

impl std::fmt::Display for ProxyCredentialEncryptionKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingKey => write!(
                f,
                "PROXY_CREDENTIAL_ENCRYPTION_KEY environment variable is not set"
            ),
            Self::UndecodableKey => write!(
                f,
                "PROXY_CREDENTIAL_ENCRYPTION_KEY is not valid hex or base64"
            ),
            Self::WrongKeyLength { decoded_len } => write!(
                f,
                "PROXY_CREDENTIAL_ENCRYPTION_KEY must decode to exactly {KEY_LEN_BYTES} bytes, got {decoded_len}"
            ),
        }
    }
}

impl std::error::Error for ProxyCredentialEncryptionKeyError {}

fn non_empty_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Decodes a hex string into bytes. Returns `None` on any non-hex
/// character or odd length, rather than panicking — the caller treats
/// that as "not hex, try base64 next".
fn decode_hex(input: &str) -> Option<Vec<u8>> {
    if !input.len().is_multiple_of(2) {
        return None;
    }

    let mut bytes = Vec::with_capacity(input.len() / 2);
    let chars: Vec<char> = input.chars().collect();

    for pair in chars.chunks(2) {
        let hi = pair[0].to_digit(16)?;
        let lo = pair[1].to_digit(16)?;
        bytes.push(((hi << 4) | lo) as u8);
    }

    Some(bytes)
}

/// Decodes a standard-alphabet base64 string (with or without `=`
/// padding) into bytes. Returns `None` on any invalid character/padding
/// rather than panicking.
fn decode_base64(input: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let stripped = input.trim_end_matches('=');
    if stripped.is_empty() || !stripped.bytes().all(|b| ALPHABET.contains(&b)) {
        return None;
    }

    let mut bits: u32 = 0;
    let mut bit_count: u32 = 0;
    let mut out = Vec::with_capacity((stripped.len() * 3) / 4 + 1);

    for &b in stripped.as_bytes() {
        let value = ALPHABET.iter().position(|&c| c == b)? as u32;
        bits = (bits << 6) | value;
        bit_count += 6;

        if bit_count >= 8 {
            bit_count -= 8;
            out.push(((bits >> bit_count) & 0xFF) as u8);
        }
    }

    Some(out)
}

/// Decodes `PROXY_CREDENTIAL_ENCRYPTION_KEY`, trying hex first, then
/// base64, and validating the decoded length is exactly 32 bytes.
///
/// SECURITY: on ANY failure (missing var, undecodable, wrong length) this
/// returns `Err` — there is no fallback path that returns a default key
/// or disables encryption. Fail loudly is the whole point: a caller that
/// ignores this `Result` and unwraps will panic at startup, which is the
/// desired behavior for a misconfigured encryption key (unlike SMTP,
/// which is genuinely optional, encryption of stored proxy credentials
/// is not something this feature can silently skip).
fn load_encryption_key() -> Result<[u8; KEY_LEN_BYTES], ProxyCredentialEncryptionKeyError> {
    let raw = non_empty_env("PROXY_CREDENTIAL_ENCRYPTION_KEY")
        .ok_or(ProxyCredentialEncryptionKeyError::MissingKey)?;

    let decoded = decode_hex(&raw)
        .or_else(|| decode_base64(&raw))
        .ok_or(ProxyCredentialEncryptionKeyError::UndecodableKey)?;

    let decoded_len = decoded.len();
    let key: [u8; KEY_LEN_BYTES] = decoded
        .try_into()
        .map_err(|_| ProxyCredentialEncryptionKeyError::WrongKeyLength { decoded_len })?;

    Ok(key)
}

/// `ProxyCredentialCipher` implementation backed by AES-256-GCM.
///
/// Wire format produced by `encrypt` (and expected by `decrypt`):
/// `nonce (12 bytes) || ciphertext_with_tag`. The nonce is generated
/// fresh (via the OS CSPRNG) on every single `encrypt` call and prefixed
/// onto the output — AES-GCM's confidentiality/integrity guarantees
/// depend entirely on a nonce never being reused under the same key, so
/// reusing a fixed or counter-reset nonce here would be a critical
/// break. `EncryptedProxyCredentials` is opaque to the application
/// layer, so this layout is a private implementation detail owned
/// entirely by this file.
///
/// Plaintext layout before encryption:
/// `username_len (4 bytes, big-endian u32) || username_bytes || password_bytes`.
/// `ProxyCredentials` doesn't derive `Serialize` (deliberately — see its
/// hand-written `Debug` impl redacting `password`), so this cipher owns
/// its own minimal, unambiguous binary encoding rather than depending on
/// serde.
pub struct AesGcmProxyCredentialCipher {
    cipher: Aes256Gcm,
}

impl AesGcmProxyCredentialCipher {
    /// Constructs the cipher directly from a 32-byte key.
    ///
    /// The `key` array is consumed by `Key::from_slice`/`Aes256Gcm::new`
    /// and the local copy is explicitly zeroed afterward — best-effort
    /// defense in depth against this fixed-size stack buffer lingering
    /// in memory longer than necessary. (`Aes256Gcm`'s own internal key
    /// schedule is zeroized on drop by the `aes-gcm`/`aes` crates'
    /// built-in `Drop` impls.)
    pub fn new(mut key: [u8; KEY_LEN_BYTES]) -> Self {
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        key.fill(0);
        Self { cipher }
    }

    /// Builds the cipher from `PROXY_CREDENTIAL_ENCRYPTION_KEY`.
    ///
    /// SECURITY: never falls back to "no encryption" or a default key —
    /// any `Err` here MUST be treated by the caller as fatal
    /// (propagate/`?` out of startup, or `expect`/`panic!`). Do not match
    /// on the `Err` case and continue with credentials stored in
    /// plaintext.
    pub fn try_from_env() -> Result<Self, ProxyCredentialEncryptionKeyError> {
        let key = load_encryption_key()?;
        Ok(Self::new(key))
    }

    /// Encrypts arbitrary plaintext bytes.
    pub fn encrypt_bytes(&self, plaintext: &[u8]) -> Result<Vec<u8>, ProxyCredentialCipherError> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = self.cipher.encrypt(&nonce, plaintext).map_err(|_| {
            ProxyCredentialCipherError::EncryptionFailed("AES-GCM encryption failed".to_string())
        })?;
        let mut out = Vec::with_capacity(NONCE_LEN_BYTES + ciphertext.len());
        out.extend_from_slice(nonce.as_slice());
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    /// Decrypts ciphertext produced by `encrypt_bytes`.
    pub fn decrypt_bytes(&self, encrypted: &[u8]) -> Result<Vec<u8>, ProxyCredentialCipherError> {
        if encrypted.len() < NONCE_LEN_BYTES {
            return Err(ProxyCredentialCipherError::DecryptionFailed(
                "ciphertext shorter than nonce prefix".to_string(),
            ));
        }
        let (nonce_bytes, ciphertext) = encrypted.split_at(NONCE_LEN_BYTES);
        let nonce = Nonce::from_slice(nonce_bytes);
        self.cipher.decrypt(nonce, ciphertext).map_err(|_| {
            ProxyCredentialCipherError::DecryptionFailed(
                "AES-GCM decryption failed (wrong key or corrupted/tampered ciphertext)"
                    .to_string(),
            )
        })
    }
}

impl std::fmt::Debug for AesGcmProxyCredentialCipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // SECURITY: deliberately does not print `self.cipher` — never
        // give an accidental `{:?}`/log call a path to key material.
        f.debug_struct("AesGcmProxyCredentialCipher").finish()
    }
}

impl ProxyCredentialCipher for AesGcmProxyCredentialCipher {
    fn encrypt(
        &self,
        credentials: &ProxyCredentials,
    ) -> Result<EncryptedProxyCredentials, ProxyCredentialCipherError> {
        let username_bytes = credentials.username().as_bytes();
        let password_bytes = credentials.password().as_bytes();

        let username_len: u32 = username_bytes.len().try_into().map_err(|_| {
            ProxyCredentialCipherError::EncryptionFailed("username too long to encode".to_string())
        })?;

        let mut plaintext = Vec::with_capacity(4 + username_bytes.len() + password_bytes.len());
        plaintext.extend_from_slice(&username_len.to_be_bytes());
        plaintext.extend_from_slice(username_bytes);
        plaintext.extend_from_slice(password_bytes);

        // SECURITY: fresh random nonce every call — never reused, never
        // derived from a counter that could reset across process
        // restarts.
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext.as_ref())
            .map_err(|_| {
                // SECURITY: `aes_gcm::Error` intentionally carries no
                // detail; do not attempt to format/log it alongside
                // plaintext or key state.
                ProxyCredentialCipherError::EncryptionFailed(
                    "AES-GCM encryption failed".to_string(),
                )
            })?;

        let mut out = Vec::with_capacity(NONCE_LEN_BYTES + ciphertext.len());
        out.extend_from_slice(nonce.as_slice());
        out.extend_from_slice(&ciphertext);

        Ok(EncryptedProxyCredentials::from(out))
    }

    fn decrypt(
        &self,
        encrypted: &EncryptedProxyCredentials,
    ) -> Result<ProxyCredentials, ProxyCredentialCipherError> {
        let bytes = encrypted.as_bytes();

        if bytes.len() < NONCE_LEN_BYTES {
            return Err(ProxyCredentialCipherError::DecryptionFailed(
                "ciphertext shorter than nonce prefix".to_string(),
            ));
        }

        let (nonce_bytes, ciphertext) = bytes.split_at(NONCE_LEN_BYTES);
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = self.cipher.decrypt(nonce, ciphertext).map_err(|_| {
            ProxyCredentialCipherError::DecryptionFailed(
                "AES-GCM decryption failed (wrong key or corrupted/tampered ciphertext)"
                    .to_string(),
            )
        })?;

        if plaintext.len() < 4 {
            return Err(ProxyCredentialCipherError::DecryptionFailed(
                "decrypted plaintext shorter than length prefix".to_string(),
            ));
        }

        let (len_prefix, rest) = plaintext.split_at(4);
        let username_len = u32::from_be_bytes(len_prefix.try_into().map_err(|_| {
            ProxyCredentialCipherError::DecryptionFailed("malformed length prefix".to_string())
        })?) as usize;

        if username_len > rest.len() {
            return Err(ProxyCredentialCipherError::DecryptionFailed(
                "length prefix exceeds decrypted plaintext size".to_string(),
            ));
        }

        let (username_bytes, password_bytes) = rest.split_at(username_len);

        let username = String::from_utf8(username_bytes.to_vec()).map_err(|_| {
            ProxyCredentialCipherError::DecryptionFailed(
                "decrypted username is not valid UTF-8".to_string(),
            )
        })?;
        let password = String::from_utf8(password_bytes.to_vec()).map_err(|_| {
            ProxyCredentialCipherError::DecryptionFailed(
                "decrypted password is not valid UTF-8".to_string(),
            )
        })?;

        ProxyCredentials::new(username, password).map_err(|err| {
            ProxyCredentialCipherError::DecryptionFailed(format!(
                "decrypted credentials failed domain validation: {err}"
            ))
        })
    }
}

impl application::ports::SecretCipher for AesGcmProxyCredentialCipher {
    fn encrypt_bytes(
        &self,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, application::ports::SecretCipherError> {
        self.encrypt_bytes(plaintext)
            .map_err(|err| application::ports::SecretCipherError::EncryptionFailed(err.to_string()))
    }

    fn decrypt_bytes(
        &self,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, application::ports::SecretCipherError> {
        self.decrypt_bytes(ciphertext)
            .map_err(|err| application::ports::SecretCipherError::DecryptionFailed(err.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cipher() -> AesGcmProxyCredentialCipher {
        AesGcmProxyCredentialCipher::new([7u8; KEY_LEN_BYTES])
    }

    #[test]
    fn round_trips_username_and_password() {
        let cipher = test_cipher();
        let creds = ProxyCredentials::new("user", "pass").unwrap();

        let encrypted = cipher.encrypt(&creds).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();

        assert_eq!(decrypted, creds);
    }

    #[test]
    fn two_encrypt_calls_use_different_nonces_and_ciphertexts() {
        let cipher = test_cipher();
        let creds = ProxyCredentials::new("user", "pass").unwrap();

        let first = cipher.encrypt(&creds).unwrap();
        let second = cipher.encrypt(&creds).unwrap();

        assert_ne!(first.as_bytes(), second.as_bytes());
        assert_ne!(
            &first.as_bytes()[..NONCE_LEN_BYTES],
            &second.as_bytes()[..NONCE_LEN_BYTES]
        );
    }

    #[test]
    fn decrypt_fails_with_wrong_key() {
        let cipher_a = AesGcmProxyCredentialCipher::new([1u8; KEY_LEN_BYTES]);
        let cipher_b = AesGcmProxyCredentialCipher::new([2u8; KEY_LEN_BYTES]);
        let creds = ProxyCredentials::new("user", "pass").unwrap();

        let encrypted = cipher_a.encrypt(&creds).unwrap();

        assert!(matches!(
            cipher_b.decrypt(&encrypted),
            Err(ProxyCredentialCipherError::DecryptionFailed(_))
        ));
    }

    #[test]
    fn decrypt_fails_on_tampered_ciphertext() {
        let cipher = test_cipher();
        let creds = ProxyCredentials::new("user", "pass").unwrap();

        let mut encrypted = cipher.encrypt(&creds).unwrap().as_bytes().to_vec();
        let last = encrypted.len() - 1;
        encrypted[last] ^= 0xFF;

        assert!(matches!(
            cipher.decrypt(&EncryptedProxyCredentials::from(encrypted)),
            Err(ProxyCredentialCipherError::DecryptionFailed(_))
        ));
    }

    #[test]
    fn decrypt_fails_on_truncated_ciphertext() {
        let cipher = test_cipher();

        assert!(matches!(
            cipher.decrypt(&EncryptedProxyCredentials::from(vec![0u8; 5])),
            Err(ProxyCredentialCipherError::DecryptionFailed(_))
        ));
    }

    #[test]
    fn decode_hex_round_trips_32_byte_key() {
        let hex = "07".repeat(KEY_LEN_BYTES);
        let decoded = decode_hex(&hex).unwrap();
        assert_eq!(decoded, vec![7u8; KEY_LEN_BYTES]);
    }

    #[test]
    fn wrong_length_key_is_rejected() {
        // 16 bytes hex-encoded (32 hex chars) — valid hex, wrong length.
        let hex = "00".repeat(16);
        let decoded = decode_hex(&hex).unwrap();
        let result: Result<[u8; KEY_LEN_BYTES], _> = decoded.clone().try_into();
        assert!(result.is_err());
        assert_eq!(decoded.len(), 16);
    }
}
