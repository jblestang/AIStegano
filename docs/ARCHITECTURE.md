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

## Data Arborescence in Slack Space

### Overview

The VFS creates a distributed tree structure across multiple host files' slack space. Unlike traditional filesystems where data is stored contiguously, the Slack VFS fragments and distributes data using erasure coding, making it resilient to partial data loss.

### Three-Level Hierarchy

```
┌─────────────────────────────────────────────────────────────────┐
│                    Level 1: Metadata File                        │
│                   (.slack_meta.json)                             │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ • Salt for key derivation                                  │ │
│  │ • Superblock symbol locations (absolute offsets)           │ │
│  │ • Encoding parameters (source/repair symbol counts)        │ │
│  └────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Level 2: Superblock                           │
│              (Encrypted, Erasure-Coded Symbols)                  │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ • VFS metadata (UUID, version, sequence number)            │ │
│  │ • Inode table (files and directories)                      │ │
│  │ • Symbol allocation map (file_id → symbol locations)       │ │
│  │ • Host allocation tracking (logical_size, slack_used)      │ │
│  └────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Level 3: File Data                            │
│              (Encrypted, Erasure-Coded Symbols)                  │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ File 1: [Symbol 0] [Symbol 1] [Symbol 2] ...              │ │
│  │ File 2: [Symbol 3] [Symbol 4] [Symbol 5] ...              │ │
│  │ File N: [Symbol X] [Symbol Y] [Symbol Z] ...              │ │
│  └────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

### Physical Distribution Across Host Files

```
Host File A (host_0.dat)          Host File B (host_1.dat)
┌──────────────────────┐          ┌──────────────────────┐
│ Original Content     │          │ Original Content     │
│ (100 bytes)          │          │ (250 bytes)          │
├──────────────────────┤          ├──────────────────────┤
│ SB Symbol 0 (1024)   │          │ File1 Sym 1 (1024)   │
│ File1 Sym 0 (1024)   │          │ File2 Sym 0 (1024)   │
│ File1 Sym 2 (1024)   │          │ File2 Sym 2 (1024)   │
└──────────────────────┘          └──────────────────────┘

Host File C (host_2.dat)          Host File D (host_3.dat)
┌──────────────────────┐          ┌──────────────────────┐
│ Original Content     │          │ Original Content     │
│ (500 bytes)          │          │ (75 bytes)           │
├──────────────────────┤          ├──────────────────────┤
│ SB Symbol 1 (1024)   │          │ SB Symbol 2 (1024)   │
│ File1 Sym 3 (1024)   │          │ File2 Sym 1 (1024)   │
│ File2 Sym 3 (1024)   │          │ File2 Sym 4 (1024)   │
└──────────────────────┘          └──────────────────────┘

Legend:
  SB = Superblock symbol
  File1 Sym N = Symbol N of File 1
  File2 Sym N = Symbol N of File 2
```

### Symbol Allocation Strategy

The `HostManager` allocates symbols using a **high-water mark** strategy:

```rust
// For each host file, track:
pub struct HostFile {
    pub logical_size: u64,    // Original file size
    pub slack_capacity: u64,  // Total slack available
    pub used_slack: u64,      // High-water mark (max offset + length)
}

// Allocation algorithm:
fn allocate(&mut self, size: u64) -> Option<u64> {
    if self.used_slack + size <= self.slack_capacity {
        let offset = self.used_slack;  // Relative to slack start
        self.used_slack += size;       // Increment high-water mark
        Some(offset)
    } else {
        None  // Not enough space
    }
}
```

**Critical Detail:** The `used_slack` is computed as `max(offset + length)` across all symbols on that host, not as a sum of lengths. This prevents data overwrites when symbols are allocated non-contiguously.

### Superblock Symbol Tracking

The superblock maintains a complete map of all file symbols:

```rust
pub struct Superblock {
    // ... other fields ...
    pub symbols: Vec<SymbolAllocation>,
    pub hosts: HashMap<PathBuf, HostAllocation>,
}

pub struct SymbolAllocation {
    pub symbol_id: u32,
    pub file_id: InodeId,        // Which VFS file owns this symbol
    pub host_path: PathBuf,      // Which host file contains it
    pub offset: u64,             // Offset from slack start (relative)
    pub length: u32,             // Symbol size in bytes
}

