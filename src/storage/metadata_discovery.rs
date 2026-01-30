//! Metadata discovery in slack space.
//!
//! This module provides functionality to discover and read VFS metadata
//! stored in slack space, eliminating the need for a visible .slack_meta.json file.

use crate::error::{Error, Result};
use crate::storage::metadata::SlackMetadata;
use crate::storage::slack::{get_slack_capacity, read_slack, write_slack};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Magic signature for metadata in slack space
const MAGIC_SIGNATURE: &[u8; 12] = b"SVFS_META_V1";

/// Current metadata format version
const METADATA_VERSION: u32 = 3;

/// Header for metadata stored in slack space
#[derive(Debug, Clone)]
struct MetadataHeader {
    magic: [u8; 12],
    version: u32,
    total_length: u32,
    checksum: [u8; 32],
}

impl MetadataHeader {
    const SIZE: usize = 12 + 4 + 4 + 32; // 52 bytes

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::SIZE);
        bytes.extend_from_slice(&self.magic);
        bytes.extend_from_slice(&self.version.to_le_bytes());
        bytes.extend_from_slice(&self.total_length.to_le_bytes());
        bytes.extend_from_slice(&self.checksum);
        bytes
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < Self::SIZE {
            return Err(Error::DataCorruption(
                "Metadata header too short".to_string(),
            ));
        }

        let mut magic = [0u8; 12];
        magic.copy_from_slice(&bytes[0..12]);

        if &magic != MAGIC_SIGNATURE {
            return Err(Error::DataCorruption("Invalid magic signature".to_string()));
        }

        let version = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
        let total_length = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);

        let mut checksum = [0u8; 32];
        checksum.copy_from_slice(&bytes[20..52]);

        Ok(Self {
            magic,
            version,
            total_length,
            checksum,
        })
    }
}

/// Metadata discovery and storage in slack space
pub struct MetadataDiscovery;

impl MetadataDiscovery {
    /// Scan directory for metadata in slack space.
    ///
    /// Returns the path to the file containing metadata and the metadata itself.
    pub fn discover(dir: &Path, block_size: u64) -> Result<Option<(PathBuf, SlackMetadata)>> {
        // Scan all files in directory
        let entries = std::fs::read_dir(dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            // Skip directories and hidden files
            if path.is_dir() || path.file_name().unwrap().to_str().unwrap().starts_with('.') {
                continue;
            }

            // Try to read metadata from this file
            if let Ok(Some(metadata)) = Self::try_read_metadata(&path, block_size) {
                return Ok(Some((path, metadata)));
            }
        }

        Ok(None)
    }

    /// Try to read metadata from a specific file's slack space.
    fn try_read_metadata(path: &Path, block_size: u64) -> Result<Option<SlackMetadata>> {
        // Get file size
        let metadata = std::fs::metadata(path)?;
        let file_size = metadata.len();

        if file_size == 0 {
            return Ok(None);
        }

        // Try reading metadata from different possible offsets
        // Scan every byte looking for the magic signature
        // This is slower but guarantees we'll find metadata wherever it is
        for logical_size in 0..file_size {
            // Check if there's enough space after this offset
            if file_size - logical_size < MetadataHeader::SIZE as u64 {
                break;
            }

            // Try to read header at this offset
            let header_bytes = match read_slack(path, logical_size, MetadataHeader::SIZE) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            };

            // Try to parse header
            let header = match MetadataHeader::from_bytes(&header_bytes) {
                Ok(h) => h,
                Err(_) => continue, // Not metadata at this offset
            };

            // Found valid header! Read full metadata
            let total_size = MetadataHeader::SIZE + header.total_length as usize;
            if logical_size + total_size as u64 > file_size {
                continue; // Metadata would extend past file end
            }

            let full_data = match read_slack(path, logical_size, total_size) {
                Ok(data) => data,
                Err(_) => continue,
            };
            let metadata_bytes = &full_data[MetadataHeader::SIZE..];

            // Verify checksum
            let mut hasher = Sha256::new();
            hasher.update(metadata_bytes);
            let computed_checksum: [u8; 32] = hasher.finalize().into();

            if computed_checksum != header.checksum {
                continue; // Checksum mismatch, try next offset
            }

            // Deserialize metadata
            let metadata: SlackMetadata = match bincode::deserialize(metadata_bytes) {
                Ok(m) => m,
                Err(_) => continue,
            };

            return Ok(Some(metadata));
        }

