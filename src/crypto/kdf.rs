//! Argon2id key derivation for password-based encryption.

use crate::config::argon2_params;
use crate::error::{Error, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngCore;

/// Key derivation using Argon2id.
#[derive(Debug, Clone)]
pub struct KeyDerivation {
    salt: [u8; argon2_params::SALT_LENGTH],
}

impl KeyDerivation {
    /// Create a new KDF with a random salt.
    pub fn new() -> Self {
        let mut salt = [0u8; argon2_params::SALT_LENGTH];
        rand::thread_rng().fill_bytes(&mut salt);
        Self { salt }
    }

    /// Create a KDF from an existing salt (for decryption).
    pub fn from_salt(salt: [u8; argon2_params::SALT_LENGTH]) -> Self {
        Self { salt }
    }

    /// Get the salt for storage.
    pub fn salt(&self) -> &[u8; argon2_params::SALT_LENGTH] {
        &self.salt
    }

    /// Derive a 256-bit key from a password.
    ///
    /// Uses Argon2id with the following parameters:
    /// - Memory: 64 MB
    /// - Iterations: 3
    /// - Parallelism: 4
    pub fn derive_key(&self, password: &str) -> Result<[u8; 32]> {
        let params = Params::new(
            argon2_params::MEMORY_COST,
            argon2_params::TIME_COST,
            argon2_params::PARALLELISM,
            Some(argon2_params::OUTPUT_LENGTH),
        )
        .map_err(|e| Error::KeyDerivation(e.to_string()))?;

        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        let mut key = [0u8; 32];
        argon2
            .hash_password_into(password.as_bytes(), &self.salt, &mut key)
            .map_err(|e| Error::KeyDerivation(e.to_string()))?;

        Ok(key)
    }
}

impl Default for KeyDerivation {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_derivation_deterministic() {
        let salt = [1u8; 32];
        let kdf = KeyDerivation::from_salt(salt);

        let key1 = kdf.derive_key("password123").unwrap();
        let key2 = kdf.derive_key("password123").unwrap();

        assert_eq!(key1, key2);
    }

    #[test]
    fn test_different_passwords_different_keys() {
        let salt = [2u8; 32];
        let kdf = KeyDerivation::from_salt(salt);

        let key1 = kdf.derive_key("password1").unwrap();
        let key2 = kdf.derive_key("password2").unwrap();

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_different_salts_different_keys() {
        let kdf1 = KeyDerivation::from_salt([1u8; 32]);
        let kdf2 = KeyDerivation::from_salt([2u8; 32]);

        let key1 = kdf1.derive_key("password").unwrap();
        let key2 = kdf2.derive_key("password").unwrap();

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_new_generates_random_salt() {
        let kdf1 = KeyDerivation::new();
        let kdf2 = KeyDerivation::new();

        assert_ne!(kdf1.salt(), kdf2.salt());
    }
}
