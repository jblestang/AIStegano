//! Slack Space Virtual File System
//!
//! A steganographic file system that stores encrypted data in file system slack space
//! with RaptorQ erasure coding for resilience against partial data loss.
//!
//! # Features
//!
//! - **Virtual File System**: Store multiple files and directories in hidden slack space
//! - **AES-256-GCM Encryption**: Secure authenticated encryption with Argon2id key derivation
//! - **RaptorQ Erasure Coding**: Recover data even when parts are overwritten
//! - **CLI Interface**: Easy-to-use command-line tool
//!
//! # Architecture
//!
//! ```text
//! Data → Encrypt (AES-256-GCM) → Encode (RaptorQ) → Store (Slack Space)
//! ```
//!
//! # Example
//!
//! ```rust,no_run
//! use slack_vfs::vfs::SlackVfs;
//! use std::path::Path;
//!
//! // Create a new VFS
//! let mut vfs = SlackVfs::create(
//!     Path::new("./host_dir"),
//!     "password",
//!     Default::default()
//! ).unwrap();
//!
//! // Write a file
//! vfs.create_file("/secret.txt", b"Hidden data").unwrap();
//!
//! // Read it back
//! let data = vfs.read_file("/secret.txt").unwrap();
//! assert_eq!(data, b"Hidden data");
//! ```

pub mod config;
pub mod crypto;
pub mod encoding;
pub mod error;
pub mod storage;
pub mod vfs;

pub use config::VfsConfig;
pub use error::{Error, Result};
pub use vfs::SlackVfs;