pub struct HostAllocation {
    pub logical_size: u64,       // Original file size
    pub slack_used: u64,         // High-water mark for allocations
}
```

## File Reconstruction Algorithm

### Complete Read Path

```
User Request: read_file("/documents/secret.txt")
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────┐
│ Step 1: Load Metadata (.slack_meta.json)                        │
│  • Read salt for key derivation                                 │
│  • Get superblock symbol locations (absolute offsets)           │
│  • Get encoding parameters                                      │
└─────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────┐
│ Step 2: Reconstruct Superblock                                  │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ for each superblock_symbol in metadata:                    │ │
│  │   • Open host file at symbol.host_path                     │ │
│  │   • Seek to symbol.offset (absolute)                       │ │
│  │   • Read symbol.length bytes                               │ │
│  │   • Store as EncodingSymbol(id, data)                      │ │
│  └────────────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ RaptorQ Decode:                                            │ │
│  │   • Input: collected symbols                               │ │
│  │   • Output: encrypted superblock blob                      │ │
│  └────────────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ Decrypt:                                                   │ │
│  │   • Derive key from password + salt                        │ │
│  │   • AES-256-GCM decrypt                                    │ │
│  │   • Verify authentication tag                              │ │
│  └────────────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ Deserialize:                                               │ │
│  │   • bincode::deserialize → Superblock struct               │ │
│  └────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────┐
│ Step 3: Resolve Path to Inode                                   │
│  • Parse path: "/documents/secret.txt"                          │
│  • Start at root inode (ID 0)                                   │
│  • Traverse: root → "documents" → "secret.txt"                  │
│  • Result: inode_id = 5, encoding_info = {...}                  │
└─────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────┐
│ Step 4: Collect File Symbols                                    │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ symbols = superblock.get_symbols_for_file(inode_id)        │ │
│  │ // Returns all SymbolAllocations where file_id == 5        │ │
│  └────────────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ for each symbol_alloc in symbols:                          │ │
│  │   • Get logical_size from superblock.hosts[host_path]      │ │
│  │   • absolute_offset = logical_size + symbol_alloc.offset   │ │
│  │   • Open host file                                         │ │
│  │   • Seek to absolute_offset                                │ │
│  │   • Read symbol_alloc.length bytes                         │ │
│  │   • Store as EncodingSymbol(symbol_id, data)               │ │
│  └────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────┐
│ Step 5: Decode File Data                                        │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ RaptorQ Decode:                                            │ │
│  │   • Input: collected symbols (may be incomplete)           │ │
│  │   • Required: source_symbols count                         │ │
│  │   • Available: source + repair symbols                     │ │
│  │   • If available >= source: SUCCESS                        │ │
│  │   • Output: encrypted file blob                            │ │
│  └────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────┐
│ Step 6: Decrypt File Data                                       │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ Deserialize:                                               │ │
│  │   • bincode::deserialize → EncryptedData struct            │ │
│  │   • Extract: salt (32 bytes), ciphertext (variable)        │ │
│  └────────────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ Decrypt:                                                   │ │
│  │   • Derive key from password + salt                        │ │
│  │   • AES-256-GCM decrypt ciphertext                         │ │
│  │   • Verify authentication tag                              │ │
│  │   • Output: plaintext file data                            │ │
│  └────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
                    │
                    ▼
              Return plaintext
```

### Detailed Example: Reading a 3KB File

```
File: /photos/vacation.jpg (3000 bytes plaintext)

1. Encryption:
   3000 bytes → AES-256-GCM → 3028 bytes (+ nonce + tag)
   → bincode serialize EncryptedData → 3070 bytes

2. Erasure Coding (50% redundancy):
   3070 bytes ÷ 1024 bytes/symbol = 3 source symbols (3072 bytes)
   Repair symbols = 3 × 0.5 = 2 (rounded up)
   Total: 5 symbols × 1024 bytes = 5120 bytes

3. Symbol Distribution:
   Symbol 0 (id=10) → host_0.dat, offset=2048
   Symbol 1 (id=11) → host_1.dat, offset=0
   Symbol 2 (id=12) → host_0.dat, offset=3072
   Symbol 3 (id=13) → host_2.dat, offset=1024  (repair)
   Symbol 4 (id=14) → host_1.dat, offset=1024  (repair)

4. Superblock Entry:
   Inode {
     id: 7,
     name: "vacation.jpg",
     size: 3000,
     encoding_info: {
       original_length: 3070,
       source_symbols: 3,
       repair_symbols: 2,
       symbol_size: 1024
     }
   }
   
   SymbolAllocations:
   [
     { symbol_id: 10, file_id: 7, host: "host_0.dat", offset: 2048, length: 1024 },
     { symbol_id: 11, file_id: 7, host: "host_1.dat", offset: 0,    length: 1024 },
     { symbol_id: 12, file_id: 7, host: "host_0.dat", offset: 3072, length: 1024 },
     { symbol_id: 13, file_id: 7, host: "host_2.dat", offset: 1024, length: 1024 },
     { symbol_id: 14, file_id: 7, host: "host_1.dat", offset: 1024, length: 1024 }
   ]

5. Reconstruction (assuming Symbol 2 is corrupted):
   Available: Symbols 0, 1, 3, 4 (4 symbols)
   Required: 3 source symbols
   Status: ✓ Can decode (4 >= 3)
   
   RaptorQ Decode: [Sym 0, Sym 1, Sym 3, Sym 4] → 3070 bytes
   Deserialize: 3070 bytes → EncryptedData { salt, ciphertext }
   Decrypt: ciphertext → 3000 bytes plaintext
   Result: vacation.jpg recovered successfully!
```

### Resilience Characteristics

| Scenario | Symbols Lost | Recovery Status |
|----------|--------------|-----------------|
| No damage | 0/5 | ✓ Full recovery |
| Minor corruption | 1/5 (20%) | ✓ Full recovery |
| Moderate damage | 2/5 (40%) | ✓ Full recovery |
| Severe damage | 3/5 (60%) | ✗ Cannot decode |

The system can tolerate up to `repair_symbols` worth of data loss. With 50% redundancy (2 repair symbols for 3 source symbols), any 3 out of 5 symbols are sufficient for full recovery.

### Offset Calculation Details

**Critical:** Offsets are stored as **relative to slack start**, but reads use **absolute file offsets**:

```rust
// During write:
let slack_offset = host.allocate(symbol_size);  // e.g., 2048 (relative)
let absolute_offset = host.logical_size + slack_offset;  // e.g., 100 + 2048 = 2148
write_slack(&host.path, &symbol.data, absolute_offset);

// Stored in superblock:
SymbolAllocation {
    offset: slack_offset,  // 2048 (relative)
    // ...
}

// During read:
let logical_size = superblock.get_logical_size(&alloc.host_path);  // 100
let absolute_offset = logical_size + alloc.offset;  // 100 + 2048 = 2148
let data = read_slack(&alloc.host_path, absolute_offset, alloc.length);
```

This design ensures that even if the host file's logical size changes slightly, the system can still attempt recovery by using the stored `logical_size` from the superblock's `HostAllocation` map.
