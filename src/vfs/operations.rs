//! VFS operations - the main interface.

use crate::config::VfsConfig;
use crate::crypto::{
    decrypt_data, decrypt_with_key, encrypt_data, encrypt_with_key, EncryptedData, KeyDerivation,
};
use crate::encoding::{decode, encode, EncodedData, EncodingSymbol};
use crate::error::{Error, Result};
use crate::storage::{
    read_slack, wipe_slack, write_slack, HostManager, SlackMetadata,
};
use crate::vfs::path::VfsPath;
use crate::vfs::superblock::{Superblock, SymbolAllocation};
use crate::vfs::types::{DirEntry, EncodingInfo, Inode, InodeId, ROOT_INODE_ID};
use std::path::{Path, PathBuf};

/// Health report for the VFS.
#[derive(Debug, Clone)]
pub struct HealthReport {
    /// Total number of files.
    pub total_files: usize,
    /// Number of files that can be recovered.
    pub recoverable_files: usize,
    /// Files with damage (path, percent symbols lost).
    pub damaged_files: Vec<(String, f32)>,
    /// Total slack capacity.
    pub total_capacity: u64,
    /// Used slack capacity.
    pub used_capacity: u64,
    /// Number of host files.
    pub host_count: usize,
}

/// The main Slack VFS interface.
pub struct SlackVfs {
    /// VFS superblock.
    superblock: Superblock,
    /// Host file manager.
    host_manager: HostManager,
    /// Slack space metadata.
    metadata: SlackMetadata,
    /// Encryption key derived from password.
    key: [u8; 32],
    /// Root directory of host files.
    host_dir: PathBuf,
    /// Whether there are unsaved changes.
    dirty: bool,
}

impl SlackVfs {
    /// Create a new VFS in the given directory.
    ///
    /// # Arguments
    ///
    /// * `host_dir` - Directory containing host files
    /// * `password` - Password for encryption
    /// * `config` - VFS configuration
    pub fn create(host_dir: &Path, password: &str, config: VfsConfig) -> Result<Self> {
        config.validate().map_err(Error::InvalidPath)?;

        // Check if VFS already exists
        let meta_path = SlackMetadata::file_path(host_dir);
        if meta_path.exists() {
            return Err(Error::AlreadyInitialized(host_dir.to_path_buf()));
        }

        // Scan for host files
        let host_manager = HostManager::scan(host_dir, config.block_size)?;
        if host_manager.host_count() == 0 {
            return Err(Error::NoHostFiles(host_dir.to_path_buf()));
        }

        // Create key derivation with random salt
        let kdf = KeyDerivation::new();
        let key = kdf.derive_key(password)?;

        // Create superblock
        let superblock = Superblock::new(&config, *kdf.salt());

        // Create metadata with salt for later decryption
        let mut metadata = SlackMetadata::new(config.block_size);
        metadata.salt = Some(*kdf.salt());

        let mut vfs = Self {
            superblock,
            host_manager,
            metadata,
            key,
            host_dir: host_dir.to_path_buf(),
            dirty: true,
        };

        // Save initial state
        vfs.sync()?;

        Ok(vfs)
    }

    /// Mount an existing VFS.
    ///
    /// # Arguments
    ///
    /// * `host_dir` - Directory containing host files
    /// * `password` - Password for decryption
    pub fn mount(host_dir: &Path, password: &str) -> Result<Self> {
        // Load metadata
        let metadata = SlackMetadata::load(host_dir)?;
        if !metadata.is_initialized() {
            return Err(Error::NotInitialized(host_dir.to_path_buf()));
        }

        // Scan host files
        let mut host_manager = HostManager::scan(host_dir, metadata.block_size)?;

        // Read and decrypt superblock
        let superblock = Self::read_superblock(&metadata, Some(&host_manager), password)?;

        // Apply used slack from superblock (Files)
        for (path, host_alloc) in &superblock.hosts {
            host_manager.apply_used_slack(path, host_alloc.slack_used);
        }
        
        // Apply used slack from metadata (Superblock itself)
        // Note: apply_used_slack overwrites? No, it sets used_slack.
        // We need to ADD.
        // host_manager.apply_used_slack only sets.
        // We should probably rely on `clean usage` -> `apply files` -> `add superblock`.
        // Or better: HostManager tracks TOTAL usage.
        // But `Superblock.hosts` only tracks FILE usage now (per my design).
        // So we need to ADD simple logic here.
        
        for loc in &metadata.superblock_symbols {
             if let Some(host) = host_manager.get_host_mut(&loc.host_path) {
                 host.used_slack += loc.length as u64;
             }
        }

        // Derive key
        let kdf = KeyDerivation::from_salt(superblock.salt);
        let key = kdf.derive_key(password)?;

        Ok(Self {
            superblock,
            host_manager,
            metadata,
            key,
            host_dir: host_dir.to_path_buf(),
            dirty: false,
        })
    }

