//! VFS superblock - the root metadata structure.

use crate::config::{EncodingConfig, VfsConfig, VFS_MAGIC, VFS_VERSION};
use crate::error::{Error, Result};
use crate::vfs::types::{Inode, InodeId, ROOT_INODE_ID};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Allocation info for a single host file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostAllocation {
    /// Original logical size of the file (before hidden data).
    pub logical_size: u64,
    /// Total slack space used in this host.
    pub slack_used: u64,
}

/// A symbol stored in slack space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolAllocation {
    /// RaptorQ symbol ID.
    pub symbol_id: u32,
    /// Path to the host file.
    pub host_path: PathBuf,
    /// Offset within slack space.
    pub offset: u64,
    /// Length of symbol data.
    pub length: u32,
    /// ID of the VFS file this symbol belongs to.
    pub file_id: InodeId,
}

/// The superblock contains all VFS metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Superblock {
    /// Magic number for identification.
    pub magic: [u8; 4],
    /// VFS version.
    pub version: u32,
    /// Block size for slack calculation.
    pub block_size: u64,
    /// Redundancy ratio for encoding.
    pub redundancy_ratio: f32,
    /// Symbol size for encoding.
    pub symbol_size: u16,
    /// Root inode ID.
    pub root_inode: InodeId,
    /// Next available inode ID.
    pub next_inode_id: InodeId,
    /// All inodes indexed by ID.
    pub inodes: HashMap<InodeId, Inode>,
    /// Salt for password verification.
    /// Salt for password verification.
    pub salt: [u8; 32],
    /// Next available symbol ID.
    pub next_symbol_id: u32,
    /// Host file allocations (logical sizes and slack usage).
    pub hosts: HashMap<PathBuf, HostAllocation>,
    /// All symbol allocations.
    pub symbols: Vec<SymbolAllocation>,
    /// Sequence number for versioning (monotonically increasing).
    pub sequence_number: u64,
    /// Unique ID for this VFS instance.
    pub uuid: u128,
}

impl Superblock {
    /// Create a new superblock.
    pub fn new(config: &VfsConfig, salt: [u8; 32]) -> Self {
        let mut inodes = HashMap::new();
        inodes.insert(ROOT_INODE_ID, Inode::root());

        Self {
            magic: VFS_MAGIC,
            version: VFS_VERSION,
            block_size: config.block_size,
            redundancy_ratio: config.redundancy_ratio,
            symbol_size: config.symbol_size,
            root_inode: ROOT_INODE_ID,
            next_inode_id: 1,
            inodes,
            salt,
            next_symbol_id: 0,
            hosts: HashMap::new(),
            symbols: Vec::new(),
            sequence_number: 0,
            uuid: rand::random(),
        }
    }

    /// Validate the superblock.
    pub fn validate(&self) -> Result<()> {
        if self.magic != VFS_MAGIC {
            return Err(Error::InvalidMagic);
        }
        if self.version != VFS_VERSION {
            return Err(Error::VersionMismatch {
                expected: VFS_VERSION,
                found: self.version,
            });
        }
        Ok(())
    }

    /// Allocate a new inode ID.
    pub fn alloc_inode_id(&mut self) -> InodeId {
        let id = self.next_inode_id;
        self.next_inode_id += 1;
        id
    }

    /// Get an inode by ID.
    pub fn get_inode(&self, id: InodeId) -> Option<&Inode> {
        self.inodes.get(&id)
    }

    /// Get a mutable inode by ID.
    pub fn get_inode_mut(&mut self, id: InodeId) -> Option<&mut Inode> {
        self.inodes.get_mut(&id)
    }

    /// Insert an inode.
    pub fn insert_inode(&mut self, inode: Inode) {
        self.inodes.insert(inode.id, inode);
    }

    /// Remove an inode.
    pub fn remove_inode(&mut self, id: InodeId) -> Option<Inode> {
        self.inodes.remove(&id)
    }

    /// Get the root inode.
    pub fn root(&self) -> &Inode {
        self.inodes.get(&ROOT_INODE_ID).expect("Root inode missing")
    }

    /// Get the root inode mutably.
    pub fn root_mut(&mut self) -> &mut Inode {
        self.inodes
            .get_mut(&ROOT_INODE_ID)
            .expect("Root inode missing")
    }

    /// Get encoding config.
    pub fn encoding_config(&self) -> EncodingConfig {
        EncodingConfig {
            symbol_size: self.symbol_size,
            redundancy_ratio: self.redundancy_ratio,
        }
    }

