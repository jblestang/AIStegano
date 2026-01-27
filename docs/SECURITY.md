# Security Documentation

Comprehensive security analysis and guidance for the Slack Space Virtual File System.

## Table of Contents

1. [Threat Model](#threat-model)
2. [Encryption Details](#encryption-details)
3. [Key Derivation](#key-derivation)
4. [Authentication](#authentication)
5. [Data Integrity](#data-integrity)
6. [What We Protect Against](#what-we-protect-against)
7. [Known Limitations](#known-limitations)
8. [Best Practices](#best-practices)
9. [Security Comparison](#security-comparison)

## Threat Model

### Assets

| Asset | Description | Protection Level |
|-------|-------------|------------------|
| File Contents | User's hidden files | High (encrypted) |
| Directory Structure | VFS layout | High (encrypted) |
| Metadata | Symbol locations | Medium (not encrypted) |
| Encryption Key | Derived from password | High (memory only) |

### Adversaries

| Adversary | Capabilities | Our Protection |
|-----------|--------------|----------------|
| Casual User | Basic file browsing | ✅ Hidden in slack space |
| System Admin | File system access | ⚠️ Encrypted, but detectable |
| Forensic Analyst | Deep disk analysis | ⚠️ Encrypted, pattern detectable |
| Law Enforcement | Full disk analysis + subpoena | ⚠️ No plausible deniability |
| Nation State | Advanced cryptanalysis | ✅ AES-256 is quantum-resistant |

### Threat Categories

```
┌─────────────────────────────────────────────────────────────┐
│                    PROTECTED AGAINST                         │
├─────────────────────────────────────────────────────────────┤
│  ✓ Data theft from disk/backup                              │
│  ✓ Unauthorized access to hidden files                      │
│  ✓ Password brute-force attacks                             │
│  ✓ Data tampering detection                                 │
│  ✓ Partial data loss/corruption                             │
│  ✓ Casual/accidental discovery                              │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                    NOT PROTECTED AGAINST                     │
├─────────────────────────────────────────────────────────────┤
│  ✗ Forensic detection of slack space usage                  │
│  ✗ Memory capture while VFS is mounted                      │
│  ✗ Coercive password extraction                             │
│  ✗ Detection of .slack_meta.json file                       │
│  ✗ Traffic analysis of file operations                      │
└─────────────────────────────────────────────────────────────┘
```

## Encryption Details

### Algorithm: AES-256-GCM

| Property | Value |
|----------|-------|
| Algorithm | AES (Advanced Encryption Standard) |
| Key Size | 256 bits |
| Mode | GCM (Galois/Counter Mode) |
| Nonce Size | 96 bits (12 bytes) |
| Tag Size | 128 bits (16 bytes) |

### Why AES-256-GCM?

1. **Proven Security**: AES is the US government standard for classified information
2. **Hardware Acceleration**: Modern CPUs have AES-NI instructions
3. **Authenticated**: GCM provides integrity and authenticity
4. **No Padding Oracle**: Stream cipher mode, no padding attacks
5. **Quantum Resistant**: 256-bit keys resist Grover's algorithm

### Encryption Process

```
Plaintext
    │
    ▼
┌───────────────────────────────────────┐
│         AES-256-GCM Encrypt           │
│  ┌─────────┐  ┌─────────┐            │
│  │  Key    │  │  Nonce  │            │
│  │(256-bit)│  │(96-bit) │            │
│  └────┬────┘  └────┬────┘            │
│       │            │                  │
│       ▼            ▼                  │
│  ┌──────────────────────────────────┐│
│  │     AES-256-GCM Cipher           ││
│  └──────────────────────────────────┘│
└───────────────────────────────────────┘
    │
    ▼
┌─────────┬──────────────────┬─────────┐
│  Nonce  │    Ciphertext    │   Tag   │
│(12 bytes)│    (variable)   │(16 bytes)│
└─────────┴──────────────────┴─────────┘
```

### Nonce Handling

- **Generated**: Using `rand::thread_rng()` (cryptographically secure)
- **Size**: 12 bytes (96 bits)
- **Uniqueness**: Random per encryption operation
- **Storage**: Prepended to ciphertext

> ⚠️ **Important**: With random nonces, the probability of collision is negligible
> for reasonable amounts of data (birthday bound: ~2^48 encryptions before concern).

## Key Derivation

### Algorithm: Argon2id

| Parameter | Value | Purpose |
|-----------|-------|---------|
| Algorithm | Argon2id | Hybrid: timing + memory-hard |
| Memory | 64 MiB | Resist GPU/ASIC attacks |
| Iterations | 3 | Increase computation time |
| Parallelism | 4 | Utilize multiple cores |
| Output | 256 bits | AES-256 key material |
| Salt | 256 bits | Per-VFS unique |

### Why Argon2id?

1. **Winner of PHC**: Password Hashing Competition winner (2015)
2. **Memory-Hard**: Expensive for GPU/ASIC attackers
3. **Hybrid**: Combines timing resistance (Argon2i) with GPU resistance (Argon2d)
4. **Configurable**: Adjustable cost parameters
5. **Modern**: Designed for contemporary threats

### Derivation Process

```
Password (user input)
        │
        ▼
┌─────────────────────────────────────┐
│          Argon2id                   │
│  ┌─────────────────────────────────┐│
│  │  Salt: 256 bits (random)        ││
│  │  Memory: 64 MiB                 ││
│  │  Iterations: 3                  ││
│  │  Parallelism: 4                 ││
│  └─────────────────────────────────┘│
└─────────────────────────────────────┘
        │
        ▼
256-bit Encryption Key
```

### Salt Storage

- Salt is stored in `.slack_meta.json` (unencrypted)
- This is necessary to derive the same key on subsequent mounts
- The salt does NOT reveal the password or key

### Timing

With default parameters, key derivation takes approximately:
- Modern CPU: ~1 second
- This is intentional to slow brute-force attacks

## Authentication

### GCM Authentication Tag

Every encrypted block includes a 128-bit authentication tag that:

1. **Verifies Integrity**: Detects any bit flips or modifications
2. **Verifies Authenticity**: Confirms data was encrypted with the correct key
3. **Fails Loudly**: Decryption fails if tag doesn't match

### Verification Order

```
┌─────────────────────────────────────┐
│  1. Derive key from password        │
│  2. Attempt decryption              │
│  3. Verify GCM authentication tag   │
│     │                               │
│     ├── Tag matches ──→ Success     │
│     │                               │
│     └── Tag mismatch ──→ Fail       │
│         (wrong password or          │
│          data corruption)           │
└─────────────────────────────────────┘
```

## Data Integrity

### RaptorQ Erasure Coding

Provides resilience against data loss (not a security feature):

```
With 50% redundancy:
  - Can lose up to 33% of symbols and still recover
  - Beyond that, recovery fails

Example:
  4 source symbols + 2 repair symbols = 6 total
  Need any 4 to recover original data
```

### Integrity vs Authenticity

| Concern | Solution |
|---------|----------|
| Bit flips in storage | RaptorQ can correct |
| Malicious tampering | GCM detects (fails) |
| Partial symbol loss | RaptorQ recovers |
| Complete symbol loss | Depends on redundancy |

## What We Protect Against

### ✅ Data at Rest

If an attacker obtains your disk or backup:
- All file contents are encrypted
- Without password, data appears random
- No information about file names, sizes, or structure

### ✅ Weak Password Attempts

Argon2id makes brute-force expensive:
- 1 second per attempt on target hardware
- Memory requirement defeats GPU parallelism
- Dictionary attacks are significantly slowed

### ✅ Tampering Detection

If an attacker modifies encrypted data:
- GCM authentication fails
- User is alerted that data is corrupted
- Partial modifications are detected

### ✅ Accidental Discovery

Hidden data is not visible through:
- Normal file browsing
- File size inspection
- Basic hex viewing

### ✅ Data Corruption

RaptorQ provides recovery from:
- Bad disk sectors
- Partial overwrites
- Bit rot

## Known Limitations

### ⚠️ Metadata is Visible

The `.slack_meta.json` file is NOT encrypted, but now contains MINIMAL information:
- **Salt** (for key derivation)
- **Block Size**
- **Pointers** to encrypted superblock replicas

**Improvement**: Detailed host usage, symbol locations, and directory structure are now moved INSIDE the encrypted superblock. However, the presence of the VFS is still visible via this bootstrap file.

**Mitigation**: Hide/encrypt the metadata file separately if needed.

### ⚠️ Pattern Detection

Forensic analysis may detect:
- Slack space with high entropy (random-looking) data
- Regular patterns in symbol distribution
- Metadata file presence

**Mitigation**: None built-in. Consider full disk encryption as outer layer.

### ⚠️ No Plausible Deniability

This is NOT a deniable encryption system:
- Cannot claim encrypted data doesn't exist
- Metadata file proves VFS presence
- No hidden volumes

**Mitigation**: Use VeraCrypt hidden volumes if deniability is required.

### ⚠️ Memory Exposure

While the VFS is mounted:
- Key is in process memory
- Could be extracted via memory dump
- Cold boot attacks may recover key

**Mitigation**: 
- Unmount when not in use
- Enable full disk encryption with secure boot
- Use memory-encrypted CPUs if available

### ⚠️ Side Channels

No protection against:
- Timing analysis during operations
- Power analysis during encryption
- Electromagnetic emanations

**Mitigation**: These attacks require physical access; focus on physical security.

## Best Practices

### Password Selection

| Practice | Reason |
|----------|--------|
| Use 16+ characters | Increases keyspace |
| Mix case, numbers, symbols | Defeats dictionary attacks |
| Avoid personal information | Prevents targeted guessing |
| Use a password manager | Enables strong, unique passwords |
| Never reuse passwords | Limits breach impact |

### Operational Security

| Practice | Reason |
|----------|--------|
| Use on encrypted disk | Defense in depth |
| Close VFS when not in use | Reduces exposure window |
| Secure wipe before disposal | Prevent data recovery |
| Regular health checks | Detect corruption early |
| Backup important data | Recovery from loss |

### Physical Security

| Practice | Reason |
|----------|--------|
| Lock workstation | Prevent unauthorized access |
| Enable secure boot | Prevent OS tampering |
| Use full disk encryption | Protect all data, not just VFS |
| Disable hibernation | Prevent key leakage |

## Security Comparison

### vs. VeraCrypt Hidden Volumes

| Feature | Slack VFS | VeraCrypt Hidden |
|---------|-----------|------------------|
| Encryption | AES-256-GCM | Multiple options |
| Deniability | ❌ No | ✅ Yes |
| Steganography | ✅ Yes | ❌ No |
| Setup complexity | Low | Medium |
| Space efficiency | Low | High |
| Resilience | ✅ RaptorQ | ❌ None |

### vs. EncFS / CryFS

| Feature | Slack VFS | EncFS/CryFS |
|---------|-----------|-------------|
| Encryption | AES-256-GCM | Various |
| Hidden storage | ✅ Slack space | ❌ Visible files |
| Metadata visible | Partial | Yes |
| Cloud sync | ❌ No | ✅ Yes |
| Performance | Lower | Higher |

### vs. Full Disk Encryption

| Feature | Slack VFS | FDE (BitLocker/LUKS) |
|---------|-----------|----------------------|
| Scope | Specific files | Entire disk |
| Steganography | ✅ Yes | ❌ No |
| Boot protection | ❌ No | ✅ Yes |
| Performance | Lower | Minimal impact |
| Usability | CLI tool | Transparent |

**Recommendation**: Use Slack VFS *in addition to* full disk encryption for defense in depth.

---

## Cryptographic References

- **AES**: NIST FIPS 197
- **GCM**: NIST SP 800-38D
- **Argon2**: RFC 9106
- **RaptorQ**: RFC 6330

## Audit Status

This is an educational/hobbyist project and has NOT been:
- Professionally audited
- Formally verified
- Penetration tested

**Use at your own risk for sensitive data.**
