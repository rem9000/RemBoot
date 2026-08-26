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

fn copy_app(esp_root: &Path, efi: &Path) -> Result<(), String> {
    let boot = esp_root.join("EFI").join("BOOT");
    fs::create_dir_all(&boot).map_err(|e| format!("mkdir EFI/BOOT: {e}"))?;
    fs::copy(efi, boot.join("BOOTX64.EFI")).map_err(|e| format!("copy app: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------- Linux --

#[cfg(target_os = "linux")]
pub fn create(target: &Disk, args: &CreateArgs) -> Result<(), String> {
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
    Ok(())
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
pub fn create(target: &Disk, args: &CreateArgs) -> Result<(), String> {
    // Reuse the proven PowerShell Storage-cmdlet flow (see tools/make-usb.ps1).
    let efi = efi_file(&args.efi)?.canonicalize().map_err(|e| e.to_string())?;
    let esp_type = "{c12a7328-f81f-11d2-ba4b-00a0c93ec93b}";
    let mut ps = String::new();
    ps.push_str(&format!(
        "$ErrorActionPreference='Stop';\
         Clear-Disk -Number {n} -RemoveData -RemoveOEM -Confirm:$false -ErrorAction SilentlyContinue;\
         Initialize-Disk -Number {n} -PartitionStyle GPT -ErrorAction SilentlyContinue | Out-Null;\
         $esp=New-Partition -DiskNumber {n} -Size {esp}MB -GptType '{ty}';\
         Format-Volume -Partition $esp -FileSystem FAT32 -NewFileSystemLabel REMBOOT -Confirm:$false|Out-Null;\
         $esp|Add-PartitionAccessPath -AssignDriveLetter;\
         $S=(Get-Partition -DiskNumber {n} -PartitionNumber $esp.PartitionNumber).DriveLetter;\
         New-Item -ItemType Directory -Force -Path \"$S`:\\EFI\\BOOT\"|Out-Null;\
         Copy-Item -Force '{efi}' \"$S`:\\EFI\\BOOT\\BOOTX64.EFI\";",
        n = target.id, esp = args.esp_mb, ty = esp_type, efi = efi.display()
    ));
    if !args.simple {
        ps.push_str(
            "$data=New-Partition -DiskNumber {n} -UseMaximumSize -AssignDriveLetter;\
             Format-Volume -Partition $data -FileSystem exFAT -NewFileSystemLabel REMBOOTDATA -Confirm:$false|Out-Null;\
             $D=(Get-Partition -DiskNumber {n} -PartitionNumber $data.PartitionNumber).DriveLetter;",
        );
    }
    // ISO + config copy target
    let target_var = if args.simple { "$S" } else { "$D" };
    if let Some(isos) = &args.isos {
        let dir = isos.canonicalize().map_err(|e| e.to_string())?;
        ps.push_str(&format!(
            "Get-ChildItem -Path '{}' -Filter *.iso -File | ForEach-Object {{ Copy-Item -Force $_.FullName \"{}`:\\$($_.Name)\" }};",
            dir.display(), target_var
        ));
    }
    let ps = ps.replace("{n}", &target.id);
    crate::util::run("powershell", &["-NoProfile", "-NonInteractive", "-Command", &ps])
}

// ---------------------------------------------------------------- macOS --

#[cfg(target_os = "macos")]
pub fn create(target: &Disk, args: &CreateArgs) -> Result<(), String> {
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
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub fn create(_t: &Disk, _a: &CreateArgs) -> Result<(), String> {
    Err("unsupported platform".into())
}
