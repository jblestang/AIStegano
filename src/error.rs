//! Error types for the Slack VFS.

use std::path::PathBuf;
use thiserror::Error;

/// Result type alias for Slack VFS operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in Slack VFS operations.
#[derive(Error, Debug)]
pub enum Error {
    /// I/O error during file operations.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// File not found in VFS.
    #[error("File not found: {0}")]
    FileNotFound(String),

    /// Path already exists.
    #[error("Path already exists: {0}")]
    PathExists(String),

    /// Not a directory.
    #[error("Not a directory: {0}")]
    NotADirectory(String),

    /// Not a file.
    #[error("Not a file: {0}")]
    NotAFile(String),

    /// Invalid path format.
    #[error("Invalid path: {0}")]
    InvalidPath(String),

    /// Not enough slack space available.
    #[error("Not enough slack space: need {needed} bytes, have {available} bytes")]
    InsufficientSpace { needed: u64, available: u64 },

    /// Host file not found.
    #[error("Host file not found: {0}")]
    HostFileNotFound(PathBuf),

    /// No host files available.
    #[error("No host files found in directory: {0}")]
    NoHostFiles(PathBuf),

    /// Encryption error.
    #[error("Encryption error: {0}")]
    Encryption(String),

    /// Decryption error (wrong password or corrupted data).
    #[error("Decryption failed: wrong password or corrupted data")]
    Decryption,

    /// Key derivation error.
    #[error("Key derivation error: {0}")]
    KeyDerivation(String),

    /// Encoding error.
    #[error("Encoding error: {0}")]
    Encoding(String),

    /// Decoding error - not enough symbols to recover data.
    #[error("Decoding failed: need {required} symbols, have {received}")]
    InsufficientSymbols { required: usize, received: usize },

    /// Data corruption - could not recover.
    #[error("Data corruption: {0}")]
    DataCorruption(String),

    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// VFS not initialized.
    #[error("VFS not initialized in {0}")]
    NotInitialized(PathBuf),

    /// VFS already exists.
    #[error("VFS already exists in {0}")]
    AlreadyInitialized(PathBuf),

    /// Invalid VFS magic number.
    #[error("Invalid VFS format: expected magic 'SVFS'")]
    InvalidMagic,

    /// Version mismatch.
    #[error("VFS version mismatch: expected {expected}, found {found}")]
    VersionMismatch { expected: u32, found: u32 },
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Serialization(e.to_string())
    }
}

impl From<bincode::Error> for Error {
    fn from(e: bincode::Error) -> Self {
        Error::Serialization(e.to_string())
    }
}
