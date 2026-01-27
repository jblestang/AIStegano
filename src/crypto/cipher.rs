//! AES-256-GCM authenticated encryption.

use crate::crypto::kdf::KeyDerivation;
use crate::error::{Error, Result};
use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};

/// Nonce size for AES-GCM (96 bits).
const NONCE_SIZE: usize = 12;

/// Authentication tag size (128 bits).
const TAG_SIZE: usize = 16;

/// AES-256-GCM cipher wrapper.
pub struct Cipher {
    cipher: Aes256Gcm,
}

impl Cipher {
    /// Create a new cipher from a derived key.
    pub fn new(key: [u8; 32]) -> Self {
        let cipher = Aes256Gcm::new_from_slice(&key).expect("Invalid key length");
        Self { cipher }
    }

    /// Encrypt data with a random nonce.
    ///
    /// Returns: nonce (12 bytes) || ciphertext || tag (16 bytes)
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| Error::Encryption(e.to_string()))?;

        // Prepend nonce to ciphertext
        let mut result = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);

        Ok(result)
    }

    /// Decrypt data that was encrypted with `encrypt`.
    ///
    /// Expects: nonce (12 bytes) || ciphertext || tag (16 bytes)
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        if ciphertext.len() < NONCE_SIZE + TAG_SIZE {
            return Err(Error::Decryption);
        }

        let (nonce_bytes, ciphertext) = ciphertext.split_at(NONCE_SIZE);
        let nonce = Nonce::from_slice(nonce_bytes);

        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| Error::Decryption)
    }
}

/// Encrypted data with all information needed for decryption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedData {
    /// Salt for key derivation.
    pub salt: [u8; 32],
    /// The encrypted payload (nonce || ciphertext || tag).
    pub ciphertext: Vec<u8>,
}

impl EncryptedData {
    /// Get the total size of the encrypted data.
    pub fn size(&self) -> usize {
        self.salt.len() + self.ciphertext.len()
    }
}

/// Encrypt data with a password.
///
/// Uses Argon2id for key derivation and AES-256-GCM for encryption.
pub fn encrypt_data(plaintext: &[u8], password: &str) -> Result<EncryptedData> {
    let kdf = KeyDerivation::new();
    let key = kdf.derive_key(password)?;
    let cipher = Cipher::new(key);

    let ciphertext = cipher.encrypt(plaintext)?;

    Ok(EncryptedData {
        salt: *kdf.salt(),
        ciphertext,
    })
}

/// Decrypt data with a password.
pub fn decrypt_data(encrypted: &EncryptedData, password: &str) -> Result<Vec<u8>> {
    let kdf = KeyDerivation::from_salt(encrypted.salt);
    let key = kdf.derive_key(password)?;
    let cipher = Cipher::new(key);

    cipher.decrypt(&encrypted.ciphertext)
}

/// Encrypt data with a pre-derived key.
///
/// Uses the provided key directly for AES-256-GCM encryption.
/// The salt in the returned EncryptedData will be all zeros since
/// no key derivation is needed.
pub fn encrypt_with_key(plaintext: &[u8], key: &[u8; 32]) -> Result<Vec<u8>> {
    let cipher = Cipher::new(*key);
    cipher.encrypt(plaintext)
}

/// Decrypt data with a pre-derived key.
pub fn decrypt_with_key(ciphertext: &[u8], key: &[u8; 32]) -> Result<Vec<u8>> {
    let cipher = Cipher::new(*key);
    cipher.decrypt(ciphertext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let plaintext = b"Hello, World! This is a secret message.";
        let password = "secure_password_123";

        let encrypted = encrypt_data(plaintext, password).unwrap();
        let decrypted = decrypt_data(&encrypted, password).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_wrong_password_fails() {
        let plaintext = b"Secret data";
        let encrypted = encrypt_data(plaintext, "correct_password").unwrap();

        let result = decrypt_data(&encrypted, "wrong_password");
        assert!(result.is_err());
    }

    #[test]
    fn test_different_encryptions_different_ciphertext() {
        let plaintext = b"Same message";
        let password = "password";

        let encrypted1 = encrypt_data(plaintext, password).unwrap();
        let encrypted2 = encrypt_data(plaintext, password).unwrap();

        // Different salts and nonces should produce different ciphertext
        assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);
        assert_ne!(encrypted1.salt, encrypted2.salt);
    }

    #[test]
    fn test_empty_plaintext() {
        let plaintext = b"";
        let password = "password";

        let encrypted = encrypt_data(plaintext, password).unwrap();
        let decrypted = decrypt_data(&encrypted, password).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_large_plaintext() {
        let plaintext: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();
        let password = "password";

        let encrypted = encrypt_data(&plaintext, password).unwrap();
        let decrypted = decrypt_data(&encrypted, password).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let plaintext = b"Secret data";
        let password = "password";

        let mut encrypted = encrypt_data(plaintext, password).unwrap();
        // Tamper with the ciphertext
        if let Some(byte) = encrypted.ciphertext.last_mut() {
            *byte ^= 0xFF;
        }

        let result = decrypt_data(&encrypted, password);
        assert!(result.is_err());
    }
}
