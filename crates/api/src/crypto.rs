use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use rand::RngCore;

use crate::error::AppError;

#[derive(Clone)]
pub struct EncryptionKey([u8; 32]);

impl EncryptionKey {
    pub fn from_hex(hex_str: &str) -> anyhow::Result<Self> {
        let bytes = hex::decode(hex_str)?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("ENCRYPTION_KEY must be exactly 32 bytes (64 hex chars)"))?;
        Ok(Self(arr))
    }

    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        Self(bytes)
    }
}

impl std::fmt::Debug for EncryptionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EncryptionKey([REDACTED])")
    }
}

/// Encrypt `plaintext`, returning `nonce (12 bytes) || ciphertext`.
pub fn encrypt(key: &EncryptionKey, plaintext: &[u8]) -> Result<Vec<u8>, AppError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key.0));

    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| AppError::Other(anyhow::anyhow!("Encryption failed: {e}")))?;

    let mut output = Vec::with_capacity(12 + ciphertext.len());
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

/// Decrypt data of the form `nonce (12 bytes) || ciphertext`.
pub fn decrypt(key: &EncryptionKey, data: &[u8]) -> Result<Vec<u8>, AppError> {
    if data.len() < 13 {
        return Err(AppError::Other(anyhow::anyhow!("Encrypted data too short")));
    }
    let (nonce_bytes, ciphertext) = data.split_at(12);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key.0));
    let nonce = Nonce::from_slice(nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| AppError::Other(anyhow::anyhow!("Decryption failed: {e}")))
}

pub fn encrypt_token(key: &EncryptionKey, token: &str) -> Result<Vec<u8>, AppError> {
    encrypt(key, token.as_bytes())
}

pub fn decrypt_token(key: &EncryptionKey, data: &[u8]) -> Result<String, AppError> {
    let plaintext = decrypt(key, data)?;
    String::from_utf8(plaintext)
        .map_err(|e| AppError::Other(anyhow::anyhow!("Invalid UTF-8 in decrypted token: {e}")))
}
