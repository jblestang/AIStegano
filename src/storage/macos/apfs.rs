//! APFS block mapping for macOS.
//!
//! Uses fcntl F_LOG2PHYS_EXT to map file logical offsets to physical disk offsets.

use crate::error::{Error, Result};
use crate::storage::slack_backend::SlackRegion;
use std::path::{Path, PathBuf};

/// APFS block mapper using fcntl.
pub struct ApfsMapper {
    /// Default block size (APFS typically uses 4096).
    block_size: u64,
}

/// Structure for F_LOG2PHYS_EXT fcntl.
#[repr(C)]
struct Log2PhysExt {
    /// Flags (input).
    l2p_flags: u32,
    /// Contiguous bytes at this location (input/output).
    l2p_contigbytes: i64,
    /// Logical offset (input).
    l2p_devoffset: i64,
}

impl ApfsMapper {
    pub fn new() -> Result<Self> {
        Ok(Self {
            block_size: 4096, // APFS default
        })
    }

    /// Get slack information for a file using fcntl F_LOG2PHYS_EXT.
    pub fn get_slack_info(&self, path: &Path) -> Result<SlackRegion> {
        use std::fs::File;
        use std::os::unix::io::AsRawFd;

        // Open the file
        let file = File::open(path)
            .map_err(|e| Error::Io(e))?;
        let fd = file.as_raw_fd();

        // Get file size
        let metadata = file.metadata()
            .map_err(|e| Error::Io(e))?;
        let file_size = metadata.len();

        if file_size == 0 {
            return Err(Error::DataCorruption("Cannot get slack for empty file".to_string()));
        }

        // Calculate offset of last byte
        let last_byte_offset = file_size - 1;

        // Use fcntl F_LOG2PHYS_EXT to map logical to physical
        let mut l2p = Log2PhysExt {
            l2p_flags: 0,
            l2p_contigbytes: 1, // We want to map just 1 byte at the end
            l2p_devoffset: last_byte_offset as i64,
        };

        // F_LOG2PHYS_EXT = 65 on macOS
        const F_LOG2PHYS_EXT: libc::c_int = 65;

        let result = unsafe {
            libc::fcntl(fd, F_LOG2PHYS_EXT, &mut l2p as *mut Log2PhysExt)
        };

        if result == -1 {
            return Err(Error::Io(std::io::Error::last_os_error()));
        }

        // l2p_devoffset now contains the physical offset
        let physical_offset = l2p.l2p_devoffset as u64;

        // Calculate slack
        // Slack starts at (physical_offset + 1) - but we need to find block boundary
        let block_start = (physical_offset / self.block_size) * self.block_size;
        let offset_in_block = file_size % self.block_size;
        let slack_start = block_start + offset_in_block;
        let slack_available = if offset_in_block == 0 {
            0 // File ends exactly at block boundary, no slack
        } else {
            self.block_size - offset_in_block
        };

        // Find the raw disk device for this file's volume
        let device_path = self.find_raw_device(path)?;

        Ok(SlackRegion {
            device_path,
            offset: slack_start,
            available: slack_available,
            logical_size: file_size,
            block_size: self.block_size,
        })
    }

    /// Find the raw disk device for a file's mount point.
    fn find_raw_device(&self, path: &Path) -> Result<PathBuf> {
        use std::process::Command;

        let path = path.canonicalize()
            .map_err(|e| Error::Io(e))?;

        // Use `df` to find mount point
        let output = Command::new("df")
            .arg(&path)
            .output()
            .map_err(|e| Error::Io(e))?;

        if !output.status.success() {
            return Err(Error::Unsupported("Failed to find mount point".to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = stdout.lines().collect();
        if lines.len() < 2 {
            return Err(Error::Unsupported("Unexpected df output".to_string()));
        }

        // First column of second line is the device
        let device = lines[1].split_whitespace().next()
            .ok_or_else(|| Error::Unsupported("Could not parse df output".to_string()))?;

        // Convert /dev/diskXsY to /dev/rdiskXsY (raw device)
        let raw_device = if device.starts_with("/dev/disk") {
            device.replace("/dev/disk", "/dev/rdisk")
        } else {
            device.to_string()
        };

        Ok(PathBuf::from(raw_device))
    }
}
