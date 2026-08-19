pub type EncryptedSecret = Vec<u8>;

#[derive(Debug)]
pub enum SecretCipherError {
    EncryptionFailed(String),
    DecryptionFailed(String),
}

impl std::fmt::Display for SecretCipherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EncryptionFailed(msg) => write!(f, "failed to encrypt secret: {msg}"),
            Self::DecryptionFailed(msg) => write!(f, "failed to decrypt secret: {msg}"),
        }
    }
}

impl std::error::Error for SecretCipherError {}

pub trait SecretCipher: Send + Sync {
    fn encrypt_bytes(&self, plaintext: &[u8]) -> Result<Vec<u8>, SecretCipherError>;
    fn decrypt_bytes(&self, ciphertext: &[u8]) -> Result<Vec<u8>, SecretCipherError>;
}