    /// Serialize to bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        bincode::serialize(self).map_err(|e| Error::Serialization(e.to_string()))
    }

    /// Deserialize from bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let sb: Superblock =
            bincode::deserialize(data).map_err(|e| Error::Serialization(e.to_string()))?;
        sb.validate()?;
        Ok(sb)
    }

    /// Get total number of files.
    pub fn file_count(&self) -> usize {
        self.inodes.values().filter(|i| i.is_file()).count()
    }

    /// Get total number of directories.
    pub fn dir_count(&self) -> usize {
        self.inodes.values().filter(|i| i.is_directory()).count()
    }

    /// Get total size of all files.
    pub fn total_size(&self) -> u64 {
        self.inodes.values().map(|i| i.size).sum()
    }

    // ===== Symbol Management =====

    /// Allocate a new symbol ID.
    pub fn alloc_symbol_id(&mut self) -> u32 {
        let id = self.next_symbol_id;
        self.next_symbol_id += 1;
        id
    }

    /// Add a symbol allocation.
    pub fn add_symbol(&mut self, alloc: SymbolAllocation) {
        // Update host allocation
        let host = self
            .hosts
            .entry(alloc.host_path.clone())
            .or_insert(HostAllocation {
                logical_size: 0,
                slack_used: 0,
            });
        host.slack_used += alloc.length as u64;

        // Track next symbol ID
        self.next_symbol_id = self.next_symbol_id.max(alloc.symbol_id + 1);

        // Add to symbols list
        self.symbols.push(alloc);
    }

    /// Get all symbols for a specific file.
    pub fn get_symbols_for_file(&self, file_id: InodeId) -> Vec<&SymbolAllocation> {
        self.symbols
            .iter()
            .filter(|s| s.file_id == file_id)
            .collect()
    }

    /// Remove all symbols for a specific file.
    pub fn remove_symbols_for_file(&mut self, file_id: InodeId) {
        // Update host slack usage
        for symbol in self.symbols.iter().filter(|s| s.file_id == file_id) {
            if let Some(host) = self.hosts.get_mut(&symbol.host_path) {
                host.slack_used = host.slack_used.saturating_sub(symbol.length as u64);
            }
        }

        // Remove symbols
        self.symbols.retain(|s| s.file_id != file_id);

        // Clean up hosts with no symbols
        self.hosts
            .retain(|_, h| h.slack_used > 0 || h.logical_size > 0);
    }

    /// Get logical size for a host file.
    pub fn get_logical_size(&self, path: &std::path::Path) -> Option<u64> {
        self.hosts.get(path).map(|h| h.logical_size)
    }

    /// Set logical size for a host file.
    pub fn set_logical_size(&mut self, path: &std::path::Path, size: u64) {
        let host = self
            .hosts
            .entry(path.to_path_buf())
            .or_insert(HostAllocation {
                logical_size: 0,
                slack_used: 0,
            });
        host.logical_size = size;
    }

    /// Get used slack for a host file.
    pub fn get_used_slack(&self, path: &std::path::Path) -> u64 {
        self.hosts.get(path).map(|h| h.slack_used).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_superblock() {
        let config = VfsConfig::default();
        let salt = [0u8; 32];
        let sb = Superblock::new(&config, salt);

        assert_eq!(sb.magic, VFS_MAGIC);
        assert_eq!(sb.version, VFS_VERSION);
        assert!(sb.inodes.contains_key(&ROOT_INODE_ID));
    }

    #[test]
    fn test_alloc_inode_id() {
        let config = VfsConfig::default();
        let salt = [0u8; 32];
        let mut sb = Superblock::new(&config, salt);

        let id1 = sb.alloc_inode_id();
        let id2 = sb.alloc_inode_id();

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
    }

    #[test]
    fn test_serialize_deserialize() {
        let config = VfsConfig::default();
        let salt = [42u8; 32];
        let mut sb = Superblock::new(&config, salt);

        // Add some inodes
        let file = Inode::new_file(sb.alloc_inode_id(), "test.txt".to_string(), 100);
        sb.insert_inode(file);

        let bytes = sb.to_bytes().unwrap();
        let restored = Superblock::from_bytes(&bytes).unwrap();

        assert_eq!(restored.salt, salt);
        assert_eq!(restored.inodes.len(), sb.inodes.len());
    }

    #[test]
    fn test_validate_bad_magic() {
        let config = VfsConfig::default();
        let salt = [0u8; 32];
        let mut sb = Superblock::new(&config, salt);
        sb.magic = [0, 0, 0, 0];

        assert!(matches!(sb.validate(), Err(Error::InvalidMagic)));
    }
}
