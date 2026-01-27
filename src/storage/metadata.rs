//! Minimal bootstrap metadata for slack space storage.
//!
//! This file contains only the essential data needed before decryption:
//! - Salt for key derivation
//! - Block size for slack calculation
//! - Superblock location (to bootstrap decryption)
//!
//! All other sensitive data (file mappings, symbol locations) is stored in the
//! encrypted superblock.

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Metadata file name (hidden file).
pub const METADATA_FILENAME: &str = ".slack_meta.json";

/// Current metadata version.
pub const METADATA_VERSION: u32 = 2;

/// Location of the superblock in slack space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuperblockLocation {
    /// Host file containing the superblock.
    pub host_path: PathBuf,
    /// Absolute offset in the host file where superblock begins.
    pub offset: u64,
    /// Length of the encrypted superblock data.
    pub length: u32,
}

/// Minimal bootstrap metadata - only contains data needed before decryption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackMetadata {
    /// Metadata format version.
    #[serde(default = "default_version")]
    pub version: u32,
    /// Block size used for slack calculation.
    pub block_size: u64,
    /// Salt for key derivation (required for decryption).
    pub salt: Option<[u8; 32]>,
    /// Locations of encrypted superblocks (replicas).
    #[serde(default)]
    pub superblocks: Vec<SuperblockLocation>,
}

fn default_version() -> u32 {
    METADATA_VERSION
}

impl Default for SlackMetadata {
    fn default() -> Self {
        Self {
            version: METADATA_VERSION,
            block_size: 4096,
            salt: None,
            superblocks: Vec::new(),
        }
    }
}

impl SlackMetadata {
    /// Create new metadata.
    pub fn new(block_size: u64) -> Self {
        Self {
            version: METADATA_VERSION,
            block_size,
            salt: None,
            superblocks: Vec::new(),
        }
    }

    /// Get the metadata file path for a directory.
    pub fn file_path(dir: &Path) -> PathBuf {
        dir.join(METADATA_FILENAME)
    }

    /// Load metadata from a directory.
    pub fn load(dir: &Path) -> Result<Self> {
        let path = Self::file_path(dir);
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)?;
        let metadata: SlackMetadata = serde_json::from_str(&content)?;
        Ok(metadata)
    }

    /// Save metadata to a directory.
    pub fn save(&self, dir: &Path) -> Result<()> {
        let path = Self::file_path(dir);
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Check if the VFS is initialized (has salt and at least one superblock).
    pub fn is_initialized(&self) -> bool {
        self.salt.is_some() && !self.superblocks.is_empty()
    }

    /// Clear the metadata (reset to empty state).
    pub fn clear(&mut self) {
        self.salt = None;
        self.superblocks.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_save_and_load() {
        let dir = TempDir::new().unwrap();
        let mut meta = SlackMetadata::new(4096);
        meta.salt = Some([42u8; 32]);

        meta.save(dir.path()).unwrap();

        let loaded = SlackMetadata::load(dir.path()).unwrap();
        assert_eq!(loaded.version, METADATA_VERSION);
        assert_eq!(loaded.block_size, 4096);
        assert_eq!(loaded.salt, Some([42u8; 32]));
    }

    #[test]
    fn test_minimal_json() {
        let meta = SlackMetadata::new(4096);
        let json = serde_json::to_string_pretty(&meta).unwrap();

        // Should be very small - just version, block_size, and salt
        assert!(json.len() < 200, "JSON should be minimal, got: {}", json);
    }
}
