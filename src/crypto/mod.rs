//! Cryptographic operations for Slack VFS.
//!
//! This module provides:
//! - AES-256-GCM authenticated encryption
//! - Argon2id password-based key derivation

mod cipher;
mod kdf;

pub use cipher::{
    decrypt_data, decrypt_with_key, encrypt_data, encrypt_with_key, Cipher, EncryptedData,
};
pub use kdf::KeyDerivation;
