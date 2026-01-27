//! Integration tests for VFS end-to-end functionality.

use slack_vfs::config::VfsConfig;
use slack_vfs::vfs::SlackVfs;
use std::fs;
use tempfile::TempDir;

/// Helper to create a test environment with host files.
/// Note: file_size should NOT be a multiple of 4096 (block size) to ensure slack space exists.
fn setup_test_env(num_files: usize, _file_size: usize) -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    for i in 0..num_files {
        let file_path = temp_dir.path().join(format!("host_{}.dat", i));
        // Use a size that is NOT aligned to block size to ensure slack space
        // Maximize slack: 100 bytes content -> ~3996 bytes slack
        let actual_size = 100 + (i * 7);
        let data: Vec<u8> = (0..actual_size).map(|x| (x % 256) as u8).collect();
        fs::write(&file_path, &data).expect("Failed to create host file");
    }

    temp_dir
}

#[test]
fn test_full_workflow_create_write_read() {
    let temp_dir = setup_test_env(5, 4096);
    let host_path = temp_dir.path();
    let password = "test_password_123";

    // Create VFS
    let mut vfs =
        SlackVfs::create(host_path, password, VfsConfig::default()).expect("Failed to create VFS");

    // Write a file
    let content = b"Hello, World! This is a secret message.";
    vfs.create_file("/secret.txt", content)
        .expect("Failed to create file");

    // Sync (persist to disk)
    vfs.sync().expect("Failed to sync");
    drop(vfs);

    // Mount and read back
    let vfs = SlackVfs::mount(host_path, password).expect("Failed to mount VFS");
    let read_content = vfs.read_file("/secret.txt").expect("Failed to read file");

    assert_eq!(read_content, content.to_vec());
}

#[test]
fn test_multiple_files_and_directories() {
    let temp_dir = setup_test_env(30, 8192);
    let host_path = temp_dir.path();
    let password = "multi_file_test";

    let mut vfs =
        SlackVfs::create(host_path, password, VfsConfig::default()).expect("Failed to create VFS");

    // Create directory structure
    vfs.create_dir("/documents").expect("Failed to create dir");
    vfs.create_dir("/documents/work")
        .expect("Failed to create nested dir");
    vfs.create_dir("/images").expect("Failed to create dir");

    // Create multiple files
    vfs.create_file("/readme.txt", b"Root level file")
        .expect("Failed to create file");
    vfs.create_file("/documents/report.txt", b"Work document content")
        .expect("Failed to create file");
    vfs.create_file("/documents/work/notes.txt", b"Deep nested notes")
        .expect("Failed to create file");
    vfs.create_file("/images/photo.dat", b"Binary image data here")
        .expect("Failed to create file");

    vfs.sync().expect("Failed to sync");
    drop(vfs);

    // Mount and verify
    let vfs = SlackVfs::mount(host_path, password).expect("Failed to mount VFS");

    // List root
    let root_entries = vfs.list_dir("/").expect("Failed to list root");
    assert_eq!(root_entries.len(), 3); // documents, images, readme.txt

    // List documents
    let doc_entries = vfs
        .list_dir("/documents")
        .expect("Failed to list documents");
    assert_eq!(doc_entries.len(), 2); // work, report.txt

    // Verify file contents
    assert_eq!(
        vfs.read_file("/readme.txt").unwrap(),
        b"Root level file".to_vec()
    );
    assert_eq!(
        vfs.read_file("/documents/report.txt").unwrap(),
        b"Work document content".to_vec()
    );
    assert_eq!(
        vfs.read_file("/documents/work/notes.txt").unwrap(),
        b"Deep nested notes".to_vec()
    );
}

#[test]
fn test_file_deletion() {
    let temp_dir = setup_test_env(20, 4096);
    let host_path = temp_dir.path();
    let password = "delete_test";

    let mut vfs =
        SlackVfs::create(host_path, password, VfsConfig::default()).expect("Failed to create VFS");

    // Create and then delete files
    vfs.create_file("/temp1.txt", b"Temporary file 1")
        .expect("Failed to create file");
    vfs.create_file("/temp2.txt", b"Temporary file 2")
        .expect("Failed to create file");
    vfs.create_file("/keep.txt", b"Keep this file")
        .expect("Failed to create file");

    // Delete temp files
    vfs.delete_file("/temp1.txt").expect("Failed to delete");
    vfs.delete_file("/temp2.txt").expect("Failed to delete");

    vfs.sync().expect("Failed to sync");
    drop(vfs);

    // Mount and verify
    let vfs = SlackVfs::mount(host_path, password).expect("Failed to mount VFS");

    let entries = vfs.list_dir("/").expect("Failed to list root");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "keep.txt");

    // Verify kept file
    assert_eq!(
        vfs.read_file("/keep.txt").unwrap(),
        b"Keep this file".to_vec()
    );

    // Verify deleted files are gone
    assert!(vfs.read_file("/temp1.txt").is_err());
    assert!(vfs.read_file("/temp2.txt").is_err());
}

