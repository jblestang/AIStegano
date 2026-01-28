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
use crate::vfs::types::EncodingInfo;
use crate::storage::host_manager::SymbolLocation;

/// Metadata file name (hidden file).
pub const METADATA_FILENAME: &str = ".slack_meta.json";

/// Current metadata version.
pub const METADATA_VERSION: u32 = 3;

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
    
    /// Encoding parameters for the superblock (RaptorQ).
    pub superblock_encoding: Option<EncodingInfo>,
    
    /// Locations of superblock symbols (distributed across hosts).
    #[serde(default)]
    pub superblock_symbols: Vec<SymbolLocation>,
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
            superblock_encoding: None,
            superblock_symbols: Vec::new(),
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
            superblock_encoding: None,
            superblock_symbols: Vec::new(),
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
    /// Save metadata to a directory securely (atomic update).
    pub fn save(&self, dir: &Path) -> Result<()> {
        let path = Self::file_path(dir);
        let tmp_path = path.with_extension("tmp");
        
        let content = serde_json::to_string_pretty(self)?;
        
        // Write to temp file first
        std::fs::write(&tmp_path, content)?;
        
        // Atomically rename
        std::fs::rename(tmp_path, path)?;
        Ok(())
    }

    /// Check if the VFS is initialized (has salt and at least one superblock).
    pub fn is_initialized(&self) -> bool {
        self.salt.is_some() && !self.superblock_symbols.is_empty()
    }

    /// Clear the metadata (reset to empty state).
    pub fn clear(&mut self) {
        self.salt = None;
        self.superblock_encoding = None;
        self.superblock_symbols.clear();
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
