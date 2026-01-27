# CLI Usage Guide

Complete command-line reference for the Slack Space Virtual File System.

## Table of Contents

1. [Overview](#overview)
2. [Commands](#commands)
   - [init](#init)
   - [ls](#ls)
   - [write](#write)
   - [read](#read)
   - [rm](#rm)
   - [mkdir](#mkdir)
   - [info](#info)
   - [health](#health)
   - [passwd](#passwd)
   - [wipe](#wipe)
3. [Common Workflows](#common-workflows)
4. [Error Messages](#error-messages)
5. [Troubleshooting](#troubleshooting)

## Overview

```bash
slack-vfs <COMMAND> [OPTIONS]
```

### Global Options

| Option | Description |
|--------|-------------|
| `-h, --help` | Print help information |
| `-V, --version` | Print version information |

### Getting Help

```bash
# General help
slack-vfs --help

# Command-specific help
slack-vfs init --help
slack-vfs write --help
```

## Commands

### init

Initialize a new VFS in a directory containing host files.

```bash
slack-vfs init <HOST_DIR> [OPTIONS]
```

#### Arguments

| Argument | Description |
|----------|-------------|
| `HOST_DIR` | Directory containing host files |

#### Options

| Option | Default | Description |
|--------|---------|-------------|
| `-b, --block-size <SIZE>` | 4096 | File system block size (bytes) |
| `-r, --redundancy <RATIO>` | 0.5 | Erasure coding redundancy (0.0-1.0) |
| `-s, --symbol-size <SIZE>` | 1024 | RaptorQ symbol size (bytes) |

#### Examples

```bash
# Initialize with default settings
slack-vfs init ./host_directory

# Custom block size and redundancy
slack-vfs init ./host_directory --block-size 4096 --redundancy 0.75

# Higher redundancy for better recovery
slack-vfs init ./important_data --redundancy 0.8
```

#### Notes

- You will be prompted for a password
- The directory must contain at least one file with slack space
- Cannot initialize an already initialized directory

---

### ls

List directory contents in the VFS.

```bash
slack-vfs ls <HOST_DIR> <VFS_PATH>
```

#### Arguments

| Argument | Description |
|----------|-------------|
| `HOST_DIR` | Directory containing the VFS |
| `VFS_PATH` | Path within the VFS (starts with /) |

#### Options

| Option | Description |
|--------|-------------|
| `-l, --long` | Use long listing format |

#### Examples

```bash
# List root directory
slack-vfs ls ./host_directory /

# List subdirectory
slack-vfs ls ./host_directory /documents

# Long format with details
slack-vfs ls ./host_directory / --long
```

#### Output

```
$ slack-vfs ls ./host_directory /
documents/
notes/
secret.txt
passwords.db

$ slack-vfs ls ./host_directory / --long
drwx  documents/       2026-01-27 12:00
drwx  notes/           2026-01-27 12:00  
-rw-  secret.txt       2026-01-27 12:00  1234 bytes
-rw-  passwords.db     2026-01-27 12:00  5678 bytes
```

---

### write

Write a file to the VFS.

```bash
slack-vfs write <HOST_DIR> <VFS_PATH> [OPTIONS]
```

#### Arguments

| Argument | Description |
|----------|-------------|
| `HOST_DIR` | Directory containing the VFS |
| `VFS_PATH` | Destination path within the VFS |

#### Options

| Option | Description |
|--------|-------------|
| `-i, --input <FILE>` | Source file to write |
| `-d, --data <STRING>` | Inline data to write |

**Note:** You must specify either `--input` or `--data`, but not both.

#### Examples

```bash
# Write from a file
slack-vfs write ./host_directory /secrets/passwords.txt --input ./my_passwords.txt

# Write inline data
slack-vfs write ./host_directory /notes/quick.txt --data "Remember the milk"

# Write binary file
slack-vfs write ./host_directory /data/image.png --input ./photo.png
```

#### Notes

- Parent directories are NOT created automatically (use `mkdir` first)
- Existing files are NOT overwritten (delete first with `rm`)
- You will be prompted for the password

---

### read

Read a file from the VFS.

```bash
slack-vfs read <HOST_DIR> <VFS_PATH> [OPTIONS]
```

#### Arguments

| Argument | Description |
|----------|-------------|
| `HOST_DIR` | Directory containing the VFS |
| `VFS_PATH` | Path to file within the VFS |

#### Options

| Option | Description |
|--------|-------------|
| `-o, --output <FILE>` | Write to file instead of stdout |

#### Examples

```bash
# Display to stdout (text files)
slack-vfs read ./host_directory /notes/secret.txt

# Write to file
slack-vfs read ./host_directory /data/backup.zip --output ./restored.zip

# Pipe to other commands
slack-vfs read ./host_directory /data/list.txt | grep "important"
```

---

### rm

Delete a file from the VFS.

```bash
slack-vfs rm <HOST_DIR> <VFS_PATH>
```

#### Arguments

| Argument | Description |
|----------|-------------|
| `HOST_DIR` | Directory containing the VFS |
| `VFS_PATH` | Path to file to delete |

#### Examples

```bash
# Delete a file
slack-vfs rm ./host_directory /old/file.txt

# Delete multiple files (with shell loop)
for f in /temp/file1.txt /temp/file2.txt; do
    slack-vfs rm ./host_directory "$f"
done
```

#### Notes

- Only files can be deleted, not directories
- To delete a directory, first delete all files inside it
- Deleted data is not immediately wiped (use `wipe` for secure deletion)

---

### mkdir

Create a directory in the VFS.

```bash
slack-vfs mkdir <HOST_DIR> <VFS_PATH>
```

#### Arguments

| Argument | Description |
|----------|-------------|
| `HOST_DIR` | Directory containing the VFS |
| `VFS_PATH` | Path of directory to create |

#### Examples

```bash
# Create a directory
slack-vfs mkdir ./host_directory /documents

# Create nested directories (one at a time)
slack-vfs mkdir ./host_directory /documents
slack-vfs mkdir ./host_directory /documents/work
slack-vfs mkdir ./host_directory /documents/work/projects
```

---

### info

Show VFS status and capacity information.

```bash
slack-vfs info <HOST_DIR>
```

#### Arguments

| Argument | Description |
|----------|-------------|
| `HOST_DIR` | Directory containing the VFS |

#### Example Output

```
$ slack-vfs info ./host_directory

Slack VFS Information
=====================
Host Directory: ./host_directory
Host Files:     10
Block Size:     4096 bytes
Redundancy:     50%

Storage
-------
Total Capacity:     125.4 KB
Used:               45.2 KB (36%)
Available:          80.2 KB (64%)

Contents
--------
Files:              5
Directories:        3
Total File Size:    30.1 KB
```

---

### health

Run a health check on the VFS.

```bash
slack-vfs health <HOST_DIR>
```

#### Arguments

| Argument | Description |
|----------|-------------|
| `HOST_DIR` | Directory containing the VFS |

#### Example Output

```
$ slack-vfs health ./host_directory

VFS Health Report
=================
Total Files:        5
Recoverable:        4 (80%)
Damaged:            1 (20%)

Damaged Files:
  - /documents/old.txt (35% symbols lost) [UNRECOVERABLE]

Host Status:
  - host_1.dat: OK (using 4.2 KB / 8.0 KB slack)
  - host_2.dat: OK (using 2.1 KB / 4.0 KB slack)
  - host_3.dat: WARNING - file modified
```

---

### passwd

Change the VFS password.

```bash
slack-vfs passwd <HOST_DIR>
```

#### Arguments

| Argument | Description |
|----------|-------------|
| `HOST_DIR` | Directory containing the VFS |

#### Process

1. Enter current password
2. Enter new password
3. Confirm new password
4. Re-encryption of superblock

#### Notes

- File data is NOT re-encrypted (only the master key wrapping)
- Old password is required
- Choose a strong, unique password

---

### wipe

Securely wipe all VFS data.

```bash
slack-vfs wipe <HOST_DIR> [OPTIONS]
```

#### Arguments

| Argument | Description |
|----------|-------------|
| `HOST_DIR` | Directory containing the VFS |

#### Options

| Option | Default | Description |
|--------|---------|-------------|
| `-p, --passes <N>` | 3 | Number of overwrite passes |
| `-f, --force` | false | Skip confirmation prompt |

#### Examples

```bash
# Interactive wipe
slack-vfs wipe ./host_directory

# Force wipe without confirmation
slack-vfs wipe ./host_directory --force

# Extra secure (7 passes)
slack-vfs wipe ./host_directory --passes 7
```

#### Warning

⚠️ **THIS IS IRREVERSIBLE!** All hidden data will be permanently destroyed.

---

## Common Workflows

### Setting Up a New Hidden Storage

```bash
# 1. Create a directory with some files
mkdir my_storage
for i in {1..10}; do
    dd if=/dev/urandom of=my_storage/file_$i.dat bs=1K count=$((RANDOM % 100 + 10)) 2>/dev/null
done

# 2. Initialize the VFS
slack-vfs init my_storage

# 3. Create a directory structure
slack-vfs mkdir my_storage /passwords
slack-vfs mkdir my_storage /documents
slack-vfs mkdir my_storage /keys

# 4. Store your files
slack-vfs write my_storage /passwords/bank.txt --input ~/bank_passwords.txt
slack-vfs write my_storage /keys/ssh_key --input ~/.ssh/id_rsa
```

### Daily Usage

```bash
# Mount is automatic - just use your password
slack-vfs ls my_storage /

# Add new files
slack-vfs write my_storage /notes/$(date +%Y%m%d).txt --data "Today's note..."

# Read files
slack-vfs read my_storage /passwords/bank.txt

# Check health periodically
slack-vfs health my_storage
```

### Recovering After Host File Changes

If host files were modified:

```bash
# 1. Check health
slack-vfs health my_storage

# 2. If files are damaged, check if recoverable
# Files with < 50% symbol loss can usually be recovered
# (with default 50% redundancy)

# 3. Read and backup recoverable files
slack-vfs read my_storage /important.txt --output ~/backup/important.txt

# 4. If too much damage, wipe and reinitialize
slack-vfs wipe my_storage
slack-vfs init my_storage
```

### Secure Disposal

```bash
# 1. Securely wipe all hidden data
slack-vfs wipe my_storage --passes 7

# 2. Optionally delete the host directory
rm -rf my_storage
```

---

## Error Messages

| Error | Cause | Solution |
|-------|-------|----------|
| `VFS already initialized` | Directory already has VFS | Use existing VFS or delete `.slack_meta.json` |
| `No host files found` | Empty directory | Add files to the directory first |
| `File not found` | VFS path doesn't exist | Check path with `ls` |
| `Path already exists` | File/dir already exists | Delete first or choose different name |
| `Insufficient space` | Not enough slack space | Add more host files or delete VFS files |
| `Decryption failed` | Wrong password | Re-enter password |
| `Data corruption` | Damaged symbols | Check health; may be unrecoverable |
| `Not a directory` | Expected directory | Check path |
| `Not a file` | Expected file | Check path |

---

## Troubleshooting

### "Insufficient space" when writing

1. Check available capacity with `slack-vfs info`
2. Consider that encoding adds ~50% overhead (with default redundancy)
3. Add more host files to increase capacity
4. Delete unused VFS files to free space

### Password forgotten

Unfortunately, there is no recovery mechanism. The password is the only way to decrypt the data. Consider:
- Using a password manager
- Writing the password down and storing it securely

### Host files were modified

If external programs modified host files:
1. Run `slack-vfs health` to assess damage
2. Files with < 50% symbol loss should recover
3. Immediately backup any recoverable files
4. Re-initialize the VFS with fresh host files
5. Restore from backups

### VFS won't mount

1. Ensure `.slack_meta.json` exists in the directory
2. Verify you're using the correct password
3. Check that host files weren't moved or renamed
4. Try running with verbose output (if available)

### Performance is slow

1. Argon2id key derivation intentionally takes ~1 second for security
2. Large files with high redundancy take longer
3. Many small host files are slower than few large ones
4. Consider using SSD storage for host files
