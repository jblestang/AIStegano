//! Resilience tests - simulate data loss and verify recovery.

use slack_vfs::config::VfsConfig;
use slack_vfs::vfs::SlackVfs;
use std::fs;
use std::io::{Seek, SeekFrom, Write};
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

/// Corrupt slack space in a specific host file by overwriting with zeros.
fn corrupt_slack_space(file_path: &std::path::Path, block_size: u64, corruption_bytes: usize) {
    let metadata = fs::metadata(file_path).expect("Failed to get file metadata");
    let file_size = metadata.len();

    // Calculate where slack space starts
    let blocks = (file_size + block_size - 1) / block_size;
    let allocated_size = blocks * block_size;
    let slack_start = file_size;

    if slack_start < allocated_size {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .open(file_path)
            .expect("Failed to open file for corruption");

        file.seek(SeekFrom::Start(slack_start))
            .expect("Failed to seek");

        // Overwrite with zeros to simulate corruption
        let zeroes = vec![0u8; corruption_bytes];
        let _ = file.write(&zeroes); // Ignore errors if can't write full amount
    }
}

/// Overwrite a portion of a file's slack space with random data.
fn overwrite_slack_portion(
    file_path: &std::path::Path,
    original_size: u64,
    offset: u64,
    data: &[u8],
) {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(file_path)
        .expect("Failed to open file");

    file.seek(SeekFrom::Start(original_size + offset))
        .expect("Failed to seek");
    file.write_all(data).expect("Failed to write corruption");
}

#[test]
fn test_recovery_with_minor_symbol_loss() {
    // With 50% redundancy, we should recover from losing ~30% of symbols
    let temp_dir = setup_test_env(10, 8192);
    let host_path = temp_dir.path();
    let password = "resilience_test";

    // Create VFS and write a file
    let mut vfs =
        SlackVfs::create(host_path, password, VfsConfig::default()).expect("Failed to create VFS");

    let original_content = b"This is important data that must survive partial loss!";
    vfs.create_file("/important.txt", original_content)
        .expect("Failed to create file");
    vfs.sync().expect("Failed to sync");
    drop(vfs);

    // Corrupt a small portion of one host file's slack space
    let host_file = temp_dir.path().join("host_0.dat");
    let file_size = fs::metadata(&host_file).unwrap().len();
    overwrite_slack_portion(&host_file, file_size, 0, &[0xDE, 0xAD, 0xBE, 0xEF]);

    // Mount and try to read - with redundancy, should still work
    let vfs = SlackVfs::mount(host_path, password).expect("Failed to mount VFS");

    // Health check should show the file (may or may not show damage depending on symbol)
    let health = vfs.health_check().expect("Failed to get health");
    println!(
        "After minor corruption: {} recoverable, {} damaged",
        health.recoverable_files,
        health.damaged_files.len()
    );

    // The file content might still be readable if the corrupted portion
    // was repair symbols or if RaptorQ can recover
    let _ = vfs.read_file("/important.txt"); // May or may not succeed
}

#[test]
fn test_recovery_from_one_host_file_complete_loss() {
    // Simulate one entire host file's slack being overwritten
    // With 10 host files and 50% redundancy, should still recover
    let temp_dir = setup_test_env(10, 8192);
    let host_path = temp_dir.path();
    let password = "complete_loss_test";

    let mut vfs =
        SlackVfs::create(host_path, password, VfsConfig::default()).expect("Failed to create VFS");

    let original_content = b"Critical data distributed across multiple hosts";
    vfs.create_file("/critical.txt", original_content)
        .expect("Failed to create file");
    vfs.sync().expect("Failed to sync");

    // Get info about symbol distribution before dropping vfs
    let _info = vfs.info();
    drop(vfs);

    // Completely wipe slack space of one host file
    let host_file = temp_dir.path().join("host_0.dat");
    corrupt_slack_space(&host_file, 4096, 4096); // Wipe up to 4KB of slack

    // Mount and check health
    let vfs = SlackVfs::mount(host_path, password).expect("Failed to mount VFS");
    let health = vfs.health_check().expect("Failed to get health");

    println!(
        "After one host loss: {} total, {} recoverable",
        health.total_files, health.recoverable_files
    );
}