    /// Read and decrypt the superblock from slack space.
    fn read_superblock(
        metadata: &SlackMetadata,
        _host_manager: Option<&HostManager>,
        password: &str,
    ) -> Result<Superblock> {
        let salt = metadata
            .salt
            .ok_or_else(|| Error::DataCorruption("Missing salt in metadata".to_string()))?;

        let kdf = KeyDerivation::from_salt(salt);
        let key = kdf.derive_key(password)?;

        if let Some(encoding_info) = &metadata.superblock_encoding {
            // Collect symbols
            let mut symbols = Vec::new();
            
            for loc in &metadata.superblock_symbols {
                // Try read
                // We use loc.length (u32)
                if let Ok(data) = read_slack(&loc.host_path, loc.offset, loc.length as usize) {
                    symbols.push(EncodingSymbol {
                         id: loc.symbol_id,
                         data,
                    });
                }
            }

            // Construct EcodedData
            let encoded = EncodedData {
                original_length: encoding_info.original_length,
                source_symbols: encoding_info.source_symbols,
                repair_symbols: encoding_info.repair_symbols,
                symbol_size: encoding_info.symbol_size,
                symbols,
            };

            // Decode
            let encrypted_bytes = decode(&encoded).map_err(|_| Error::DataCorruption("Insufficient superblock symbols".to_string()))?;
            
            // Decrypt
            let plaintext = decrypt_with_key(&encrypted_bytes, &key)?;
            
            Superblock::from_bytes(&plaintext)

        } else {
             Err(Error::DataCorruption("Missing superblock encoding info (legacy format not supported)".to_string()))
        }
    }

    /// Collect all symbols for a file from slack space using superblock.
    fn collect_file_symbols(&self, file_id: InodeId) -> Result<Vec<EncodingSymbol>> {
        let stored_symbols = self.superblock.get_symbols_for_file(file_id);
        let mut symbols = Vec::new();

        for alloc in stored_symbols {
            // Get logical size for this host
            let logical_size = self
                .superblock
                .get_logical_size(&alloc.host_path)
                .unwrap_or(0);

            // Read symbol data from slack
            let data = read_slack(
                &alloc.host_path,
                logical_size + alloc.offset,
                alloc.length as usize,
            )?;

            symbols.push(EncodingSymbol {
                id: alloc.symbol_id,
                data,
            });
        }

        Ok(symbols)
    }

    /// Write the superblock to slack space.
    fn write_superblock(&mut self) -> Result<()> {
        self.superblock.sequence_number += 1;

        // Clean internal host allocation tracking (rebuild from file symbols)
        // This ensures the superblock doesn't track its *own* old location, allowing CoW.
        for host in self.superblock.hosts.values_mut() {
            host.slack_used = 0;
        }
        for sym in &self.superblock.symbols {
             // Get the correct logical_size from host_manager
             let logical_size = self.host_manager.get_host(&sym.host_path)
                 .map(|h| h.logical_size)
                 .unwrap_or(0);
             
             let host = self.superblock.hosts.entry(sym.host_path.clone())
                 .or_insert(crate::vfs::superblock::HostAllocation { logical_size, slack_used: 0 });
             // Use high-water mark: next free offset is max(current_used, offset + length)
             // This correctly handles non-contiguous allocations
             host.slack_used = host.slack_used.max(sym.offset + sym.length as u64);
        }

        // Serialize
        let sb_bytes = self.superblock.to_bytes()?;
        
        // Encrypt
        let encrypted = encrypt_with_key(&sb_bytes, &self.key)?;
        
        // Encode
        let config = self.superblock.encoding_config();
        let encoded = encode(&encrypted, &config)?;
        
        // Allocate space for symbols
        let mut locations = self.host_manager.allocate(
            encoded.symbols.len(),
            encoded.symbol_size as usize,
            0 // Start ID
        )?;
        
        // Write symbols
        for (i, loc) in locations.iter_mut().enumerate() {
            let symbol = &encoded.symbols[i];
            
            // Get logical size for absolute offset
            let host_logical_size = self.host_manager.get_host(&loc.host_path)
                .map(|h| h.logical_size)
                .ok_or_else(|| Error::DataCorruption("Allocated on missing host".to_string()))?;

            let absolute_offset = host_logical_size + loc.offset;
            write_slack(&loc.host_path, &symbol.data, absolute_offset)?;
            
            // UPDATE to absolute offset for metadata persistence
            loc.offset = absolute_offset;
        }
        
        // Update Metadata
        self.metadata.superblock_encoding = Some(EncodingInfo {
            original_length: encoded.original_length,
            source_symbols: encoded.source_symbols,
            repair_symbols: encoded.repair_symbols,
            symbol_size: encoded.symbol_size,
        });
        self.metadata.superblock_symbols = locations;
        
        // Atomic Save
        self.metadata.save(&self.host_dir)?;
        
        // Sync HostManager to reflect New SB Usage (Files + New SB)
        // 1. Apply Files
        for (path, host_alloc) in &self.superblock.hosts {
            self.host_manager.apply_used_slack(path, host_alloc.slack_used);
        }
        // 2. Add New SB
        for loc in &self.metadata.superblock_symbols {
             if let Some(host) = self.host_manager.get_host_mut(&loc.host_path) {
                 host.used_slack += loc.length as u64;
             }
        }

        Ok(())
    }

