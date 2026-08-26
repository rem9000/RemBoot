//! Partition + format + copy, per platform.

use crate::disk::Disk;
use crate::embed;
use crate::CreateArgs;
use std::fs;
use std::path::{Path, PathBuf};

/// True if we have a BOOTX64.EFI to install (an explicit file or the embedded
/// copy).
pub fn efi_available(explicit: &Path) -> bool {
    explicit.is_file() || embed::EFI.is_some()
}

/// Are we Administrator (Windows) / root (Unix)? Writing to a raw disk needs it.
pub fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        crate::util::output(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "[bool]([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)",
            ],
        )
        .map(|s| s.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        crate::util::output("id", &["-u"]).map(|s| s.trim() == "0").unwrap_or(false)
    }
}

#[cfg(windows)]
pub const ELEVATION_HINT: &str = "right-click the app and choose \u{201c}Run as administrator\u{201d}";
#[cfg(not(windows))]
pub const ELEVATION_HINT: &str = "run it with sudo";

/// Return a real path to the app to install: the explicit file, or the
/// embedded copy written to a temp file.
fn efi_file(explicit: &Path) -> Result<PathBuf, String> {
    if explicit.is_file() {
        return Ok(explicit.to_owned());
    }
    if let Some(bytes) = embed::EFI {
        let p = std::env::temp_dir().join("remboot-BOOTX64.EFI");
        fs::write(&p, bytes).map_err(|e| format!("stage embedded app: {e}"))?;
        return Ok(p);
    }
    Err(format!("BOOTX64.EFI not found at {}", explicit.display()))
}

/// Copy the ISO folder (if any) and a remboot.conf seed into `dst`.
#[allow(dead_code)] // used on Linux/macOS; Windows copies via PowerShell
fn copy_payload(dst: &Path, args: &CreateArgs) -> Result<(), String> {
    if let Some(isos) = &args.isos {
        let entries = fs::read_dir(isos).map_err(|e| format!("read {}: {e}", isos.display()))?;
        for e in entries.flatten() {
            let p = e.path();
            let is_iso = p.extension().is_some_and(|x| x.eq_ignore_ascii_case("iso"));
            if is_iso {
                let name = p.file_name().unwrap();
                eprintln!("  copying {}", name.to_string_lossy());
                fs::copy(&p, dst.join(name)).map_err(|e| format!("copy {}: {e}", p.display()))?;
            }
        }
    }
    let cfg = args.config.clone().or_else(|| {
        let d = Path::new("remboot.conf.example");
        d.is_file().then(|| d.to_path_buf())
    });
    if let Some(cfg) = cfg {
        if cfg.is_file() {
            fs::copy(&cfg, dst.join("remboot.conf")).map_err(|e| format!("copy config: {e}"))?;
        }
    }
    Ok(())
}

