# HOWTO: Safe Testing with Disk Images

This guide shows how to safely experiment with the Slack VFS using disk images instead of your real file system.

## Table of Contents

1. [Quick Start](#quick-start)
2. [Linux: Create ext4 Disk Image](#linux-create-ext4-disk-image)
3. [macOS: Create APFS/HFS+ Disk Image](#macos-create-apfshfs-disk-image)
4. [Using the VFS](#using-the-vfs)
5. [Cleanup](#cleanup)

---

## Quick Start

```bash
# Build the project
cargo build --release

# Create a test environment (uses temp files, safe)
./scripts/create_test_env.sh
```

---

## Linux: Create ext4 Disk Image

### Step 1: Create and Format the Image

```bash
# Create a 100MB disk image file
dd if=/dev/zero of=test_disk.img bs=1M count=100

# Format as ext4
mkfs.ext4 test_disk.img

# Create mount point
sudo mkdir -p /mnt/slack_test

# Mount the image
sudo mount -o loop test_disk.img /mnt/slack_test

# Set permissions for your user
sudo chown -R $(whoami):$(whoami) /mnt/slack_test
```

### Step 2: Populate with Host Files

```bash
# Create host files that will contain hidden data
for i in {1..20}; do
    # Create files with random content (100-500 bytes)
    # Smaller files = more slack space per file
    dd if=/dev/urandom of=/mnt/slack_test/host_$i.dat bs=1 count=$((100 + RANDOM % 400)) 2>/dev/null
done

# Verify files were created
ls -la /mnt/slack_test/
```

### Step 3: Use the VFS

```bash
# Initialize the VFS
./target/release/slack-vfs init /mnt/slack_test
# Enter password when prompted

# Check available capacity
./target/release/slack-vfs info /mnt/slack_test

# Store a secret file
./target/release/slack-vfs write /mnt/slack_test /secrets/password.txt --data "my_secret_password"

# List hidden files
./target/release/slack-vfs ls /mnt/slack_test /

# Read back the secret
./target/release/slack-vfs read /mnt/slack_test /secrets/password.txt
```

### Step 4: Cleanup

```bash
# Unmount
sudo umount /mnt/slack_test

# Remove mount point
sudo rmdir /mnt/slack_test

# Delete disk image
rm test_disk.img
```

---

## macOS: Create APFS/HFS+ Disk Image

### Option A: Using Disk Utility (GUI)

1. Open **Disk Utility** (`/Applications/Utilities/Disk Utility.app`)
2. **File → New Image → Blank Image...**
3. Configure:
   - **Name**: `slack_test`
   - **Size**: 100 MB
   - **Format**: APFS or Mac OS Extended (Journaled)
   - **Partitions**: Single partition - GUID Partition Map
   - **Image Format**: read/write disk image
4. Click **Save**
5. The image will be mounted automatically at `/Volumes/slack_test`

### Option B: Using Command Line

```bash
# Create a 100MB sparse disk image (APFS)
hdiutil create -size 100m -fs APFS -volname slack_test -type SPARSE slack_test.sparseimage

# Attach (mount) the image
hdiutil attach slack_test.sparseimage

# The volume is now mounted at /Volumes/slack_test
```

### Alternative: HFS+ Format

```bash
# Create HFS+ image (more predictable block allocation)
hdiutil create -size 100m -fs HFS+ -volname slack_test slack_test.dmg

# Mount it
hdiutil attach slack_test.dmg
```

### Step 2: Populate with Host Files

```bash
# Create host files
for i in {1..20}; do
    dd if=/dev/urandom of=/Volumes/slack_test/host_$i.dat bs=1 count=$((100 + RANDOM % 400)) 2>/dev/null
done

# Verify
ls -la /Volumes/slack_test/
```

### Step 3: Use the VFS

```bash
# Initialize
./target/release/slack-vfs init /Volumes/slack_test

# Store secrets
./target/release/slack-vfs write /Volumes/slack_test /documents/secret.txt --data "Top secret information"

# Create directory structure
./target/release/slack-vfs mkdir /Volumes/slack_test /photos
./target/release/slack-vfs write /Volumes/slack_test /photos/hidden.jpg --input ~/Desktop/photo.jpg

# Check health
./target/release/slack-vfs health /Volumes/slack_test
```

### Step 4: Cleanup

```bash
# Eject the disk image
hdiutil detach /Volumes/slack_test

# Delete the image file
rm slack_test.sparseimage  # or slack_test.dmg
```

---

## Using the VFS

### Common Operations

```bash
# Set your VFS path
export VFS_PATH="/mnt/slack_test"  # Linux
export VFS_PATH="/Volumes/slack_test"  # macOS

# Initialize new VFS
slack-vfs init $VFS_PATH

# Show info
slack-vfs info $VFS_PATH

# Create directories
slack-vfs mkdir $VFS_PATH /documents
slack-vfs mkdir $VFS_PATH /photos
slack-vfs mkdir $VFS_PATH /documents/work

# Write files
echo "Secret notes" | slack-vfs write $VFS_PATH /documents/notes.txt --stdin
slack-vfs write $VFS_PATH /photos/vacation.jpg --input ~/photos/vacation.jpg

# List directory
slack-vfs ls $VFS_PATH /
slack-vfs ls $VFS_PATH /documents

# Read files
slack-vfs read $VFS_PATH /documents/notes.txt
slack-vfs read $VFS_PATH /photos/vacation.jpg --output recovered.jpg

# Delete files
slack-vfs rm $VFS_PATH /documents/notes.txt

# Health check
slack-vfs health $VFS_PATH

# Securely wipe all hidden data
slack-vfs wipe $VFS_PATH
```

### Verify Steganography

```bash
# Check that host files are unchanged
ls -la $VFS_PATH/host_*.dat

# The file sizes should be the same as before VFS operations
# Hidden data is stored AFTER the logical file end

# You can still read host files normally
cat $VFS_PATH/host_1.dat  # Shows original content, not hidden data
```

---

## Cleanup

### Linux

```bash
sudo umount /mnt/slack_test 2>/dev/null
sudo rmdir /mnt/slack_test 2>/dev/null
rm -f test_disk.img
```

### macOS

```bash
hdiutil detach /Volumes/slack_test 2>/dev/null
rm -f slack_test.sparseimage slack_test.dmg
```

---

## Troubleshooting

### "No host files found"

Make sure you have created host files in the directory:
```bash
ls -la $VFS_PATH/*.dat
```

### "Insufficient space"

Your host files may be too large (no slack space) or too few:
```bash
# Check slack space
slack-vfs info $VFS_PATH
```
Create more small files to increase available slack.

### Permission denied (Linux)

Mount with correct permissions:
```bash
sudo mount -o loop,uid=$(id -u),gid=$(id -g) test_disk.img /mnt/slack_test
```

### Disk image won't mount (macOS)

Try attaching with verbose output:
```bash
hdiutil attach -verbose slack_test.dmg
```

---

## Example Session

```bash
# Complete example session on macOS

# 1. Create disk image
hdiutil create -size 50m -fs HFS+ -volname test_vfs test_vfs.dmg
hdiutil attach test_vfs.dmg

# 2. Create host files
cd /Volumes/test_vfs
for i in {1..15}; do
    dd if=/dev/urandom of=file_$i.bin bs=1 count=$((50 + RANDOM % 200)) 2>/dev/null
done

# 3. Build and initialize VFS
cd ~/AIStegano
cargo build --release
./target/release/slack-vfs init /Volumes/test_vfs
# Enter password: mypassword123

# 4. Store some secrets
./target/release/slack-vfs mkdir /Volumes/test_vfs /private
./target/release/slack-vfs write /Volumes/test_vfs /private/api_keys.txt --data "API_KEY=abc123"
./target/release/slack-vfs write /Volumes/test_vfs /private/notes.md --data "# Secret Notes\n\nDon't share this!"

# 5. Verify
./target/release/slack-vfs ls /Volumes/test_vfs /private
./target/release/slack-vfs read /Volumes/test_vfs /private/api_keys.txt

# 6. Check health
./target/release/slack-vfs health /Volumes/test_vfs

# 7. Cleanup
./target/release/slack-vfs wipe /Volumes/test_vfs
hdiutil detach /Volumes/test_vfs
rm test_vfs.dmg
```