        Ok(None)
    }

    /// Write metadata to slack space of a specific file.
    pub fn write_metadata(
        path: &Path,
        metadata: &SlackMetadata,
        logical_size: u64,
        block_size: u64,
    ) -> Result<()> {
        // Serialize metadata
        let metadata_bytes = bincode::serialize(metadata)
            .map_err(|e| Error::Serialization(format!("Failed to serialize metadata: {}", e)))?;

        // Compute checksum
        let mut hasher = Sha256::new();
        hasher.update(&metadata_bytes);
        let checksum: [u8; 32] = hasher.finalize().into();

        // Create header
        let header = MetadataHeader {
            magic: *MAGIC_SIGNATURE,
            version: METADATA_VERSION,
            total_length: metadata_bytes.len() as u32,
            checksum,
        };

        // Combine header + metadata
        let mut full_data = header.to_bytes();
        full_data.extend_from_slice(&metadata_bytes);

        // Check slack capacity
        let slack_capacity = get_slack_capacity(path, block_size)?;
        if full_data.len() as u64 > slack_capacity {
            return Err(Error::InsufficientSpace {
                needed: full_data.len() as u64,
                available: slack_capacity,
            });
        }

        // Write to slack space
        write_slack(path, &full_data, logical_size)?;

        Ok(())
    }

    /// Find a suitable file for storing metadata.
    ///
    /// Returns a file with sufficient slack space, or None if no suitable file exists.
    pub fn find_metadata_host(dir: &Path, block_size: u64, required_size: usize) -> Result<Option<PathBuf>> {
        let entries = std::fs::read_dir(dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            // Skip directories and hidden files
            if path.is_dir() || path.file_name().unwrap().to_str().unwrap().starts_with('.') {
                continue;
            }

            // Check slack capacity
            if let Ok(capacity) = get_slack_capacity(&path, block_size) {
                if capacity >= required_size as u64 {
                    return Ok(Some(path));
                }
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::SymbolLocation;
    use crate::vfs::types::EncodingInfo;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_header_serialization() {
        let header = MetadataHeader {
            magic: *MAGIC_SIGNATURE,
            version: 3,
            total_length: 1234,
            checksum: [42u8; 32],
        };

        let bytes = header.to_bytes();
        assert_eq!(bytes.len(), MetadataHeader::SIZE);

        let parsed = MetadataHeader::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.magic, header.magic);
        assert_eq!(parsed.version, header.version);
        assert_eq!(parsed.total_length, header.total_length);
        assert_eq!(parsed.checksum, header.checksum);
    }

    #[test]
    fn test_write_and_discover_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("host.dat");

        // Create a small file (100 bytes)
        let mut file = File::create(&file_path).unwrap();
        let content = vec![0u8; 100];
        file.write_all(&content).unwrap();
        drop(file);

        // Create test metadata
        let metadata = SlackMetadata {
            version: 3,
            block_size: 4096,
            salt: Some([1u8; 32]),
            superblock_encoding: Some(EncodingInfo {
                original_length: 500,
                source_symbols: 1,
                repair_symbols: 1,
                symbol_size: 1024,
            }),
            superblock_symbols: vec![SymbolLocation {
                host_path: PathBuf::from("test.dat"),
                offset: 4096,
                length: 1024,
                symbol_id: 0,
            }],
        };

        // Write metadata - this will extend the file into slack space
        MetadataDiscovery::write_metadata(&file_path, &metadata, 100, 4096).unwrap();

        // Discover metadata
        let result = MetadataDiscovery::discover(temp_dir.path(), 4096).unwrap();
        assert!(result.is_some());

        let (discovered_path, discovered_metadata) = result.unwrap();
        assert_eq!(discovered_path, file_path);
        assert_eq!(discovered_metadata.version, metadata.version);
        assert_eq!(discovered_metadata.block_size, metadata.block_size);
        assert_eq!(discovered_metadata.salt, metadata.salt);
    }

    #[test]
    fn test_no_metadata_found() {
        let temp_dir = TempDir::new().unwrap();

        // Create a file with no metadata
        let file_path = temp_dir.path().join("normal.dat");
        let mut file = File::create(&file_path).unwrap();
        file.write_all(&vec![0u8; 100]).unwrap();
        drop(file);

        // Discovery should return None
        let result = MetadataDiscovery::discover(temp_dir.path(), 4096).unwrap();
        assert!(result.is_none());
    }
}