#[allow(dead_code)] // used on Linux/macOS; Windows copies via PowerShell
fn copy_app(esp_root: &Path, efi: &Path) -> Result<(), String> {
    let boot = esp_root.join("EFI").join("BOOT");
    fs::create_dir_all(&boot).map_err(|e| format!("mkdir EFI/BOOT: {e}"))?;
    fs::copy(efi, boot.join("BOOTX64.EFI")).map_err(|e| format!("copy app: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------- Linux --

#[cfg(target_os = "linux")]
pub fn create(target: &Disk, args: &CreateArgs) -> Result<String, String> {
    use crate::util::{run, try_run};
    let dev = target.id.as_str();
    let efi = efi_file(&args.efi)?;

    eprintln!("Partitioning {dev} ...");
    try_run("wipefs", &["-a", dev]);
    run("sgdisk", &["--zap-all", dev])?;
    if args.simple {
        run("sgdisk", &["-n", "1:0:0", "-t", "1:ef00", "-c", "1:REMBOOT", dev])?;
    } else {
        run(
            "sgdisk",
            &[
                "-n", &format!("1:0:+{}M", args.esp_mb), "-t", "1:ef00", "-c", "1:REMBOOT",
                "-n", "2:0:0", "-t", "2:0700", "-c", "2:REMBOOT_DATA", dev,
            ],
        )?;
    }
    try_run("partprobe", &[dev]);
    try_run("udevadm", &["settle"]);
    // give the kernel a moment to create the partition nodes
    std::thread::sleep(std::time::Duration::from_millis(700));

    let p1 = part(dev, 1);
    eprintln!("Formatting {p1} (FAT32) ...");
    run("mkfs.vfat", &["-F", "32", "-n", "REMBOOT", &p1])?;

    let m1 = tempdir("esp")?;
    mount(&p1, &m1)?;
    let r: Result<(), String> = (|| {
        copy_app(&m1, &efi)?;
        if args.simple {
            copy_payload(&m1, args)?;
        }
        Ok(())
    })();
    try_run("sync", &[] as &[&str]);
    unmount(&m1);
    r?;

    if !args.simple {
        let p2 = part(dev, 2);
        eprintln!("Formatting {p2} (exFAT) ...");
        run("mkfs.exfat", &["-L", "REMBOOTDATA", &p2])?;
        let m2 = tempdir("data")?;
        mount_exfat(&p2, &m2)?;
        let r = copy_payload(&m2, args);
        try_run("sync", &[] as &[&str]);
        unmount(&m2);
        r?;
    }
    Ok("The USB is ready.".into())
}

#[cfg(target_os = "linux")]
fn part(dev: &str, n: u32) -> String {
    match dev.chars().last() {
        Some(c) if c.is_ascii_digit() => format!("{dev}p{n}"),
        _ => format!("{dev}{n}"),
    }
}

#[cfg(target_os = "linux")]
fn tempdir(tag: &str) -> Result<std::path::PathBuf, String> {
    let p = std::env::temp_dir().join(format!("remboot-usb-{}-{tag}", std::process::id()));
    fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    Ok(p)
}

#[cfg(target_os = "linux")]
fn mount(dev: &str, mnt: &Path) -> Result<(), String> {
    crate::util::run("mount", &[dev, &mnt.to_string_lossy()])
}

#[cfg(target_os = "linux")]
fn mount_exfat(dev: &str, mnt: &Path) -> Result<(), String> {
    let m = mnt.to_string_lossy().into_owned();
    // Kernel exFAT if present, else the FUSE driver.
    crate::util::run("mount", &["-t", "exfat", dev, &m])
        .or_else(|_| crate::util::run("mount.exfat-fuse", &[dev, &m]))
}

#[cfg(target_os = "linux")]
fn unmount(mnt: &Path) {
    crate::util::try_run("umount", &[&mnt.to_string_lossy().into_owned()]);
}

// -------------------------------------------------------------- Windows --

#[cfg(target_os = "windows")]
pub fn create(target: &Disk, args: &CreateArgs) -> Result<String, String> {
    let efi = efi_file(&args.efi)?.canonicalize().map_err(|e| e.to_string())?;
    let esp_guid = "{c12a7328-f81f-11d2-ba4b-00a0c93ec93b}";
    let dst = if args.simple { "$S" } else { "$D" };

    let mut s = String::new();
    s.push_str("$ErrorActionPreference='Stop'\n");
    s.push_str("$n=@DISK@\n");
    s.push_str("Clear-Disk -Number $n -RemoveData -RemoveOEM -Confirm:$false -ErrorAction SilentlyContinue\n");
    // Convert to GPT reliably: initialise a RAW disk, or convert an MBR one.
    s.push_str("$d=Get-Disk -Number $n\n");
    // In PowerShell a newline before `elseif` is a syntax error — keep on one line.
    s.push_str("if ($d.PartitionStyle -eq 'RAW') { Initialize-Disk -Number $n -PartitionStyle GPT | Out-Null } elseif ($d.PartitionStyle -ne 'GPT') { Set-Disk -Number $n -PartitionStyle GPT }\n");
    s.push_str(&format!("$esp=New-Partition -DiskNumber $n -Size @ESP@MB -GptType '{esp_guid}'\n"));
    s.push_str("Format-Volume -Partition $esp -FileSystem FAT32 -NewFileSystemLabel REMBOOT -Confirm:$false | Out-Null\n");
    s.push_str("$esp | Add-PartitionAccessPath -AssignDriveLetter | Out-Null\n");
    s.push_str("$S=(Get-Partition -DiskNumber $n -PartitionNumber $esp.PartitionNumber).DriveLetter\n");
    s.push_str("New-Item -ItemType Directory -Force -Path \"$S`:\\EFI\\BOOT\" | Out-Null\n");
    s.push_str("Copy-Item -Force '@EFI@' \"$S`:\\EFI\\BOOT\\BOOTX64.EFI\"\n");
    s.push_str("Write-Output \"Installed the app on $S`:\"\n");
    if !args.simple {
        // Assign the data-partition letter after formatting (like the ESP).
        s.push_str("$data=New-Partition -DiskNumber $n -UseMaximumSize\n");
        s.push_str("Format-Volume -Partition $data -FileSystem exFAT -NewFileSystemLabel REMBOOTDATA -Confirm:$false | Out-Null\n");
        s.push_str("$data | Add-PartitionAccessPath -AssignDriveLetter | Out-Null\n");
        s.push_str("$D=(Get-Partition -DiskNumber $n -PartitionNumber $data.PartitionNumber).DriveLetter\n");
    }
    if args.isos.is_some() {
        s.push_str("$src='@ISOS@'\n");
        s.push_str("$isos=@(Get-ChildItem -LiteralPath $src -Filter *.iso -File)\n");
        s.push_str(&format!("Write-Output \"Found $($isos.Count) ISO(s) in $src - copying to {dst}`:\"\n"));
        s.push_str(&format!("$isos | ForEach-Object {{ Copy-Item -Force -LiteralPath $_.FullName -Destination \"{dst}`:\\$($_.Name)\"; Write-Output (\"  copied \" + $_.Name) }}\n"));
    }
    let has_config = args.config.as_ref().is_some_and(|c| c.is_file());
    if has_config {
        s.push_str(&format!("Copy-Item -Force -LiteralPath '@CONFIG@' -Destination \"{dst}`:\\remboot.conf\"\n"));
    }
    s.push_str("Write-Output 'The USB is ready.'\n");

    // Fill placeholders. std::fs::canonicalize yields a \\?\ verbatim path,
    // under which Get-ChildItem silently matches nothing — strip it. Then escape
    // ' as '' for the PowerShell single-quoted literal.
    let lit = |p: &Path| {
        let s = p.to_string_lossy();
        let clean = s.strip_prefix(r"\\?\").unwrap_or(&s);
        clean.replace('\'', "''")
    };
    let mut script = s
        .replace("@DISK@", &target.id)
        .replace("@ESP@", &args.esp_mb.to_string())
        .replace("@EFI@", &lit(&efi));
    if let Some(isos) = &args.isos {
        let dir = isos.canonicalize().map_err(|e| format!("ISO folder: {e}"))?;
        script = script.replace("@ISOS@", &lit(&dir));
    }
    if has_config {
        let cfg = args.config.as_ref().unwrap().canonicalize().map_err(|e| e.to_string())?;
        script = script.replace("@CONFIG@", &lit(&cfg));
    }

    // Run as a script file (avoids -Command quoting headaches); capture output
    // so the GUI shows exactly what happened.
    let path = std::env::temp_dir().join("remboot-usb-provision.ps1");
    fs::write(&path, &script).map_err(|e| format!("write script: {e}"))?;
    let ps = path.to_string_lossy().into_owned();
    crate::util::output(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", &ps],
    )
    .map(|out| out.trim().to_string())
}

// ---------------------------------------------------------------- macOS --

#[cfg(target_os = "macos")]
pub fn create(target: &Disk, args: &CreateArgs) -> Result<String, String> {
    use crate::util::run;
    let id = target.id.as_str();
    if args.simple {
        run("diskutil", &["partitionDisk", id, "GPT", "FAT32", "REMBOOT", "R"])?;
    } else {
        run(
            "diskutil",
            &["partitionDisk", id, "2", "GPT",
              "FAT32", "REMBOOT", &format!("{}M", args.esp_mb),
              "ExFAT", "REMBOOTDATA", "R"],
        )?;
    }
    let efi = efi_file(&args.efi)?;
    copy_app(Path::new("/Volumes/REMBOOT"), &efi)?;
    let data = if args.simple { "/Volumes/REMBOOT" } else { "/Volumes/REMBOOTDATA" };
    copy_payload(Path::new(data), args)?;
    Ok("The USB is ready.".into())
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub fn create(_t: &Disk, _a: &CreateArgs) -> Result<String, String> {
    Err("unsupported platform".into())
}
