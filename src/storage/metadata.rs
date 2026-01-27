//! Persistent metadata for slack space storage.

use crate::error::Result;
use crate::storage::SymbolLocation;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Metadata file name (hidden file).
pub const METADATA_FILENAME: &str = ".slack_meta.json";

/// Metadata for the entire slack storage system.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SlackMetadata {
    /// Block size used for slack calculation.
    pub block_size: u64,
    /// Per-host metadata.
    pub hosts: HashMap<PathBuf, HostMetadata>,
    /// Next available symbol ID.
    pub next_symbol_id: u32,
    /// Salt for key derivation (stored here for decryption).
    #[serde(default)]
    pub salt: Option<[u8; 32]>,
}

impl SlackMetadata {
    /// Create new metadata.
    pub fn new(block_size: u64) -> Self {
        Self {
            block_size,
            hosts: HashMap::new(),
            next_symbol_id: 0,
            salt: None,
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

    /// Add a stored symbol.
    pub fn add_symbol(&mut self, location: SymbolLocation, vfs_file_id: u64) {
        let host_meta = self
            .hosts
            .entry(location.host_path.clone())
            .or_insert_with(|| HostMetadata {
                logical_size: 0, // Will be set by caller
                symbols: Vec::new(),
            });

        host_meta.symbols.push(StoredSymbol {
            symbol_id: location.symbol_id,
            offset: location.offset,
            length: location.length,
            vfs_file_id,
        });

        self.next_symbol_id = self.next_symbol_id.max(location.symbol_id + 1);
    }

    /// Get all symbols for a VFS file.
    pub fn get_symbols_for_file(&self, vfs_file_id: u64) -> Vec<(PathBuf, StoredSymbol)> {
        let mut result = Vec::new();

        for (path, host_meta) in &self.hosts {
            for symbol in &host_meta.symbols {
                if symbol.vfs_file_id == vfs_file_id {
                    result.push((path.clone(), symbol.clone()));
                }
            }
        }

        result
    }

    /// Remove all symbols for a VFS file.
    pub fn remove_symbols_for_file(&mut self, vfs_file_id: u64) {
        for host_meta in self.hosts.values_mut() {
            host_meta.symbols.retain(|s| s.vfs_file_id != vfs_file_id);
        }

        // Clean up empty hosts
        self.hosts.retain(|_, meta| !meta.symbols.is_empty());
    }

    /// Get total used slack per host.
    pub fn get_used_slack(&self, path: &Path) -> u64 {
        self.hosts
            .get(path)
            .map(|meta| meta.symbols.iter().map(|s| s.length as u64).sum())
            .unwrap_or(0)
    }

    /// Set logical size for a host.
    pub fn set_logical_size(&mut self, path: &Path, size: u64) {
        if let Some(meta) = self.hosts.get_mut(path) {
            meta.logical_size = size;
        } else {
            self.hosts.insert(
                path.to_path_buf(),
                HostMetadata {
                    logical_size: size,
                    symbols: Vec::new(),
                },
            );
        }
    }

    /// Get logical size for a host.
    pub fn get_logical_size(&self, path: &Path) -> Option<u64> {
        self.hosts.get(path).map(|meta| meta.logical_size)
    }

    /// Clear all metadata.
    pub fn clear(&mut self) {
        self.hosts.clear();
        self.next_symbol_id = 0;
    }

    /// Get a path from hosts for error messages.
    pub fn file_path_from_hosts(&self) -> PathBuf {
        self.hosts
            .keys()
            .next()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

/// Metadata for a single host file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostMetadata {
    /// Original logical size of the file.
    pub logical_size: u64,
    /// Symbols stored in this file's slack space.
    pub symbols: Vec<StoredSymbol>,
}

/// A symbol stored in slack space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSymbol {
    /// RaptorQ symbol ID.
    pub symbol_id: u32,
    /// Offset within slack space.
    pub offset: u64,
    /// Length of symbol data.
    pub length: u32,
    /// ID of the VFS file this symbol belongs to.
    pub vfs_file_id: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_save_and_load() {
        let dir = TempDir::new().unwrap();
        let mut meta = SlackMetadata::new(4096);

        meta.add_symbol(
            SymbolLocation {
                host_path: PathBuf::from("/test/file.txt"),
                offset: 0,
                symbol_id: 0,
                length: 1024,
            },
            1,
        );

        meta.save(dir.path()).unwrap();

        let loaded = SlackMetadata::load(dir.path()).unwrap();
        assert_eq!(loaded.block_size, 4096);
        assert_eq!(loaded.hosts.len(), 1);
    }

    #[test]
    fn test_get_symbols_for_file() {
        let mut meta = SlackMetadata::new(4096);

        // Add symbols for file 1
        meta.add_symbol(
            SymbolLocation {
                host_path: PathBuf::from("/test/a.txt"),
                offset: 0,
                symbol_id: 0,
                length: 100,
            },
            1,
        );
        meta.add_symbol(
            SymbolLocation {
                host_path: PathBuf::from("/test/b.txt"),
                offset: 0,
                symbol_id: 1,
                length: 100,
            },
            1,
        );

        // Add symbol for file 2
        meta.add_symbol(
            SymbolLocation {
                host_path: PathBuf::from("/test/a.txt"),
                offset: 100,
                symbol_id: 2,
                length: 100,
            },
            2,
        );

        let file1_symbols = meta.get_symbols_for_file(1);
        assert_eq!(file1_symbols.len(), 2);

        let file2_symbols = meta.get_symbols_for_file(2);
        assert_eq!(file2_symbols.len(), 1);
    }

    #[test]
    fn test_remove_symbols_for_file() {
        let mut meta = SlackMetadata::new(4096);

        meta.add_symbol(
            SymbolLocation {
                host_path: PathBuf::from("/test/file.txt"),
                offset: 0,
                symbol_id: 0,
                length: 100,
            },
            1,
        );
        meta.add_symbol(
            SymbolLocation {
                host_path: PathBuf::from("/test/file.txt"),
                offset: 100,
                symbol_id: 1,
                length: 100,
            },
            2,
        );

        meta.remove_symbols_for_file(1);

        let remaining = meta.get_symbols_for_file(2);
        assert_eq!(remaining.len(), 1);

        let removed = meta.get_symbols_for_file(1);
        assert_eq!(removed.len(), 0);
    }
}
