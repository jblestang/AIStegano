//! Host file manager for tracking and allocating slack space.

use crate::error::{Error, Result};
use crate::storage::slack::get_slack_capacity;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Location of a stored symbol in slack space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolLocation {
    /// Path to the host file.
    pub host_path: PathBuf,
    /// Offset within the slack space (relative to logical_size).
    pub offset: u64,
    /// RaptorQ symbol ID.
    pub symbol_id: u32,
    /// Length of the symbol data.
    pub length: u32,
}

/// Information about a single host file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostFile {
    /// Path to the file.
    pub path: PathBuf,
    /// Original logical size of the file.
    pub logical_size: u64,
    /// Total slack capacity available.
    pub slack_capacity: u64,
    /// Amount of slack space currently used.
    pub used_slack: u64,
}

impl HostFile {
    /// Create a new HostFile from a path.
    pub fn new(path: PathBuf, block_size: u64) -> Result<Self> {
        let metadata = std::fs::metadata(&path)?;
        let logical_size = metadata.len();
        let slack_capacity = if logical_size == 0 {
            0
        } else {
            get_slack_capacity(&path, block_size)?
        };

        Ok(Self {
            path,
            logical_size,
            slack_capacity,
            used_slack: 0,
        })
    }

    /// Get available slack space.
    pub fn available(&self) -> u64 {
        self.slack_capacity.saturating_sub(self.used_slack)
    }

    /// Check if this host can accommodate data of given size.
    pub fn can_fit(&self, size: u64) -> bool {
        self.available() >= size
    }

    /// Allocate space for a symbol.
    pub fn allocate(&mut self, size: u64) -> Option<u64> {
        if self.can_fit(size) {
            let offset = self.used_slack;
            self.used_slack += size;
            Some(offset)
        } else {
            None
        }
    }

    /// Get the write position for a given offset within slack space.
    pub fn get_write_position(&self, offset: u64) -> u64 {
        self.logical_size + offset
    }
}

/// Manager for a collection of host files.
#[derive(Debug)]
pub struct HostManager {
    /// Root directory containing host files.
    root_dir: PathBuf,
    /// All tracked host files.
    hosts: Vec<HostFile>,
    /// Block size for slack calculation.
    block_size: u64,
}