    /// Sync all changes to disk.
    pub fn sync(&mut self) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }

        // Write superblock
        self.write_superblock()?;

        // Save metadata
        self.metadata.save(&self.host_dir)?;

        self.dirty = false;
        Ok(())
    }

    /// Resolve a path to an inode ID.
    fn resolve_path(&self, path: &VfsPath) -> Result<InodeId> {
        let mut current_id = ROOT_INODE_ID;

        for component in path.components() {
            let current = self
                .superblock
                .get_inode(current_id)
                .ok_or_else(|| Error::FileNotFound(path.to_string()))?;

            let children = current
                .children()
                .ok_or_else(|| Error::NotADirectory(path.to_string()))?;

            let mut found = false;
            for &child_id in children {
                if let Some(child) = self.superblock.get_inode(child_id) {
                    if child.name == *component {
                        current_id = child_id;
                        found = true;
                        break;
                    }
                }
            }

            if !found {
                return Err(Error::FileNotFound(path.to_string()));
            }
        }

        Ok(current_id)
    }

    /// Create a file in the VFS.
    pub fn create_file(&mut self, path: &str, data: &[u8]) -> Result<InodeId> {
        let vfs_path = VfsPath::parse(path)?;

        if vfs_path.is_root() {
            return Err(Error::InvalidPath("Cannot create file at root".to_string()));
        }

        // Check parent exists and is a directory
        let parent_path = vfs_path.parent().unwrap();
        let parent_id = self.resolve_path(&parent_path)?;

        let parent = self
            .superblock
            .get_inode(parent_id)
            .ok_or_else(|| Error::FileNotFound(parent_path.to_string()))?;

        if !parent.is_directory() {
            return Err(Error::NotADirectory(parent_path.to_string()));
        }

        // Check file doesn't already exist
        let name = vfs_path.name().unwrap();
        for &child_id in parent.children().unwrap() {
            if let Some(child) = self.superblock.get_inode(child_id) {
                if child.name == name {
                    return Err(Error::PathExists(path.to_string()));
                }
            }
        }

        // Encrypt the data
        let encrypted = encrypt_data(data, &hex::encode(self.key))?;
        let encrypted_bytes =
            bincode::serialize(&encrypted).map_err(|e| Error::Serialization(e.to_string()))?;

        // Encode with RaptorQ
        let config = self.superblock.encoding_config();
        let encoded = encode(&encrypted_bytes, &config)?;

        // Allocate space and store symbols
        let inode_id = self.superblock.alloc_inode_id();

        // Store each symbol
        for symbol in &encoded.symbols {
            self.store_symbol(symbol, inode_id)?;
        }

        // Create inode
        let mut inode = Inode::new_file(inode_id, name.to_string(), data.len() as u64);
        inode.symbol_ids = encoded.symbols.iter().map(|s| s.id).collect();
        inode.encoding_info = Some(EncodingInfo {
            original_length: encoded.original_length,
            source_symbols: encoded.source_symbols,
            repair_symbols: encoded.repair_symbols,
            symbol_size: encoded.symbol_size,
        });

        // Add to parent
        self.superblock
            .get_inode_mut(parent_id)
            .unwrap()
            .add_child(inode_id);

        // Insert inode
        self.superblock.insert_inode(inode);

        self.dirty = true;
        self.sync()?;

        Ok(inode_id)
    }

    /// Store a single symbol in slack space.
    fn store_symbol(&mut self, symbol: &EncodingSymbol, file_id: InodeId) -> Result<()> {
        // Find a host with enough space
        for host in self.host_manager.hosts_mut() {
            if host.can_fit(symbol.data.len() as u64) {
                let offset = host.allocate(symbol.data.len() as u64).unwrap();

                // Write to slack
                write_slack(&host.path, &symbol.data, host.logical_size + offset)?;

                // Record in superblock
                self.superblock.add_symbol(SymbolAllocation {
                    symbol_id: symbol.id,
                    host_path: host.path.clone(),
                    offset,
                    length: symbol.data.len() as u32,
                    file_id,
                });

                self.superblock
                    .set_logical_size(&host.path, host.logical_size);

                return Ok(());
            }
        }

        Err(Error::InsufficientSpace {
            needed: symbol.data.len() as u64,
            available: self.host_manager.total_available(),
        })
    }

    /// Read a file from the VFS.
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let vfs_path = VfsPath::parse(path)?;
        let inode_id = self.resolve_path(&vfs_path)?;

        let inode = self
            .superblock
            .get_inode(inode_id)
            .ok_or_else(|| Error::FileNotFound(path.to_string()))?;

        if !inode.is_file() {
            return Err(Error::NotAFile(path.to_string()));
        }

        let encoding_info = inode
            .encoding_info
            .as_ref()
            .ok_or_else(|| Error::DataCorruption("Missing encoding info".to_string()))?;

        // Collect symbols
        let symbols = self.collect_file_symbols(inode_id)?;

        // Create EncodedData for decoding
        let encoded = EncodedData {
            original_length: encoding_info.original_length,
            source_symbols: encoding_info.source_symbols,
            repair_symbols: encoding_info.repair_symbols,
            symbol_size: encoding_info.symbol_size,
            symbols,
        };

        // Decode
        let encrypted_bytes = decode(&encoded)?;

        // Deserialize encrypted data
        let encrypted: EncryptedData = bincode::deserialize(&encrypted_bytes)
            .map_err(|e| Error::Serialization(e.to_string()))?;

        // Decrypt
        decrypt_data(&encrypted, &hex::encode(self.key))
    }

    /// Delete a file from the VFS.
    pub fn delete_file(&mut self, path: &str) -> Result<()> {
        let vfs_path = VfsPath::parse(path)?;

        if vfs_path.is_root() {
            return Err(Error::InvalidPath("Cannot delete root".to_string()));
        }

        let inode_id = self.resolve_path(&vfs_path)?;

        let inode = self
            .superblock
            .get_inode(inode_id)
            .ok_or_else(|| Error::FileNotFound(path.to_string()))?;

        if !inode.is_file() {
            return Err(Error::NotAFile(path.to_string()));
        }

        // Remove from parent
        let parent_path = vfs_path.parent().unwrap();
        let parent_id = self.resolve_path(&parent_path)?;

        self.superblock
            .get_inode_mut(parent_id)
            .unwrap()
            .remove_child(inode_id);

        // Remove symbols from superblock (updating host allocations)
        self.superblock.remove_symbols_for_file(inode_id);

        // Remove inode
        self.superblock.remove_inode(inode_id);

        self.dirty = true;
        self.sync()?;

        Ok(())
    }

    /// List directory contents.
    pub fn list_dir(&self, path: &str) -> Result<Vec<DirEntry>> {
        let vfs_path = VfsPath::parse(path)?;
        let inode_id = self.resolve_path(&vfs_path)?;

        let inode = self
            .superblock
            .get_inode(inode_id)
            .ok_or_else(|| Error::FileNotFound(path.to_string()))?;

        let children = inode
            .children()
            .ok_or_else(|| Error::NotADirectory(path.to_string()))?;

        let mut entries = Vec::new();
        for &child_id in children {
            if let Some(child) = self.superblock.get_inode(child_id) {
                entries.push(DirEntry::from_inode(child));
            }
        }

        // Sort by name
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(entries)
    }

    /// Create a directory.
    pub fn create_dir(&mut self, path: &str) -> Result<InodeId> {
        let vfs_path = VfsPath::parse(path)?;

        if vfs_path.is_root() {
            return Err(Error::PathExists("/".to_string()));
        }

        // Check parent exists
        let parent_path = vfs_path.parent().unwrap();
        let parent_id = self.resolve_path(&parent_path)?;

        let parent = self
            .superblock
            .get_inode(parent_id)
            .ok_or_else(|| Error::FileNotFound(parent_path.to_string()))?;

        if !parent.is_directory() {
            return Err(Error::NotADirectory(parent_path.to_string()));
        }

        // Check doesn't exist
        let name = vfs_path.name().unwrap();
        for &child_id in parent.children().unwrap() {
            if let Some(child) = self.superblock.get_inode(child_id) {
                if child.name == name {
                    return Err(Error::PathExists(path.to_string()));
                }
            }
        }

        // Create inode
        let inode_id = self.superblock.alloc_inode_id();
        let inode = Inode::new_directory(inode_id, name.to_string());

        // Add to parent
        self.superblock
            .get_inode_mut(parent_id)
            .unwrap()
            .add_child(inode_id);

        self.superblock.insert_inode(inode);

        self.dirty = true;
        self.sync()?;

        Ok(inode_id)
    }

    /// Get file or directory info.
    pub fn stat(&self, path: &str) -> Result<Inode> {
        let vfs_path = VfsPath::parse(path)?;
        let inode_id = self.resolve_path(&vfs_path)?;

        self.superblock
            .get_inode(inode_id)
            .cloned()
            .ok_or_else(|| Error::FileNotFound(path.to_string()))
    }

    /// Get VFS health report.
    pub fn health_check(&self) -> Result<HealthReport> {
        let mut total_files = 0;
        let mut recoverable_files = 0;
        let mut damaged_files = Vec::new();

        for inode in self.superblock.inodes.values() {
            if inode.is_file() {
                total_files += 1;

                if let Some(encoding_info) = &inode.encoding_info {
                    // Count available symbols
                    let symbols = self.collect_file_symbols(inode.id)?;

                    let available = symbols.len();
                    let required = encoding_info.source_symbols;

                    if available >= required {
                        recoverable_files += 1;
                    } else {
                        let loss_percent = (1.0 - available as f32 / required as f32) * 100.0;
                        // Find path for this file (simplified - just use name)
                        damaged_files.push((inode.name.clone(), loss_percent));
                    }
                }
            }
        }

        Ok(HealthReport {
            total_files,
            recoverable_files,
            damaged_files,
            total_capacity: self.host_manager.total_capacity(),
            used_capacity: self.host_manager.total_used(),
            host_count: self.host_manager.host_count(),
        })
    }

    /// Change the VFS password.
    pub fn change_password(&mut self, old_password: &str, new_password: &str) -> Result<()> {
        // Verify old password
        let kdf = KeyDerivation::from_salt(self.superblock.salt);
        let old_key = kdf.derive_key(old_password)?;

        if old_key != self.key {
            return Err(Error::Decryption);
        }

        // Generate new salt and key
        let new_kdf = KeyDerivation::new();
        let new_key = new_kdf.derive_key(new_password)?;

        // Update superblock
        self.superblock.salt = *new_kdf.salt();
        self.key = new_key;

        self.dirty = true;
        self.sync()?;

        Ok(())
    }

    /// Securely wipe all VFS data.
    pub fn wipe(&mut self) -> Result<()> {
        // Wipe all host files' slack space
        for host in self.host_manager.hosts() {
            if let Some(logical_size) = self.superblock.get_logical_size(&host.path) {
                wipe_slack(&host.path, logical_size, None)?;
            }
        }

        // Delete the metadata file completely
        let metadata_path = self.host_dir.join(".slack_meta.json");
        if metadata_path.exists() {
            std::fs::remove_file(&metadata_path)?;
        }

        // Clear in-memory metadata
        self.metadata.clear();

        Ok(())
    }

    /// Get VFS info.
    pub fn info(&self) -> VfsInfo {
        VfsInfo {
            host_dir: self.host_dir.clone(),
            host_count: self.host_manager.host_count(),
            total_capacity: self.host_manager.total_capacity(),
            used_capacity: self.host_manager.total_used(),
            available_capacity: self.host_manager.total_available(),
            file_count: self.superblock.file_count(),
            dir_count: self.superblock.dir_count(),
            total_file_size: self.superblock.total_size(),
            block_size: self.superblock.block_size,
            redundancy_ratio: self.superblock.redundancy_ratio,
        }
    }
}

