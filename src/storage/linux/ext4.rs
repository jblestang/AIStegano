//! Ext4 file system parser for Linux.
//!
//! Parses ext4 superblock, block group descriptors, inodes, and extent trees
//! to locate file blocks and calculate slack space offsets.

use crate::error::{Error, Result};
use crate::storage::linux::BlockDevice;
use std::path::Path;

/// Ext4 magic number.
const EXT4_MAGIC: u16 = 0xEF53;

/// Size of the ext4 superblock.
const SUPERBLOCK_SIZE: usize = 1024;

/// Offset of the primary superblock (after boot sector).
const SUPERBLOCK_OFFSET: u64 = 1024;

/// Ext4 superblock structure (partial - key fields only).
#[derive(Debug, Clone)]
pub struct Ext4Superblock {
    pub inodes_count: u32,
    pub blocks_count: u64,
    pub block_size: u64,
    pub blocks_per_group: u32,
    pub inodes_per_group: u32,
    pub inode_size: u16,
    pub first_data_block: u32,
    pub desc_size: u16,
    pub feature_incompat: u32,
}

/// Ext4 inode structure (partial - key fields only).
#[derive(Debug, Clone)]
pub struct Ext4Inode {
    pub mode: u16,
    pub size: u64,
    pub blocks: u32,
    pub flags: u32,
    pub extents: Vec<Ext4Extent>,
}

/// Ext4 extent - represents a contiguous range of blocks.
#[derive(Debug, Clone)]
pub struct Ext4Extent {
    /// First file block number this extent covers.
    pub block: u32,
    /// Number of blocks covered by this extent.
    pub len: u16,
    /// Physical block number where data starts.
    pub start: u64,
}

/// Parser for ext4 file systems.
pub struct Ext4Parser {
    device: BlockDevice,
    superblock: Ext4Superblock,
}

impl Ext4Parser {
    /// Create a new ext4 parser for the given block device.
    pub fn new(device_path: &Path) -> Result<Self> {
        let device = BlockDevice::open(device_path)?;
        
        // Read superblock
        let sb_data = device.read_at(SUPERBLOCK_OFFSET, SUPERBLOCK_SIZE)?;
        let superblock = Self::parse_superblock(&sb_data)?;
        
        Ok(Self { device, superblock })
    }

    /// Parse the ext4 superblock from raw bytes.
    fn parse_superblock(data: &[u8]) -> Result<Ext4Superblock> {
        if data.len() < SUPERBLOCK_SIZE {
            return Err(Error::DataCorruption("Superblock too small".to_string()));
        }

        // Check magic number at offset 0x38 (56)
        let magic = u16::from_le_bytes([data[0x38], data[0x39]]);
        if magic != EXT4_MAGIC {
            return Err(Error::Unsupported(format!(
                "Not an ext4 filesystem (magic: 0x{:04X})",
                magic
            )));
        }

        // Parse key fields
        let inodes_count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let blocks_count_lo = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let blocks_count_hi = u32::from_le_bytes([data[0x150], data[0x151], data[0x152], data[0x153]]);
        let blocks_count = ((blocks_count_hi as u64) << 32) | (blocks_count_lo as u64);

        let log_block_size = u32::from_le_bytes([data[0x18], data[0x19], data[0x1A], data[0x1B]]);
        let block_size = 1024u64 << log_block_size;

        let blocks_per_group = u32::from_le_bytes([data[0x20], data[0x21], data[0x22], data[0x23]]);
        let inodes_per_group = u32::from_le_bytes([data[0x28], data[0x29], data[0x2A], data[0x2B]]);

        let inode_size = u16::from_le_bytes([data[0x58], data[0x59]]);
        let first_data_block = u32::from_le_bytes([data[0x14], data[0x15], data[0x16], data[0x17]]);

        let desc_size = u16::from_le_bytes([data[0xFE], data[0xFF]]);
        let feature_incompat = u32::from_le_bytes([data[0x60], data[0x61], data[0x62], data[0x63]]);

        Ok(Ext4Superblock {
            inodes_count,
            blocks_count,
            block_size,
            blocks_per_group,
            inodes_per_group,
            inode_size,
            first_data_block,
            desc_size: if desc_size == 0 { 32 } else { desc_size },
            feature_incompat,
        })
    }

    /// Get the block size of this filesystem.
    pub fn block_size(&self) -> u64 {
        self.superblock.block_size
    }