impl HostManager {
    /// Scan a directory for files that can be used as hosts.
    ///
    /// Skips hidden files, the metadata file, and empty files.
    pub fn scan(root: &Path, block_size: u64) -> Result<Self> {
        if !root.exists() {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Directory not found: {}", root.display()),
            )));
        }

        let mut hosts = Vec::new();

        for entry in WalkDir::new(root)
            .min_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Skip directories
            if path.is_dir() {
                continue;
            }

            // Skip hidden files and our metadata file
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') {
                    continue;
                }
            }

            // Try to create a host file
            if let Ok(host) = HostFile::new(path.to_path_buf(), block_size) {
                // Only include files with slack space
                if host.slack_capacity > 0 {
                    hosts.push(host);
                }
            }
        }

        Ok(Self {
            root_dir: root.to_path_buf(),
            hosts,
            block_size,
        })
    }

    /// Get the root directory.
    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    /// Get block size.
    pub fn block_size(&self) -> u64 {
        self.block_size
    }

    /// Get all host files.
    pub fn hosts(&self) -> &[HostFile] {
        &self.hosts
    }

    /// Get mutable reference to hosts.
    pub fn hosts_mut(&mut self) -> &mut Vec<HostFile> {
        &mut self.hosts
    }

    /// Get total available slack space across all hosts.
    pub fn total_available(&self) -> u64 {
        self.hosts.iter().map(|h| h.available()).sum()
    }

    /// Get total slack capacity (before any allocations).
    pub fn total_capacity(&self) -> u64 {
        self.hosts.iter().map(|h| h.slack_capacity).sum()
    }

    /// Get total used slack space.
    pub fn total_used(&self) -> u64 {
        self.hosts.iter().map(|h| h.used_slack).sum()
    }

    /// Get number of host files.
    pub fn host_count(&self) -> usize {
        self.hosts.len()
    }

    /// Get a host file by path.
    pub fn get_host(&self, path: &Path) -> Option<&HostFile> {
        self.hosts.iter().find(|h| h.path == path)
    }

    /// Get a mutable host file by path.
    pub fn get_host_mut(&mut self, path: &Path) -> Option<&mut HostFile> {
        self.hosts.iter_mut().find(|h| h.path == path)
    }

    /// Allocate space for symbols of given size.
    ///
    /// Returns locations for each symbol, distributed across hosts.
    pub fn allocate(
        &mut self,
        symbol_count: usize,
        symbol_size: usize,
        start_symbol_id: u32,
    ) -> Result<Vec<SymbolLocation>> {
        let total_needed = symbol_count as u64 * symbol_size as u64;
        let available = self.total_available();

        if total_needed > available {
            return Err(Error::InsufficientSpace {
                needed: total_needed,
                available,
            });
        }

        let mut locations = Vec::with_capacity(symbol_count);
        let mut symbol_id = start_symbol_id;
        let mut remaining = symbol_count;

        // Distribute symbols across hosts
        for host in &mut self.hosts {
            while remaining > 0 && host.can_fit(symbol_size as u64) {
                if let Some(offset) = host.allocate(symbol_size as u64) {
                    locations.push(SymbolLocation {
                        host_path: host.path.clone(),
                        offset,
                        symbol_id,
                        length: symbol_size as u32,
                    });
                    symbol_id += 1;
                    remaining -= 1;
                } else {
                    break;
                }
            }
        }

        if locations.len() < symbol_count {
            return Err(Error::InsufficientSpace {
                needed: total_needed,
                available,
            });
        }

        Ok(locations)
    }

    /// Update host info after loading metadata.
    pub fn apply_used_slack(&mut self, path: &Path, used: u64) {
        if let Some(host) = self.hosts.iter_mut().find(|h| h.path == path) {
            host.used_slack = used;
        }
    }

    /// Reset all allocations.
    pub fn reset_allocations(&mut self) {
        for host in &mut self.hosts {
            host.used_slack = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_dir_with_files() -> TempDir {
        let dir = TempDir::new().unwrap();

        // Create files of various sizes
        for i in 0..5 {
            let path = dir.path().join(format!("file{}.txt", i));
            let mut f = std::fs::File::create(&path).unwrap();
            // Write different amounts to each file
            let content = vec![b'A'; 1000 + i * 500];
            f.write_all(&content).unwrap();
        }

        dir
    }

    #[test]
    fn test_scan_directory() {
        let dir = create_test_dir_with_files();
        let manager = HostManager::scan(dir.path(), 4096).unwrap();

        assert_eq!(manager.host_count(), 5);
        assert!(manager.total_capacity() > 0);
    }

    #[test]
    fn test_host_file_available() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, vec![0u8; 1000]).unwrap();

        let host = HostFile::new(path, 4096).unwrap();

        // 4096 - 1000 = 3096 slack
        assert_eq!(host.slack_capacity, 3096);
        assert_eq!(host.available(), 3096);
    }

    #[test]
    fn test_allocate_symbols() {
        let dir = create_test_dir_with_files();
        let mut manager = HostManager::scan(dir.path(), 4096).unwrap();

        let initial_available = manager.total_available();

        // Allocate 10 symbols of 100 bytes each
        let locations = manager.allocate(10, 100, 0).unwrap();

        assert_eq!(locations.len(), 10);
        assert_eq!(manager.total_available(), initial_available - 1000);
    }

    #[test]
    fn test_allocate_insufficient_space() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("small.txt");
        std::fs::write(&path, vec![0u8; 100]).unwrap();

        let mut manager = HostManager::scan(dir.path(), 4096).unwrap();

        // Try to allocate more than available
        let result = manager.allocate(100, 1000, 0);

        assert!(matches!(result, Err(Error::InsufficientSpace { .. })));
    }
}