/// VFS information summary.
#[derive(Debug)]
pub struct VfsInfo {
    pub host_dir: PathBuf,
    pub host_count: usize,
    pub total_capacity: u64,
    pub used_capacity: u64,
    pub available_capacity: u64,
    pub file_count: usize,
    pub dir_count: usize,
    pub total_file_size: u64,
    pub block_size: u64,
    pub redundancy_ratio: f32,
}

impl Drop for SlackVfs {
    fn drop(&mut self) {
        // Try to sync on drop
        let _ = self.sync();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_host_dir() -> TempDir {
        let dir = TempDir::new().unwrap();

        // Create some host files
        for i in 0..5 {
            let path = dir.path().join(format!("host_{}.dat", i));
            let mut f = std::fs::File::create(&path).unwrap();
            // Write enough data to have slack space
            // Use small size to MAXIMIZE slack (4096 - 100 = 3996 bytes slack)
            let content = vec![0u8; 100];
            f.write_all(&content).unwrap();
        }

        dir
    }

    #[test]
    fn test_create_and_mount() {
        let dir = create_test_host_dir();
        let password = "test_password";

        // Create VFS
        {
            let vfs = SlackVfs::create(dir.path(), password, VfsConfig::default()).unwrap();
            assert_eq!(vfs.superblock.file_count(), 0);
        }

        // Mount VFS
        {
            let vfs = SlackVfs::mount(dir.path(), password).unwrap();
            assert_eq!(vfs.superblock.file_count(), 0);
        }
    }

    #[test]
    fn test_create_file() {
        let dir = create_test_host_dir();
        let password = "test_password";

        let mut vfs = SlackVfs::create(dir.path(), password, VfsConfig::default()).unwrap();

        let data = b"Hello, secret world!";
        vfs.create_file("/secret.txt", data).unwrap();

        let entries = vfs.list_dir("/").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "secret.txt");
    }