#[test]
fn test_wrong_password_fails() {
    let temp_dir = setup_test_env(5, 4096);
    let host_path = temp_dir.path();

    // Create with one password
    let mut vfs = SlackVfs::create(host_path, "correct_password", VfsConfig::default())
        .expect("Failed to create VFS");
    vfs.create_file("/secret.txt", b"Secret data")
        .expect("Failed to create file");
    vfs.sync().expect("Failed to sync");
    drop(vfs);

    // Try to mount with wrong password
    let result = SlackVfs::mount(host_path, "wrong_password");
    assert!(result.is_err());
}

#[test]
fn test_large_file() {
    let temp_dir = setup_test_env(20, 8192); // More host files for capacity
    let host_path = temp_dir.path();
    let password = "large_file_test";

    let mut vfs =
        SlackVfs::create(host_path, password, VfsConfig::default()).expect("Failed to create VFS");

    // Create a larger file (but still within capacity)
    let large_content: Vec<u8> = (0..5000).map(|i| (i % 256) as u8).collect();
    vfs.create_file("/large.bin", &large_content)
        .expect("Failed to create large file");

    vfs.sync().expect("Failed to sync");
    drop(vfs);

    // Mount and verify
    let vfs = SlackVfs::mount(host_path, password).expect("Failed to mount VFS");
    let read_content = vfs.read_file("/large.bin").expect("Failed to read file");

    assert_eq!(read_content, large_content);
}

#[test]
fn test_health_check_healthy() {
    let temp_dir = setup_test_env(5, 4096);
    let host_path = temp_dir.path();
    let password = "health_test";

    let mut vfs =
        SlackVfs::create(host_path, password, VfsConfig::default()).expect("Failed to create VFS");
    vfs.create_file("/test.txt", b"Test content")
        .expect("Failed to create file");
    vfs.sync().expect("Failed to sync");

    // Check health
    let report = vfs.health_check().expect("Failed to get health");
    assert_eq!(report.total_files, 1);
    assert_eq!(report.recoverable_files, 1);
    assert!(report.damaged_files.is_empty());
}

#[test]
fn test_capacity_info() {
    let temp_dir = setup_test_env(10, 8192);
    let host_path = temp_dir.path();
    let password = "capacity_test";

    let vfs =
        SlackVfs::create(host_path, password, VfsConfig::default()).expect("Failed to create VFS");

    let info = vfs.info();
    assert!(info.total_capacity > 0);
    assert!(info.host_count == 10);
}

#[test]
fn test_unicode_filenames() {
    let temp_dir = setup_test_env(5, 4096);
    let host_path = temp_dir.path();
    let password = "unicode_test";

    let mut vfs =
        SlackVfs::create(host_path, password, VfsConfig::default()).expect("Failed to create VFS");

    // Create files with unicode names
    vfs.create_file("/日本語.txt", b"Japanese content")
        .expect("Failed to create unicode file");
    vfs.create_file("/émoji_🎉.txt", b"Emoji content")
        .expect("Failed to create emoji file");

    vfs.sync().expect("Failed to sync");
    drop(vfs);

    // Mount and verify
    let vfs = SlackVfs::mount(host_path, password).expect("Failed to mount VFS");

    assert_eq!(
        vfs.read_file("/日本語.txt").unwrap(),
        b"Japanese content".to_vec()
    );
    assert_eq!(
        vfs.read_file("/émoji_🎉.txt").unwrap(),
        b"Emoji content".to_vec()
    );
}

#[test]
fn test_empty_file() {
    let temp_dir = setup_test_env(5, 4096);
    let host_path = temp_dir.path();
    let password = "empty_test";

    let mut vfs =
        SlackVfs::create(host_path, password, VfsConfig::default()).expect("Failed to create VFS");

    // Create an empty file
    vfs.create_file("/empty.txt", b"")
        .expect("Failed to create empty file");

    vfs.sync().expect("Failed to sync");
    drop(vfs);

    // Mount and verify
    let vfs = SlackVfs::mount(host_path, password).expect("Failed to mount VFS");
    let content = vfs
        .read_file("/empty.txt")
        .expect("Failed to read empty file");
    assert!(content.is_empty());
}
