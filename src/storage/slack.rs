//! Low-level slack space read/write operations.

use crate::config::wipe_params;
use crate::error::Result;
use rand::RngCore;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Calculate the available slack space for a file.
///
/// Slack space = block_size - (file_size % block_size)
/// If file_size is exactly aligned, slack space is 0.
///
/// # Arguments
///
/// * `path` - Path to the file
/// * `block_size` - The file system block size
///
/// # Returns
///
/// The number of bytes of slack space available.
pub fn get_slack_capacity(path: &Path, block_size: u64) -> Result<u64> {
    let metadata = std::fs::metadata(path)?;
    let file_size = metadata.len();

    if file_size == 0 {
        return Ok(0);
    }

    let remainder = file_size % block_size;
    if remainder == 0 {
        Ok(0)
    } else {
        Ok(block_size - remainder)
    }
}

/// Write data to slack space (after the logical end of file).
///
/// This function writes data starting at the specified logical_size offset,
/// which represents the original end of the file before hidden data.
///
/// # Arguments
///
/// * `path` - Path to the file
/// * `data` - Data to write to slack space
/// * `logical_size` - The original file size (where hidden data starts)
///
/// # Returns
///
/// Ok(()) on success.
pub fn write_slack(path: &Path, data: &[u8], logical_size: u64) -> Result<()> {
    let mut file = OpenOptions::new().write(true).open(path)?;

    // Seek to the position after logical file end
    file.seek(SeekFrom::Start(logical_size))?;

    // Write the hidden data
    file.write_all(data)?;

    // Ensure data is flushed to disk
    file.sync_all()?;

    Ok(())
}

/// Read data from slack space.
///
/// # Arguments
///
/// * `path` - Path to the file
/// * `logical_size` - The original file size (where hidden data starts)
/// * `len` - Number of bytes to read
///
/// # Returns
///
/// The data read from slack space.
pub fn read_slack(path: &Path, logical_size: u64, len: usize) -> Result<Vec<u8>> {
    let mut file = File::open(path)?;

    // Seek to the position after logical file end
    file.seek(SeekFrom::Start(logical_size))?;

    // Read the hidden data
    let mut buffer = vec![0u8; len];
    let bytes_read = file.read(&mut buffer)?;

    // Truncate buffer to actual bytes read
    buffer.truncate(bytes_read);

    Ok(buffer)
}

/// Securely wipe slack space.
///
/// Performs multiple overwrite passes:
/// 1. Random data passes
/// 2. Zero passes
/// 3. Truncate file to logical size
///
/// # Arguments
///
/// * `path` - Path to the file
/// * `logical_size` - The original file size to restore
/// * `passes` - Number of overwrite passes (minimum 1)
pub fn wipe_slack(path: &Path, logical_size: u64, passes: Option<u8>) -> Result<()> {
    let metadata = std::fs::metadata(path)?;
    let current_size = metadata.len();

    if current_size <= logical_size {
        // No slack data to wipe
        return Ok(());
    }

    let slack_size = (current_size - logical_size) as usize;
    let random_passes = passes.unwrap_or(wipe_params::RANDOM_PASSES);

    let mut file = OpenOptions::new().write(true).open(path)?;

    // Random overwrite passes
    let mut rng = rand::thread_rng();
    let mut random_data = vec![0u8; slack_size];

    for _ in 0..random_passes {
        rng.fill_bytes(&mut random_data);
        file.seek(SeekFrom::Start(logical_size))?;
        file.write_all(&random_data)?;
        file.sync_all()?;
    }

    // Zero overwrite passes
    let zero_data = vec![0u8; slack_size];
    for _ in 0..wipe_params::ZERO_PASSES {
        file.seek(SeekFrom::Start(logical_size))?;
        file.write_all(&zero_data)?;
        file.sync_all()?;
    }

    // Truncate file to original logical size
    file.set_len(logical_size)?;
    file.sync_all()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_file(content: &[u8]) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_get_slack_capacity() {
        let file = create_test_file(&[0u8; 1000]);
        let capacity = get_slack_capacity(file.path(), 4096).unwrap();

        // 1000 bytes in a 4096 block = 3096 slack
        assert_eq!(capacity, 3096);
    }

    #[test]
    fn test_get_slack_capacity_aligned() {
        let file = create_test_file(&[0u8; 4096]);
        let capacity = get_slack_capacity(file.path(), 4096).unwrap();

        // Exactly aligned = 0 slack
        assert_eq!(capacity, 0);
    }

    #[test]
    fn test_write_and_read_slack() {
        let file = create_test_file(b"Original content");
        let logical_size = std::fs::metadata(file.path()).unwrap().len();

        let hidden_data = b"Secret hidden data!";
        write_slack(file.path(), hidden_data, logical_size).unwrap();

        let read_data = read_slack(file.path(), logical_size, hidden_data.len()).unwrap();
        assert_eq!(read_data, hidden_data);
    }

    #[test]
    fn test_original_content_preserved() {
        let original = b"Original content here";
        let file = create_test_file(original);
        let logical_size = std::fs::metadata(file.path()).unwrap().len();

        // Write hidden data
        write_slack(file.path(), b"Hidden!", logical_size).unwrap();

        // Read original content
        let mut original_read = vec![0u8; original.len()];
        let mut f = File::open(file.path()).unwrap();
        f.read_exact(&mut original_read).unwrap();

        assert_eq!(original_read, original);
    }

    #[test]
    fn test_wipe_slack() {
        let file = create_test_file(b"Original");
        let logical_size = std::fs::metadata(file.path()).unwrap().len();

        // Write hidden data
        let hidden = b"Super secret data";
        write_slack(file.path(), hidden, logical_size).unwrap();

        // Verify it was written
        let size_before_wipe = std::fs::metadata(file.path()).unwrap().len();
        assert!(size_before_wipe > logical_size);

        // Wipe
        wipe_slack(file.path(), logical_size, Some(1)).unwrap();

        // Check file is truncated
        let size_after_wipe = std::fs::metadata(file.path()).unwrap().len();
        assert_eq!(size_after_wipe, logical_size);
    }
}