    /// Read an inode by its number.
    pub fn read_inode(&self, inode_num: u32) -> Result<Ext4Inode> {
        if inode_num == 0 || inode_num > self.superblock.inodes_count {
            return Err(Error::DataCorruption(format!(
                "Invalid inode number: {}",
                inode_num
            )));
        }

        // Calculate which block group the inode belongs to
        let group = (inode_num - 1) / self.superblock.inodes_per_group;
        let index_in_group = (inode_num - 1) % self.superblock.inodes_per_group;

        // Read block group descriptor to find inode table location
        let bgd_offset = if self.superblock.block_size == 1024 {
            2048  // Block 2 if block_size is 1024
        } else {
            self.superblock.block_size  // Block 1 otherwise
        };

        let desc_offset = bgd_offset + (group as u64 * self.superblock.desc_size as u64);
        let desc_data = self.device.read_at(desc_offset, self.superblock.desc_size as usize)?;

        // Inode table block from descriptor
        let inode_table_lo = u32::from_le_bytes([desc_data[8], desc_data[9], desc_data[10], desc_data[11]]);
        let inode_table_hi = if self.superblock.desc_size > 32 {
            u32::from_le_bytes([desc_data[40], desc_data[41], desc_data[42], desc_data[43]])
        } else {
            0
        };
        let inode_table = ((inode_table_hi as u64) << 32) | (inode_table_lo as u64);

        // Calculate inode offset
        let inode_offset = (inode_table * self.superblock.block_size)
            + (index_in_group as u64 * self.superblock.inode_size as u64);

        // Read inode
        let inode_data = self.device.read_at(inode_offset, self.superblock.inode_size as usize)?;
        self.parse_inode(&inode_data)
    }

    /// Parse an inode from raw bytes.
    fn parse_inode(&self, data: &[u8]) -> Result<Ext4Inode> {
        let mode = u16::from_le_bytes([data[0], data[1]]);
        let size_lo = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let size_hi = u32::from_le_bytes([data[0x6C], data[0x6D], data[0x6E], data[0x6F]]);
        let size = ((size_hi as u64) << 32) | (size_lo as u64);

        let blocks = u32::from_le_bytes([data[0x1C], data[0x1D], data[0x1E], data[0x1F]]);
        let flags = u32::from_le_bytes([data[0x20], data[0x21], data[0x22], data[0x23]]);

        // Parse extent tree from i_block (offset 0x28, 60 bytes)
        let extent_data = &data[0x28..0x28 + 60];
        let extents = self.parse_extent_tree(extent_data)?;

        Ok(Ext4Inode {
            mode,
            size,
            blocks,
            flags,
            extents,
        })
    }

    /// Parse extent tree from i_block area.
    fn parse_extent_tree(&self, data: &[u8]) -> Result<Vec<Ext4Extent>> {
        // Extent header
        let magic = u16::from_le_bytes([data[0], data[1]]);
        if magic != 0xF30A {
            // Not using extents (old block map) - not supported
            return Err(Error::Unsupported("Only extent-based files supported".to_string()));
        }

        let entries = u16::from_le_bytes([data[2], data[3]]);
        let depth = u16::from_le_bytes([data[6], data[7]]);

        let mut extents = Vec::new();

        if depth == 0 {
            // Leaf node - extents directly in this block
            for i in 0..entries as usize {
                let offset = 12 + i * 12; // Skip header (12 bytes), each extent is 12 bytes
                if offset + 12 > data.len() {
                    break;
                }

                let block = u32::from_le_bytes([
                    data[offset],
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                ]);
                let len = u16::from_le_bytes([data[offset + 4], data[offset + 5]]);
                let start_hi = u16::from_le_bytes([data[offset + 6], data[offset + 7]]);
                let start_lo = u32::from_le_bytes([
                    data[offset + 8],
                    data[offset + 9],
                    data[offset + 10],
                    data[offset + 11],
                ]);
                let start = ((start_hi as u64) << 32) | (start_lo as u64);

                extents.push(Ext4Extent { block, len, start });
            }
        } else {
            // Internal node - would need to follow index entries
            // For now, return error - full implementation would recursively read index blocks
            return Err(Error::Unsupported("Multi-level extent trees not yet supported".to_string()));
        }

        Ok(extents)
    }

    /// Get the physical block offset and slack space for a file.
    pub fn get_file_slack(&self, inode: &Ext4Inode) -> Result<(u64, u64)> {
        if inode.extents.is_empty() {
            return Err(Error::DataCorruption("File has no extents".to_string()));
        }

        // Find the last extent
        let last_extent = inode.extents.last().unwrap();
        
        // Calculate the physical location of the last block
        let blocks_used_in_extent = ((inode.size + self.superblock.block_size - 1)
            / self.superblock.block_size) as u32
            - last_extent.block;
        
        if blocks_used_in_extent == 0 || blocks_used_in_extent > last_extent.len as u32 {
            return Err(Error::DataCorruption("Invalid extent coverage".to_string()));
        }

        let last_block_phys = last_extent.start + (blocks_used_in_extent as u64 - 1);
        let last_block_offset = last_block_phys * self.superblock.block_size;

        // Slack starts at file_size mod block_size within the last block
        let slack_offset_in_block = inode.size % self.superblock.block_size;
        let slack_start = last_block_offset + slack_offset_in_block;
        let slack_available = self.superblock.block_size - slack_offset_in_block;

        Ok((slack_start, slack_available))
    }
}
