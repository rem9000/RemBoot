//! Disk enumeration, per platform.

pub struct Disk {
    /// Stable id used by `create --disk` (e.g. /dev/sdb, a Windows disk
    /// number, or a macOS disk identifier).
    pub id: String,
    pub model: String,
    pub size: u64,
    pub removable: bool,
    pub system: bool,
}

pub fn find(id: &str) -> Result<Option<Disk>, String> {
    let want = normalize(id);
    Ok(list()?.into_iter().find(|d| normalize(&d.id) == want))
}

fn normalize(id: &str) -> String {
    id.trim().trim_start_matches("/dev/").to_string()
}

// ---------------------------------------------------------------- Linux --

#[cfg(target_os = "linux")]
pub fn list() -> Result<Vec<Disk>, String> {
    use crate::util::output;
    let sys = root_disks();
    let out = output("lsblk", &["-dnb", "-o", "NAME,SIZE,RM,TYPE,MODEL"])?;
    let mut disks = Vec::new();
    for line in out.lines() {
        let mut it = line.split_whitespace();
        let (Some(name), Some(size), Some(rm), Some(ty)) =
            (it.next(), it.next(), it.next(), it.next())
        else {
            continue;
        };
        // Loopback devices are hidden unless a test opts in.
        let allow_loop = std::env::var_os("REMBOOT_USB_ALLOW_LOOP").is_some();
        if ty != "disk" && !(ty == "loop" && allow_loop) {
            continue;
        }
        let model = it.collect::<Vec<_>>().join(" ");
        disks.push(Disk {
            id: format!("/dev/{name}"),
            model: if model.is_empty() { "(unknown)".into() } else { model },
            size: size.parse().unwrap_or(0),
            removable: rm == "1",
            system: sys.iter().any(|s| s == name),
        });
    }
    Ok(disks)
}

/// Kernel names of disks backing "/" (best-effort), so we never offer them.
#[cfg(target_os = "linux")]
fn root_disks() -> Vec<String> {
    use crate::util::output;
    let Ok(src) = output("findmnt", &["-n", "-o", "SOURCE", "/"]) else {
        return Vec::new();
    };
    let src = src.trim();
    let mut out = Vec::new();
    // Whole-disk parent of the root source.
    if let Ok(pk) = output("lsblk", &["-no", "PKNAME", src]) {
        let pk = pk.trim();
        if !pk.is_empty() {
            out.push(pk.to_string());
        }
    }
    out
}

// -------------------------------------------------------------- Windows --

#[cfg(target_os = "windows")]
pub fn list() -> Result<Vec<Disk>, String> {
    use crate::util::output;
    // CSV: Number,FriendlyName,Size,BusType,IsSystem,IsBoot
    let ps = "Get-Disk | ForEach-Object { \
        '{0}|{1}|{2}|{3}|{4}|{5}' -f $_.Number,$_.FriendlyName,$_.Size,$_.BusType,$_.IsSystem,$_.IsBoot }";
    let out = output("powershell", &["-NoProfile", "-NonInteractive", "-Command", ps])?;
    let mut disks = Vec::new();
    for line in out.lines() {
        let f: Vec<&str> = line.trim().split('|').collect();
        if f.len() < 6 {
            continue;
        }
        let system = f[4].eq_ignore_ascii_case("true") || f[5].eq_ignore_ascii_case("true");
        disks.push(Disk {
            id: f[0].to_string(),
            model: f[1].to_string(),
            size: f[2].parse().unwrap_or(0),
            removable: f[3].eq_ignore_ascii_case("usb"),
            system,
        });
    }
    Ok(disks)
}

// ---------------------------------------------------------------- macOS --

#[cfg(target_os = "macos")]
pub fn list() -> Result<Vec<Disk>, String> {
    use crate::util::output;
    let mut disks = Vec::new();
    // External physical disks only — never internal.
    let listing = output("diskutil", &["list", "external", "physical"])?;
    for line in listing.lines() {
        // Header lines look like: "/dev/disk4 (external, physical):"
        let Some(dev) = line.strip_prefix("/dev/") else {
            continue;
        };
        let Some(id) = dev.split_whitespace().next() else {
            continue;
        };
        let info = output("diskutil", &["info", id]).unwrap_or_default();
        let get = |k: &str| {
            info.lines()
                .find_map(|l| l.trim().strip_prefix(k).map(|v| v.trim_start_matches(':').trim().to_string()))
                .unwrap_or_default()
        };
        let size = get("Disk Size");
        let bytes = size
            .split('(')
            .nth(1)
            .and_then(|s| s.split(' ').next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        disks.push(Disk {
            id: id.to_string(),
            model: get("Device / Media Name"),
            size: bytes,
            removable: get("Removable Media").contains("Removable")
                || get("Internal").eq_ignore_ascii_case("No"),
            system: false,
        });
    }
    Ok(disks)
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub fn list() -> Result<Vec<Disk>, String> {
    Err("unsupported platform".into())
}