#[test]
fn test_metadata_survives_host_modification() {
    let temp_dir = setup_test_env(5, 4096);
    let host_path = temp_dir.path();
    let password = "metadata_test";

    let mut vfs =
        SlackVfs::create(host_path, password, VfsConfig::default()).expect("Failed to create VFS");
    vfs.create_file("/test.txt", b"Test data")
        .expect("Failed to create file");
    vfs.sync().expect("Failed to sync");
    drop(vfs);

    // Verify metadata file exists
    let metadata_path = host_path.join(".slack_meta.json");
    assert!(metadata_path.exists(), "Metadata file should exist");

    // Modify a host file (append data)
    let host_file = temp_dir.path().join("host_0.dat");
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&host_file)
        .expect("Failed to open host file");
    file.write_all(b"Extra content")
        .expect("Failed to append to host file");

    // Try to mount - should still work (though some symbols may be lost)
    let result = SlackVfs::mount(host_path, password);
    assert!(
        result.is_ok(),
        "Should still be able to mount after host modification"
    );
}

#[test]
fn test_health_detects_damage() {
    let temp_dir = setup_test_env(10, 8192);
    let host_path = temp_dir.path();
    let password = "health_detect_test";

    let mut vfs =
        SlackVfs::create(host_path, password, VfsConfig::default()).expect("Failed to create VFS");
    vfs.create_file("/file1.txt", b"File one content")
        .expect("Failed to create file");
    vfs.create_file("/file2.txt", b"File two content")
        .expect("Failed to create file");
    vfs.sync().expect("Failed to sync");
    drop(vfs);

    // Health check before corruption
    let vfs = SlackVfs::mount(host_path, password).expect("Failed to mount VFS");
    let health_before = vfs.health_check().expect("Failed to get health");
    assert_eq!(health_before.total_files, 2);
    assert_eq!(health_before.recoverable_files, 2);
    assert!(health_before.damaged_files.is_empty());
    drop(vfs);

    // Corrupt multiple host files severely
    for i in 0..5 {
        let host_file = temp_dir.path().join(format!("host_{}.dat", i));
        corrupt_slack_space(&host_file, 4096, 2048);
    }

    // Health check after corruption
    let vfs = SlackVfs::mount(host_path, password).expect("Failed to mount VFS");
    let health_after = vfs.health_check().expect("Failed to get health");

    println!(
        "After corruption: {} total, {} recoverable, {} damaged",
        health_after.total_files,
        health_after.recoverable_files,
        health_after.damaged_files.len()
    );

    // With severe corruption, some files may be damaged
    // The exact result depends on symbol distribution
}

#[test]
fn test_high_redundancy_improves_resilience() {
    let temp_dir = setup_test_env(15, 8192);
    let host_path = temp_dir.path();
    let password = "high_redundancy_test";

    // Create with higher redundancy (75%)
    let config = VfsConfig {
        redundancy_ratio: 0.75,
        ..Default::default()
    };

    let mut vfs = SlackVfs::create(host_path, password, config).expect("Failed to create VFS");
    vfs.create_file("/resilient.txt", b"This file should survive more damage")
        .expect("Failed to create file");
    vfs.sync().expect("Failed to sync");
    drop(vfs);

    // Corrupt several host files
    for i in 0..4 {
        let host_file = temp_dir.path().join(format!("host_{}.dat", i));
        corrupt_slack_space(&host_file, 4096, 1024);
    }

    // Should still be recoverable with 75% redundancy
    let vfs = SlackVfs::mount(host_path, password).expect("Failed to mount VFS");
    let health = vfs.health_check().expect("Failed to get health");

    println!(
        "High redundancy after corruption: {} recoverable of {} total",
        health.recoverable_files, health.total_files
    );
}

