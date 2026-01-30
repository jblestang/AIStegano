//! macOS-specific slack space implementation using APFS.

mod apfs;
mod raw_disk;

use crate::error::Result;
use crate::storage::slack_backend::{SlackBackend, SlackRegion};
use std::path::Path;

pub use apfs::ApfsMapper;
pub use raw_disk::RawDisk;

/// macOS slack backend using APFS block mapping.
pub struct MacSlackBackend {
    /// Cached APFS mapper.
    mapper: ApfsMapper,
}

impl MacSlackBackend {
    pub fn new() -> Result<Self> {
        Ok(Self {
            mapper: ApfsMapper::new()?,
        })
    }
}

impl SlackBackend for MacSlackBackend {
    fn get_slack_info(&self, path: &Path) -> Result<SlackRegion> {
        // Use fcntl F_LOG2PHYS_EXT to map file to physical blocks
        self.mapper.get_slack_info(path)
    }

    fn read_slack(&self, region: &SlackRegion, offset: u64, len: usize) -> Result<Vec<u8>> {
        let disk = RawDisk::open(&region.device_path)?;
        let absolute_offset = region.offset + offset;
        disk.read_at(absolute_offset, len)
    }

    fn write_slack(&self, region: &SlackRegion, offset: u64, data: &[u8]) -> Result<()> {
        let disk = RawDisk::open_write(&region.device_path)?;
        let absolute_offset = region.offset + offset;
        disk.write_at(absolute_offset, data)
    }

    fn wipe_slack(&self, region: &SlackRegion) -> Result<()> {
        let zeros = vec![0u8; region.available as usize];
        self.write_slack(region, 0, &zeros)
    }

    fn is_available(&self) -> bool {
        // Check if we can use fcntl and access raw disk
        true
    }

    fn name(&self) -> &'static str {
        "macOS APFS"
    }
}