    #[test]
    fn test_read_file() {
        let dir = create_test_host_dir();
        let password = "test_password";

        let mut vfs = SlackVfs::create(dir.path(), password, VfsConfig::default()).unwrap();

        let data = b"Hello, secret world!";
        vfs.create_file("/secret.txt", data).unwrap();

        let read_data = vfs.read_file("/secret.txt").unwrap();
        assert_eq!(read_data, data);
    }

    #[test]
    fn test_create_directory() {
        let dir = create_test_host_dir();
        let password = "test_password";

        let mut vfs = SlackVfs::create(dir.path(), password, VfsConfig::default()).unwrap();

        vfs.create_dir("/docs").unwrap();
        vfs.create_file("/docs/readme.txt", b"Read me!").unwrap();

        let entries = vfs.list_dir("/docs").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "readme.txt");
    }

    #[test]
    fn test_delete_file() {
        let dir = create_test_host_dir();
        let password = "test_password";

        let mut vfs = SlackVfs::create(dir.path(), password, VfsConfig::default()).unwrap();

        vfs.create_file("/to_delete.txt", b"Delete me").unwrap();
        assert_eq!(vfs.list_dir("/").unwrap().len(), 1);

        vfs.delete_file("/to_delete.txt").unwrap();
        assert_eq!(vfs.list_dir("/").unwrap().len(), 0);
    }

    #[test]
    fn test_persistence() {
        let dir = create_test_host_dir();
        let password = "test_password";

        // Create and write
        {
            let mut vfs = SlackVfs::create(dir.path(), password, VfsConfig::default()).unwrap();
            vfs.create_file("/persistent.txt", b"Persisted data")
                .unwrap();
        }

        // Mount and read
        {
            let vfs = SlackVfs::mount(dir.path(), password).unwrap();
            let data = vfs.read_file("/persistent.txt").unwrap();
            assert_eq!(data, b"Persisted data");
        }
    }

    #[test]
    fn test_wrong_password() {
        let dir = create_test_host_dir();

        // Create with one password
        {
            let mut vfs =
                SlackVfs::create(dir.path(), "correct_password", VfsConfig::default()).unwrap();
            vfs.create_file("/secret.txt", b"Secret").unwrap();
        }

        // Try to mount with wrong password
        let result = SlackVfs::mount(dir.path(), "wrong_password");
        assert!(result.is_err());
    }
}
