//! Linux-specific slack space implementation using ext4.

mod block_device;
mod ext4;

use crate::error::{Error, Result};
use crate::storage::slack_backend::{SlackBackend, SlackRegion};
use std::path::Path;

pub use block_device::BlockDevice;
pub use ext4::Ext4Parser;

/// Linux slack backend using raw block device access.
pub struct LinuxSlackBackend {
    /// Cached ext4 parser (per-device).
    parsers: std::collections::HashMap<std::path::PathBuf, Ext4Parser>,
}

impl LinuxSlackBackend {
    pub fn new() -> Result<Self> {
        Ok(Self {
            parsers: std::collections::HashMap::new(),
        })
    }

    /// Get or create an ext4 parser for the device containing a file.
    fn get_parser(&mut self, file_path: &Path) -> Result<&Ext4Parser> {
        // Find the device for this file's mount point
        let device_path = Self::find_device_for_path(file_path)?;
        
        if !self.parsers.contains_key(&device_path) {
            let parser = Ext4Parser::new(&device_path)?;
            self.parsers.insert(device_path.clone(), parser);
        }
        
        Ok(self.parsers.get(&device_path).unwrap())
    }

    /// Find the block device for a given file path by parsing /proc/mounts.
    fn find_device_for_path(file_path: &Path) -> Result<std::path::PathBuf> {
        use std::fs;
        use std::io::{BufRead, BufReader};

        let file_path = file_path.canonicalize()
            .map_err(|e| Error::Io(e))?;

        let mounts = fs::File::open("/proc/mounts")
            .map_err(|e| Error::Io(e))?;
        let reader = BufReader::new(mounts);

        let mut best_match: Option<(std::path::PathBuf, std::path::PathBuf)> = None;
        let mut best_len = 0;

        for line in reader.lines() {
            let line = line.map_err(|e| Error::Io(e))?;
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }

            let device = parts[0];
            let mount_point = parts[1];

            // Check if this mount point is a prefix of our file path
            if file_path.starts_with(mount_point) && mount_point.len() > best_len {
                // Only consider block devices
                if device.starts_with("/dev/") {
                    best_match = Some((
                        std::path::PathBuf::from(device),
                        std::path::PathBuf::from(mount_point),
                    ));
                    best_len = mount_point.len();
                }
            }
        }

        best_match
            .map(|(device, _)| device)
            .ok_or_else(|| Error::Unsupported("Could not find block device for path".to_string()))
    }
}

impl SlackBackend for LinuxSlackBackend {
    fn get_slack_info(&self, _path: &Path) -> Result<SlackRegion> {
        // TODO: Implement using ext4 parser
        // 1. Find device and mount point
        // 2. Parse inode for file
        // 3. Get extent tree
        // 4. Calculate slack offset
        Err(Error::Unsupported("Linux slack backend not yet implemented".to_string()))
    }

    fn read_slack(&self, region: &SlackRegion, offset: u64, len: usize) -> Result<Vec<u8>> {
        let device = BlockDevice::open(&region.device_path)?;
        let absolute_offset = region.offset + offset;
        device.read_at(absolute_offset, len)
    }

    fn write_slack(&self, region: &SlackRegion, offset: u64, data: &[u8]) -> Result<()> {
        let device = BlockDevice::open_write(&region.device_path)?;
        let absolute_offset = region.offset + offset;
        device.write_at(absolute_offset, data)
    }

    fn wipe_slack(&self, region: &SlackRegion) -> Result<()> {
        let zeros = vec![0u8; region.available as usize];
        self.write_slack(region, 0, &zeros)
    }

    fn is_available(&self) -> bool {
        // Check if we can access /proc/mounts and have necessary privileges
        std::path::Path::new("/proc/mounts").exists()
    }

    fn name(&self) -> &'static str {
        "Linux ext4"
    }
}
