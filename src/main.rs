//! Slack VFS - A steganographic Virtual File System.
//!
//! Stores encrypted data in file system slack space with erasure coding
//! for resilience against partial data loss.

use clap::{Parser, Subcommand};
use slack_vfs::{Result, SlackVfs, VfsConfig};
use std::io::{self, Read, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "slack-vfs")]
#[command(author, version, about, long_about = None)]
#[command(
    about = "Steganographic Virtual File System using slack space",
    long_about = "A VFS that stores encrypted data in file system slack space with RaptorQ erasure coding for resilience."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new VFS in the given directory
    Init {
        /// Directory containing host files
        host_dir: PathBuf,

        /// Block size for slack calculation (default: 4096)
        #[arg(long, default_value = "4096")]
        block_size: u64,

        /// Redundancy ratio for encoding (default: 0.5 = 50% extra symbols)
        #[arg(long, default_value = "0.5")]
        redundancy: f32,

        /// Symbol size for encoding (default: 1024)
        #[arg(long, default_value = "1024")]
        symbol_size: u16,
    },

    /// List VFS directory contents
    Ls {
        /// Directory containing host files
        host_dir: PathBuf,

        /// VFS path to list (default: /)
        #[arg(default_value = "/")]
        vfs_path: String,
    },

    /// Write a file to the VFS
    Write {
        /// Directory containing host files
        host_dir: PathBuf,

        /// VFS path for the new file
        vfs_path: String,

        /// Input file to write
        #[arg(long, conflicts_with = "data")]
        input: Option<PathBuf>,

        /// String data to write
        #[arg(long, conflicts_with = "input")]
        data: Option<String>,
    },

    /// Read a file from the VFS
    Read {
        /// Directory containing host files
        host_dir: PathBuf,

        /// VFS path to read
        vfs_path: String,

        /// Output file (default: stdout)
        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// Delete a file from the VFS
    Rm {
        /// Directory containing host files
        host_dir: PathBuf,

        /// VFS path to delete
        vfs_path: String,
    },

    /// Create a directory in the VFS
    Mkdir {
        /// Directory containing host files
        host_dir: PathBuf,

        /// VFS path for the new directory
        vfs_path: String,
    },

    /// Show VFS status and capacity
    Info {
        /// Directory containing host files
        host_dir: PathBuf,
    },

    /// Run health check on VFS
    Health {
        /// Directory containing host files
        host_dir: PathBuf,
    },

    /// Securely wipe all VFS data
    Wipe {
        /// Directory containing host files
        host_dir: PathBuf,

        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Change VFS password
    Passwd {
        /// Directory containing host files
        host_dir: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Init {
            host_dir,
            block_size,
            redundancy,
            symbol_size,
        } => cmd_init(&host_dir, block_size, redundancy, symbol_size),

        Commands::Ls { host_dir, vfs_path } => cmd_ls(&host_dir, &vfs_path),

        Commands::Write {
            host_dir,
            vfs_path,
            input,
            data,
        } => cmd_write(&host_dir, &vfs_path, input, data),

        Commands::Read {
            host_dir,
            vfs_path,
            output,
        } => cmd_read(&host_dir, &vfs_path, output),

        Commands::Rm { host_dir, vfs_path } => cmd_rm(&host_dir, &vfs_path),

        Commands::Mkdir { host_dir, vfs_path } => cmd_mkdir(&host_dir, &vfs_path),

        Commands::Info { host_dir } => cmd_info(&host_dir),

        Commands::Health { host_dir } => cmd_health(&host_dir),

        Commands::Wipe { host_dir, force } => cmd_wipe(&host_dir, force),

        Commands::Passwd { host_dir } => cmd_passwd(&host_dir),
    }
}

fn prompt_password(prompt: &str) -> String {
    rpassword::prompt_password(prompt).unwrap_or_else(|_| {
        eprint!("{}", prompt);
        io::stderr().flush().unwrap();
        let mut password = String::new();
        io::stdin().read_line(&mut password).unwrap();
        password.trim().to_string()
    })
}

fn cmd_init(host_dir: &PathBuf, block_size: u64, redundancy: f32, symbol_size: u16) -> Result<()> {
    let password = prompt_password("Enter password: ");
    let confirm = prompt_password("Confirm password: ");

    if password != confirm {
        eprintln!("Passwords do not match");
        std::process::exit(1);
    }

    let config = VfsConfig::new(block_size, symbol_size, redundancy);
    let vfs = SlackVfs::create(host_dir, &password, config)?;
    let info = vfs.info();

    println!("VFS initialized successfully!");
    println!("  Host files: {}", info.host_count);
    println!("  Total capacity: {} bytes", info.total_capacity);
    println!("  Block size: {} bytes", info.block_size);
    println!("  Redundancy: {:.0}%", info.redundancy_ratio * 100.0);

    Ok(())
}

fn cmd_ls(host_dir: &PathBuf, vfs_path: &str) -> Result<()> {
    let password = prompt_password("Password: ");
    let vfs = SlackVfs::mount(host_dir, &password)?;

    let entries = vfs.list_dir(vfs_path)?;

    if entries.is_empty() {
        println!("(empty)");
    } else {
        for entry in entries {
            let type_char = if entry.is_dir { 'd' } else { '-' };
            let size = if entry.is_dir {
                "-".to_string()
            } else {
                format!("{}", entry.size)
            };
            println!("{} {:>10}  {}", type_char, size, entry.name);
        }
    }

    Ok(())
}

fn cmd_write(
    host_dir: &PathBuf,
    vfs_path: &str,
    input: Option<PathBuf>,
    data: Option<String>,
) -> Result<()> {
    let password = prompt_password("Password: ");
    let mut vfs = SlackVfs::mount(host_dir, &password)?;

    let content = match (input, data) {
        (Some(path), None) => std::fs::read(&path)?,
        (None, Some(s)) => s.into_bytes(),
        (None, None) => {
            // Read from stdin
            let mut buffer = Vec::new();
            io::stdin().read_to_end(&mut buffer)?;
            buffer
        }
        (Some(_), Some(_)) => unreachable!(),
    };

    vfs.create_file(vfs_path, &content)?;
    println!("Wrote {} bytes to {}", content.len(), vfs_path);

    Ok(())
}

fn cmd_read(host_dir: &PathBuf, vfs_path: &str, output: Option<PathBuf>) -> Result<()> {
    let password = prompt_password("Password: ");
    let vfs = SlackVfs::mount(host_dir, &password)?;

    let data = vfs.read_file(vfs_path)?;

    match output {
        Some(path) => {
            std::fs::write(&path, &data)?;
            println!("Wrote {} bytes to {}", data.len(), path.display());
        }
        None => {
            io::stdout().write_all(&data)?;
        }
    }

    Ok(())
}

fn cmd_rm(host_dir: &PathBuf, vfs_path: &str) -> Result<()> {
    let password = prompt_password("Password: ");
    let mut vfs = SlackVfs::mount(host_dir, &password)?;

    vfs.delete_file(vfs_path)?;
    println!("Deleted {}", vfs_path);

    Ok(())
}

fn cmd_mkdir(host_dir: &PathBuf, vfs_path: &str) -> Result<()> {
    let password = prompt_password("Password: ");
    let mut vfs = SlackVfs::mount(host_dir, &password)?;

    vfs.create_dir(vfs_path)?;
    println!("Created directory {}", vfs_path);

    Ok(())
}

fn cmd_info(host_dir: &PathBuf) -> Result<()> {
    let password = prompt_password("Password: ");
    let vfs = SlackVfs::mount(host_dir, &password)?;
    let info = vfs.info();

    println!("Slack VFS Information");
    println!("=====================");
    println!("Host directory:   {}", info.host_dir.display());
    println!("Host files:       {}", info.host_count);
    println!("Block size:       {} bytes", info.block_size);
    println!("Redundancy:       {:.0}%", info.redundancy_ratio * 100.0);
    println!();
    println!("Capacity:");
    println!("  Total:          {} bytes", info.total_capacity);
    println!("  Used:           {} bytes", info.used_capacity);
    println!("  Available:      {} bytes", info.available_capacity);
    println!();
    println!("Contents:");
    println!("  Directories:    {}", info.dir_count);
    println!("  Files:          {}", info.file_count);
    println!("  Total size:     {} bytes", info.total_file_size);

    Ok(())
}

fn cmd_health(host_dir: &PathBuf) -> Result<()> {
    let password = prompt_password("Password: ");
    let vfs = SlackVfs::mount(host_dir, &password)?;
    let report = vfs.health_check()?;

    println!("VFS Health Report");
    println!("=================");
    println!("Host files:       {}", report.host_count);
    println!("Total capacity:   {} bytes", report.total_capacity);
    println!("Used capacity:    {} bytes", report.used_capacity);
    println!();
    println!("File Status:");
    println!("  Total files:    {}", report.total_files);
    println!("  Recoverable:    {}", report.recoverable_files);

    if !report.damaged_files.is_empty() {
        println!();
        println!("Damaged Files:");
        for (name, loss_percent) in &report.damaged_files {
            println!("  {} ({:.1}% symbols lost)", name, loss_percent);
        }
    } else if report.total_files > 0 {
        println!();
        println!("✓ All files are intact and recoverable");
    }

    Ok(())
}

fn cmd_wipe(host_dir: &PathBuf, force: bool) -> Result<()> {
    if !force {
        eprint!("This will permanently destroy all VFS data. Continue? [y/N] ");
        io::stderr().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted");
            return Ok(());
        }
    }

    let password = prompt_password("Password: ");
    let mut vfs = SlackVfs::mount(host_dir, &password)?;

    vfs.wipe()?;
    println!("VFS data securely wiped");

    Ok(())
}

fn cmd_passwd(host_dir: &PathBuf) -> Result<()> {
    let old_password = prompt_password("Current password: ");
    let new_password = prompt_password("New password: ");
    let confirm = prompt_password("Confirm new password: ");

    if new_password != confirm {
        eprintln!("Passwords do not match");
        std::process::exit(1);
    }

    let mut vfs = SlackVfs::mount(host_dir, &old_password)?;
    vfs.change_password(&old_password, &new_password)?;

    println!("Password changed successfully");

    Ok(())
}