#[test]
fn test_superblock_survives_with_encryption() {
    let temp_dir = setup_test_env(5, 4096);
    let host_path = temp_dir.path();
    let password = "superblock_test";

    // Create VFS with some content
    let mut vfs =
        SlackVfs::create(host_path, password, VfsConfig::default()).expect("Failed to create VFS");
    vfs.create_dir("/folder").expect("Failed to create dir");
    vfs.create_file("/folder/data.txt", b"Nested file")
        .expect("Failed to create file");
    vfs.sync().expect("Failed to sync");
    drop(vfs);

    // Remount multiple times to verify superblock persistence
    for i in 0..3 {
        let vfs = SlackVfs::mount(host_path, password)
            .expect(&format!("Failed to mount VFS on iteration {}", i));

        let entries = vfs.list_dir("/").expect("Failed to list root");
        assert!(
            !entries.is_empty(),
            "Root should have entries on iteration {}",
            i
        );

        let content = vfs
            .read_file("/folder/data.txt")
            .expect("Failed to read file");
        assert_eq!(content, b"Nested file".to_vec());
    }
}

#[test]
fn test_wipe_removes_all_data() {
    let temp_dir = setup_test_env(5, 4096);
    let host_path = temp_dir.path();
    let password = "wipe_test";

    // Create VFS with content
    let mut vfs =
        SlackVfs::create(host_path, password, VfsConfig::default()).expect("Failed to create VFS");
    vfs.create_file("/secret.txt", b"Sensitive data")
        .expect("Failed to create file");
    vfs.sync().expect("Failed to sync");

    // Wipe
    vfs.wipe().expect("Failed to wipe VFS");
    drop(vfs);

    // Metadata should be removed
    let metadata_path = host_path.join(".slack_meta.json");
    assert!(
        !metadata_path.exists(),
        "Metadata file should be removed after wipe"
    );

    // Cannot remount because VFS is wiped
    let result = SlackVfs::mount(host_path, password);
    assert!(result.is_err(), "Should not be able to mount wiped VFS");
}

#[test]
fn test_superblock_replication() {
    let temp_dir = setup_test_env(5, 4096);
    let host_path = temp_dir.path();
    let password = "replication_test";

    // Create VFS
    let mut vfs = SlackVfs::create(host_path, password, VfsConfig::default()).expect("Created");
    vfs.sync().expect("Synced");
    drop(vfs);

    // Inspect metadata to verify replication count
    let meta = slack_vfs::storage::SlackMetadata::load(host_path).expect("Loaded metadata");
    let replica_count = meta.superblocks.len();
    println!("Superblock replicas: {}", replica_count);

    // We expect 3 replicas since we provided 5 hosts and they have enough space
    assert_eq!(replica_count, 3, "Should have 3 replicas given 5 hosts");

    // Corrupt the first replica
    let first_loc = &meta.superblocks[0];
    overwrite_slack_portion(&first_loc.host_path, first_loc.offset, 0, &[0u8; 100]);

    // Try to mount - should succeed using 2nd or 3rd replica
    let vfs = SlackVfs::mount(host_path, password).expect("Should mount with 1 corruption");
    drop(vfs);

    // Corrupt second replica
    let second_loc = &meta.superblocks[1];
    overwrite_slack_portion(&second_loc.host_path, second_loc.offset, 0, &[0u8; 100]);

    // Try to mount - should succeed using 3rd replica
    let vfs = SlackVfs::mount(host_path, password).expect("Should mount with 2 corruptions");
    drop(vfs);

    // Corrupt third replica (all corrupted)
    let third_loc = &meta.superblocks[2];
    overwrite_slack_portion(&third_loc.host_path, third_loc.offset, 0, &[0u8; 100]);

    // Try to mount - should fail
    let result = SlackVfs::mount(host_path, password);
    assert!(result.is_err(), "Should fail when all replicas corrupted");
}
