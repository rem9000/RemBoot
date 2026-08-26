//! `remboot-usb` — provision a USB stick as a RemBoot drive on Windows, Linux
//! or macOS: partition (FAT32 ESP + exFAT data, or a single FAT32), format,
//! and copy the app + ISOs. This is the cross-platform core that the (future)
//! GUI wraps.
//!
//! DESTRUCTIVE: `create` erases the whole target disk.

mod disk;
mod provision;
mod util;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "remboot-usb", about = "Provision a USB stick for RemBoot")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List candidate USB disks.
    List,
    /// Partition + format a disk and install RemBoot onto it. ERASES the disk.
    Create(CreateArgs),
}

#[derive(clap::Args)]
pub struct CreateArgs {
    /// Target disk id (see `list`): a device like /dev/sdb, a Windows disk
    /// number, or a macOS disk identifier.
    #[arg(long)]
    disk: String,

    /// Path to BOOTX64.EFI (default: ./dist/EFI/BOOT/BOOTX64.EFI).
    #[arg(long, default_value = "dist/EFI/BOOT/BOOTX64.EFI")]
    efi: PathBuf,

    /// Folder of *.iso files to copy onto the data partition.
    #[arg(long)]
    isos: Option<PathBuf>,

    /// remboot.conf to seed (default: ./remboot.conf.example if present).
    #[arg(long)]
    config: Option<PathBuf>,

    /// Single FAT32 partition (no exFAT). Only for ISOs under 4 GB.
    #[arg(long)]
    simple: bool,

    /// Size of the FAT32 boot partition, MiB.
    #[arg(long, default_value_t = 512)]
    esp_mb: u64,

    /// Proceed without the interactive confirmation.
    #[arg(long)]
    yes: bool,

    /// Operate on a non-removable disk (dangerous; off by default).
    #[arg(long)]
    allow_internal: bool,
}

fn main() -> ExitCode {
    match Cli::parse().cmd {
        Cmd::List => match disk::list() {
            Ok(disks) => {
                if disks.is_empty() {
                    println!("No disks found.");
                }
                println!("{:<16} {:>9}  {:<9} {}", "ID", "SIZE", "TYPE", "MODEL");
                for d in disks {
                    println!(
                        "{:<16} {:>9}  {:<9} {}",
                        d.id,
                        human(d.size),
                        if d.removable { "removable" } else { "internal" },
                        d.model
                    );
                }
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e),
        },
        Cmd::Create(args) => match run_create(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => fail(&e),
        },
    }
}

fn run_create(args: CreateArgs) -> Result<(), String> {
    if !args.efi.is_file() {
        return Err(format!(
            "BOOTX64.EFI not found at {}. Build it first (tools/build.sh) or pass --efi.",
            args.efi.display()
        ));
    }
    let target = disk::find(&args.disk)?
        .ok_or_else(|| format!("disk '{}' not found (see `remboot-usb list`)", args.disk))?;

    if target.system {
        return Err(format!("{} looks like a system disk — refusing.", target.id));
    }
    if !target.removable && !args.allow_internal {
        return Err(format!(
            "{} is not removable. Re-run with --allow-internal only if you are sure.",
            target.id
        ));
    }

    eprintln!();
    eprintln!("About to ERASE and repartition:");
    eprintln!("  {}  {}  ({})", target.id, target.model, human(target.size));
    if args.simple {
        eprintln!("  -> one FAT32 partition (app + ISOs)");
    } else {
        eprintln!("  -> FAT32 {} MiB (app) + exFAT (ISOs)", args.esp_mb);
    }
    eprintln!();

    if !args.yes && !confirm(&target.id)? {
        eprintln!("Aborted.");
        return Ok(());
    }

    provision::create(&target, &args)?;
    eprintln!("\nDone. RemBoot USB is ready. Boot the target PC from it (UEFI, Secure Boot off).");
    Ok(())
}

fn confirm(id: &str) -> Result<bool, String> {
    use std::io::Write;
    eprint!("Type  ERASE {id}  to continue: ");
    std::io::stderr().flush().ok();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).map_err(|e| e.to_string())?;
    Ok(line.trim() == format!("ERASE {id}"))
}

fn human(bytes: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", U[i])
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("error: {msg}");
    ExitCode::FAILURE
}
