# Architecture Documentation

This document provides detailed technical documentation for the Slack Space Virtual File System.

## Table of Contents

1. [System Overview](#system-overview)
2. [Layer Architecture](#layer-architecture)
3. [Data Structures](#data-structures)
4. [Algorithms](#algorithms)
5. [Security Model](#security-model)
6. [File Formats](#file-formats)

## System Overview

The Slack VFS is a layered system that provides a virtual file system interface over steganographic storage in file system slack space.

```
┌─────────────────────────────────────────────────────────────────────┐
│                          User / CLI                                  │
└─────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    Virtual File System Layer                         │
│  ┌───────────────┐ ┌───────────────┐ ┌───────────────────────────┐  │
│  │  Operations   │ │   Superblock  │ │      Inode Table          │  │
│  │  (CRUD API)   │ │   (VFS Meta)  │ │   (Files/Directories)     │  │
│  └───────────────┘ └───────────────┘ └───────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                      Encryption Layer                                │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │  Argon2id Key Derivation  →  AES-256-GCM Encryption           │  │
│  │                                                                │  │
│  │  Password  ──┬──→  Salt (32 bytes)                            │  │
│  │              └──→  Key  (32 bytes)                            │  │
│  └───────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                      Encoding Layer                                  │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                    RaptorQ Encoder                             │  │
│  │                                                                │  │
│  │  Data  ──→  Source Symbols  ──→  + Repair Symbols  ──→  Output │  │
│  │                                                                │  │
│  │  Example: 4 source + 2 repair = 50% redundancy                 │  │
│  └───────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                      Storage Layer                                   │
│  ┌───────────────┐ ┌───────────────┐ ┌───────────────────────────┐  │
│  │  Host Manager │ │  Slack R/W    │ │      Metadata             │  │
│  │  (Allocates)  │ │  (Low-level)  │ │   (.slack_meta.json)      │  │
│  └───────────────┘ └───────────────┘ └───────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     File System (Host Files)                         │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐        │
│  │ file_1  │ │ file_2  │ │ file_3  │ │ file_4  │ │ file_5  │  ...   │
│  │ [████]░ │ │ [██]░░░ │ │ [███]░░ │ │ [█]░░░░ │ │ [████]░ │        │
│  └─────────┘ └─────────┘ └─────────┘ └─────────┘ └─────────┘        │
│               █ = File Content    ░ = Slack Space (Hidden Data)     │
└─────────────────────────────────────────────────────────────────────┘
```

## Layer Architecture

### Storage Layer (`src/storage/`)

The storage layer provides low-level access to slack space with platform-specific implementations.

#### Current Implementation: File-Level I/O (`slack.rs`)

The current implementation uses file-level APIs to write past the logical EOF:

```rust
/// Calculate available slack space for a file
pub fn get_slack_capacity(path: &Path, block_size: u64) -> Result<u64>;

/// Write data to slack space (after logical EOF)
pub fn write_slack(path: &Path, data: &[u8], logical_size: u64) -> Result<()>;

/// Read data from slack space
pub fn read_slack(path: &Path, logical_size: u64, len: usize) -> Result<Vec<u8>>;

/// Wipe slack space with secure overwrite
pub fn wipe_slack(path: &Path, logical_size: u64, passes: Option<u8>) -> Result<()>;
```

#### Future: Block Device Access (`src/storage/slack_backend.rs`)

For true steganographic storage (invisible to `stat`/`ls`), raw block device access is being implemented:

```rust
/// Common trait for platform-specific slack access
pub trait SlackBackend: Send + Sync {
    fn get_slack_regions(&self, path: &Path) -> Result<Vec<SlackRegion>>;
    fn read_slack(&self, region: &SlackRegion, offset: u64, len: usize) -> Result<Vec<u8>>;
    fn write_slack(&self, region: &SlackRegion, offset: u64, data: &[u8]) -> Result<()>;
}

pub struct SlackRegion {
    pub host_path: PathBuf,
    pub block_device: PathBuf,      // /dev/sdX or /dev/rdiskN
    pub physical_block: u64,        // Block number on device
    pub offset_in_block: u64,       // Where slack starts
    pub slack_size: u64,            // Available bytes
}
```

##### Linux Implementation (`src/storage/linux/`)

- **`ext4.rs`**: Parses ext4 superblock, group descriptors, inode tables, and extent trees
- **`block_device.rs`**: O_DIRECT raw block I/O with proper alignment

```rust
// Linux: Parse ext4 to find physical block location
pub fn get_physical_block(fd: RawFd, logical_block: u64) -> Result<u64>;

// Read/write with O_DIRECT for true invisibility
pub fn read_block_direct(device: &Path, block_num: u64, block_size: u64) -> Result<Vec<u8>>;
pub fn write_block_direct(device: &Path, block_num: u64, data: &[u8]) -> Result<()>;
```

##### macOS Implementation (`src/storage/macos/`)

- **`apfs.rs`**: Uses `fcntl(F_LOG2PHYS_EXT)` to map file offsets to physical disk locations
- **`raw_disk.rs`**: Raw access to `/dev/rdiskN` character devices

```rust
// macOS: Get physical block via fcntl
pub fn get_physical_offset(file: &File, logical_offset: u64) -> Result<PhysicalMapping>;

// Access raw disk
pub fn open_raw_disk(path: &Path) -> Result<RawDiskHandle>;
```

> **Note:** Block device access requires root privileges (`sudo` or `CAP_SYS_RAWIO` on Linux).

#### `host_manager.rs` - Host File Management

Manages the collection of host files for symbol storage:

```rust
pub struct HostFile {
    pub path: PathBuf,
    pub logical_size: u64,
    pub slack_capacity: u64,
    pub used_slack: u64,
}

impl HostManager {
    pub fn scan(root: &Path, block_size: u64) -> Result<Self>;
    pub fn allocate(&mut self, size: u64) -> Result<u64>;
    pub fn total_available(&self) -> u64;
}
```

#### `metadata.rs` - Persistent Metadata

Tracks where the encrypted superblock symbols are stored. This bootstrap metadata is unencrypted but validated during recovery.

```rust
pub struct SlackMetadata {
    pub version: u32,
    pub block_size: u64,
    pub salt: Option<[u8; 32]>,  // For key derivation
    pub superblock_encoding: Option<EncodingInfo>, // RaptorQ params
    pub superblock_symbols: Vec<SymbolLocation>,   // Symbol locations
}

pub struct SymbolLocation {
    pub host_path: PathBuf,
    pub offset: u64, // Absolute offset in host file
    pub symbol_id: u32,
    pub length: u32,
}
```

### Encryption Layer (`src/crypto/`)

#### `kdf.rs` - Key Derivation

Uses Argon2id with secure parameters:

```rust
const MEMORY_COST: u32 = 65536;    // 64 MiB
const TIME_COST: u32 = 3;           // 3 iterations
const PARALLELISM: u32 = 4;         // 4 threads
const OUTPUT_LEN: usize = 32;       // 256-bit key

pub struct KeyDerivation {
    salt: [u8; 32],  // Random 256-bit salt
}

impl KeyDerivation {
    pub fn derive_key(&self, password: &str) -> Result<[u8; 32]>;
}
```

#### `cipher.rs` - Encryption

AES-256-GCM authenticated encryption:

```rust
pub struct Cipher {
    cipher: Aes256Gcm,
}

impl Cipher {
    /// Returns: nonce (12 bytes) || ciphertext || tag (16 bytes)
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>>;
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>>;
}
```

### Encoding Layer (`src/encoding/`)

#### `encoder.rs` - RaptorQ Encoding

```rust
pub struct EncodingConfig {
    pub symbol_size: u16,       // Default: 1024 bytes
    pub redundancy_ratio: f32,  // Default: 0.5 (50% extra symbols)
}

pub struct EncodedData {
    pub original_length: u64,
    pub source_symbols: usize,
    pub repair_symbols: usize,
    pub symbol_size: u16,
    pub symbols: Vec<EncodingSymbol>,
}

pub fn encode(data: &[u8], config: &EncodingConfig) -> Result<EncodedData>;
```

#### `decoder.rs` - RaptorQ Decoding

```rust
pub fn decode(encoded: &EncodedData) -> Result<Vec<u8>>;
pub fn can_decode(received: usize, required: usize) -> bool;
```

### VFS Layer (`src/vfs/`)

#### `types.rs` - Core Types

```rust
pub type InodeId = u64;

pub struct Inode {
    pub id: InodeId,
    pub name: String,
    pub inode_type: InodeType,
    pub size: u64,
    pub created: u64,
    pub modified: u64,
    pub symbol_ids: Vec<u32>,
    pub encoding_info: Option<EncodingInfo>,
}

pub enum InodeType {
    File,
    Directory { children: Vec<InodeId> },
}
```

#### `superblock.rs` - VFS Metadata

```rust
pub struct Superblock {
    pub magic: [u8; 4],           // "SVFS"
    pub version: u32,
    pub uuid: u128,              // Unique ID for this VFS
    pub sequence_number: u64,     // Monotonic version counter
    pub block_size: u64,
    pub redundancy_ratio: f32,
    pub root_inode: InodeId,
    pub next_inode_id: InodeId,
    pub inodes: HashMap<InodeId, Inode>,
    pub hosts: HashMap<PathBuf, HostAllocation>, // Host usage tracking
    pub salt: [u8; 32],
}
```

#### `operations.rs` - VFS Operations

```rust
impl SlackVfs {
    pub fn create(host_dir: &Path, password: &str, config: VfsConfig) -> Result<Self>;
    pub fn mount(host_dir: &Path, password: &str) -> Result<Self>;
    pub fn create_file(&mut self, path: &str, data: &[u8]) -> Result<InodeId>;
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>>;
    pub fn delete_file(&mut self, path: &str) -> Result<()>;
    pub fn create_dir(&mut self, path: &str) -> Result<InodeId>;
    pub fn list_dir(&self, path: &str) -> Result<Vec<DirEntry>>;
    pub fn sync(&mut self) -> Result<()>;
    pub fn health_check(&self) -> Result<HealthReport>;
}
```

## Data Structures

### Inode Structure

```
┌──────────────────────────────────────────┐
│              Inode                        │
├──────────────────────────────────────────┤
│  id: u64                                 │
│  name: String                            │
│  type: File | Directory                  │
│  size: u64                               │
│  created: u64 (Unix timestamp)           │
│  modified: u64 (Unix timestamp)          │
│  symbol_ids: Vec<u32>  (for files)       │
│  children: Vec<InodeId> (for dirs)       │
│  encoding_info: {                        │
│      original_length: u64                │
│      source_symbols: usize               │
│      repair_symbols: usize               │
│      symbol_size: u16                    │
│  }                                       │
└──────────────────────────────────────────┘
```

### Symbol Distribution

```
Host Files:        ┌─────────┐  ┌─────────┐  ┌─────────┐
                   │ Host A  │  │ Host B  │  │ Host C  │
                   │         │  │         │  │         │
Slack Space:       │ [S0,S1] │  │ [S2,S3] │  │ [S4,S5] │
                   └─────────┘  └─────────┘  └─────────┘
                         │           │           │
                         └───────────┼───────────┘
                                     │
VFS File:              ┌─────────────┴─────────────┐
                       │        secret.txt         │
                       │  Source: S0,S1,S2,S3      │
                       │  Repair: S4,S5            │
                       └───────────────────────────┘
```

## Algorithms

### Write File Algorithm

```
Input: path, plaintext data, password

1. Validate path doesn't exist
2. Encrypt data:
   - Use pre-derived key from password
   - Generate random nonce
   - Encrypt with AES-256-GCM
   - Result: nonce || ciphertext || tag

3. Encode encrypted data:
   - Split into source symbols
   - Generate repair symbols (redundancy_ratio)
   - Each symbol has unique ID

4. Store symbols:
   for each symbol:
     - Find host with available slack
     - Write to slack space
     - Record location in metadata

5. Create inode:
   - Assign new inode ID
   - Record symbol IDs and encoding info
   - Add to parent directory

6. Sync:
   - Encrypt and write superblock
   - Save metadata to .slack_meta.json
```

### Read File Algorithm

```
Input: path, password

1. Resolve path to inode ID
2. Get inode and encoding info
3. Collect symbols:
   - Get symbol locations from metadata
   - Read each symbol from slack space
   
4. Decode (RaptorQ):
   - If symbols >= source_symbols: decode
   - Else: return error (insufficient data)

5. Decrypt:
   - Derive key from password + stored salt
   - Decrypt with AES-256-GCM
   - Verify authentication tag

6. Return plaintext
```

### Health Check Algorithm

```
For each file inode:
  1. Get encoding info
  2. Count available symbols
  3. Calculate recovery status:
     - available >= required: HEALTHY
     - available < required: DAMAGED (X% loss)
  
Return health report with:
  - Total files
  - Recoverable files
  - Damaged files list
  - Capacity statistics
```

## Security Model

### Threat Model

| Threat | Protection | Notes |
|--------|------------|-------|
| Casual inspection | Steganography | Data hidden in slack space |
| Data theft (disk) | AES-256-GCM | Encrypted at rest |
| Password brute force | Argon2id | Memory-hard KDF |
| Data tampering | GCM auth tags | Detected during decryption |
| Partial data loss | RaptorQ | Recoverable with redundancy |

### Key Derivation

```
Password → Argon2id(password, salt, params) → 256-bit Key

Parameters:
  - Memory: 64 MiB
  - Iterations: 3
  - Parallelism: 4
  - Salt: 256 bits (random, stored in metadata)
```

### Encryption

```
Plaintext → AES-256-GCM.Encrypt(key, nonce, plaintext) → Ciphertext

Format: [nonce: 12 bytes][ciphertext: variable][tag: 16 bytes]
```

## File Formats

### `.slack_meta.json` (Version 3)

```json
{
  "version": 3,
  "block_size": 4096,
  "salt": [1, 2, 3, ...], // 32 bytes
  "superblock_encoding": {
    "original_length": 500,
    "source_symbols": 1,
    "repair_symbols": 1,
    "symbol_size": 1024
  },
  "superblock_symbols": [
    {
      "host_path": "/path/to/host1.dat",
      "offset": 4096,
      "length": 1024,
      "symbol_id": 0
    },
    {
      "host_path": "/path/to/host2.dat",
      "offset": 8192,
      "length": 1024,
      "symbol_id": 1
    }
  ]
}
```

> **Note:** `offset` in `superblock_symbols` is an ABSOLUTE offset from the beginning of the host file. This ensures reliable recovery even if the host file is modified or its size cannot be correctly inferred during discovery.

### Encrypted Superblock Structure

The superblock is processed in three stages:

1.  **Serialization**: The `Superblock` struct is serialized using `bincode`.
2.  **Encryption**: The serialized bytes are encrypted using AES-256-GCM.
    *   Format: `[nonce: 12 bytes][ciphertext][tag: 16 bytes]`
    *   Result length: `len(serialized) + 28 bytes`
3.  **Erasure Coding**: The encrypted blob is encoded using RaptorQ into multiple symbols.
    *   The encoding parameters (source count, symbol size) are stored in `.slack_meta.json`.

Recovery Process:
1.  Read `superblock_symbols` from `.slack_meta.json`.
2.  Read symbol data from the specified host paths and offsets.
3.  Reconstruct the encrypted blob using RaptorQ decoding.
4.  Decrypt the blob using the key derived from the password and salt.
5.  Deserialize to obtain the `Superblock` struct.

### Superblock (Encrypted)

```
[length: 4 bytes LE]
[encrypted blob: AES-256-GCM]
  |
  └─→ Decrypted content (bincode serialized):
      {
        magic: "SVFS",
        version: 1,
        block_size: 4096,
        redundancy_ratio: 0.5,
        root_inode: 0,
        next_inode_id: N,
        inodes: { ... },
        salt: [32 bytes]
      }
```

### Symbol Storage

Each symbol is stored directly in slack space:

```
Host File Layout:
┌─────────────────────────────────────────────────────────────┐
│               File Content (logical_size bytes)             │
├─────────────────────────────────────────────────────────────┤
│ Symbol 0      │ Symbol 1      │ Symbol 2      │ ...         │
│ (1024 bytes)  │ (1024 bytes)  │ (1024 bytes)  │             │
└─────────────────────────────────────────────────────────────┘
        ▲                              ▲
        │                              │
        └──────── Slack Space ─────────┘
```
