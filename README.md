# Slack Space Virtual File System

A Rust implementation of a **steganographic Virtual File System** that stores encrypted data in file system slack space, with **RaptorQ erasure coding** for resilience against partial data loss.

## What is Slack Space?

**Slack space** is the unused space between the end of a file's actual data and the end of its allocated cluster/block on disk. File systems allocate storage in fixed-size blocks (e.g., 4KB), so a 5KB file occupies two 4KB blocks, leaving 3KB of slack space in the second block.

This tool hides encrypted data in that otherwise wasted space, making it invisible to casual inspection while preserving the original host files completely intact.

## Features

- **🔒 AES-256-GCM Encryption**: Military-grade authenticated encryption with Argon2id password-based key derivation
- **📁 Virtual File System**: Full directory structure with standard file operations (create, read, delete, list)
- **🔄 RaptorQ Erasure Coding**: Recover your data even when parts are overwritten or corrupted
- **🛡️ Superblock Resilience**: 3-way replication and self-healing versioning for critical metadata
- **🖥️ CLI Interface**: Easy-to-use command-line interface for all operations
- **🔐 Password Protection**: Change passwords without re-encrypting all data
- **✨ Steganographic**: Host files remain fully functional and unchanged

## Architecture

```
Data Flow:
  Write: Data → Encrypt (AES-256-GCM) → Encode (RaptorQ) → Store (Slack Space)
  Read:  Slack Space → Collect Symbols → Decode (RaptorQ) → Decrypt → Data
```

```
┌─────────────────────────────────────────────────────────────┐
│                      CLI Interface                          │
├─────────────────────────────────────────────────────────────┤
│                   Virtual File System                        │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │  VFS Ops    │  │  Directory  │  │  File Allocation    │  │
│  │  (CRUD)     │  │  Structure  │  │  Table (FAT)        │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│                   Encryption Layer                           │
│  ┌─────────────────────────────────────────────────────────┐│
│  │  AES-256-GCM + Argon2id Key Derivation                  ││
│  └─────────────────────────────────────────────────────────┘│
├─────────────────────────────────────────────────────────────┤
│                   Encoding Layer                             │
│  ┌─────────────────────────────────────────────────────────┐│
│  │              RaptorQ Encoder/Decoder                     ││
│  │  (Configurable redundancy for data recovery)             ││
│  └─────────────────────────────────────────────────────────┘│
├─────────────────────────────────────────────────────────────┤
│                   Storage Layer                              │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────────┐  │
│  │ Slack Writer │  │ Slack Reader │  │ Slack Wiper       │  │
│  └──────────────┘  └──────────────┘  └───────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/user/slack-vfs.git
cd slack-vfs

# Build in release mode
cargo build --release

# Install to your path (optional)
cargo install --path .
```

### Requirements

- Rust 1.70 or later
- A directory containing files with sufficient slack space

## Quick Start

### 1. Prepare Host Files

Create a directory with some files that will host the hidden data:

```bash
mkdir host_directory
# Create some files with content (the more files, the more hidden storage)
for i in {1..10}; do
    dd if=/dev/urandom of=host_directory/file_$i.dat bs=1K count=$((RANDOM % 100 + 10)) 2>/dev/null
done
```

### 2. Initialize the VFS

```bash
slack-vfs init ./host_directory
# You will be prompted for a password
```

### 3. Store Hidden Files

```bash
# Write a secret file
slack-vfs write ./host_directory /secret/passwords.txt --input ./my_passwords.txt

# Or write inline data
slack-vfs write ./host_directory /notes/secret.txt --data "Top secret information"

# Create directories
slack-vfs mkdir ./host_directory /documents/confidential
```

### 4. Retrieve Hidden Files

```bash
# List contents
slack-vfs ls ./host_directory /

# Read a file
slack-vfs read ./host_directory /secret/passwords.txt --output ./recovered.txt

# Or display to stdout
slack-vfs read ./host_directory /notes/secret.txt
```

### 5. Manage the VFS

```bash
# Check VFS health
slack-vfs health ./host_directory

# Show VFS info
slack-vfs info ./host_directory

# Change password
slack-vfs passwd ./host_directory

# Securely wipe all hidden data
slack-vfs wipe ./host_directory
```

## CLI Commands

| Command  | Description                           |
|----------|---------------------------------------|
| `init`   | Initialize a new VFS in a directory   |
| `ls`     | List VFS directory contents           |
| `write`  | Write a file to the VFS               |
| `read`   | Read a file from the VFS              |
| `rm`     | Delete a file from the VFS            |
| `mkdir`  | Create a directory in the VFS         |
| `info`   | Show VFS status and capacity          |
| `health` | Run health check on the VFS           |
| `wipe`   | Securely wipe all VFS data            |
| `passwd` | Change the VFS password               |

For detailed usage, run `slack-vfs <command> --help`.

## Configuration Options

When initializing a VFS, you can customize:

| Option           | Default | Description                              |
|------------------|---------|------------------------------------------|
| `--block-size`   | 4096    | File system block size in bytes          |
| `--redundancy`   | 0.5     | Erasure coding redundancy (0.0 - 1.0)    |
| `--symbol-size`  | 1024    | RaptorQ symbol size in bytes             |

Example:
```bash
slack-vfs init ./host_directory --block-size 4096 --redundancy 0.5
```

## Security Considerations

### What This Tool Provides

- **Confidentiality**: Data is encrypted with AES-256-GCM
- **Authentication**: Tampering is detected via GCM auth tags
- **Key Security**: Argon2id with secure parameters for key derivation
- **Resilience**: RaptorQ encoding allows recovery from partial data loss

### Limitations

- **Metadata is Visible**: The `.slack_meta.json` file is not hidden
- **Not Forensically Secure**: Advanced forensic analysis may detect slack space usage
- **Host File Modification**: If host files are modified, some hidden data may be lost
- **No Plausible Deniability**: This is not a true deniable encryption system

### Best Practices

1. **Use Strong Passwords**: The encryption is only as secure as your password
2. **Backup Important Data**: Slack space storage is inherently fragile
3. **Monitor Health**: Regularly check VFS health if host files may be modified
4. **Secure Wipe**: Always use `wipe` command before deleting the host directory

## How It Works

### Slack Space Storage

When a file is written to disk, it typically doesn't fill its last allocated block completely. For example:

```
┌────────────────────┬────────────────────┐
│    File Content    │    Slack Space     │
│    (3000 bytes)    │    (1096 bytes)    │
├────────────────────┼────────────────────┤
│ [████████████████] │ [░░░░░░░░░░░░░░░░] │
└────────────────────┴────────────────────┘
                Block (4096 bytes)
```

This tool writes encrypted, encoded data into that slack space.

### RaptorQ Erasure Coding

Data is split into symbols and encoded with additional repair symbols:

```
Original:  [A] [B] [C] [D]  (4 source symbols)
Encoded:   [A] [B] [C] [D] [R1] [R2]  (6 total with 50% redundancy)
```

If any 4 symbols are available, the original data can be recovered. This means:
- 10% data loss → Full recovery ✓
- 30% data loss → Full recovery ✓
- 50%+ data loss → Recovery may fail ✗

## License

MIT License - See [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome! Please feel free to submit pull requests.

## Acknowledgments

- [RaptorQ RFC 6330](https://tools.ietf.org/html/rfc6330) for the erasure coding standard
- [argon2](https://crates.io/crates/argon2) for secure password hashing
- [aes-gcm](https://crates.io/crates/aes-gcm) for authenticated encryption
