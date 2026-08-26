//! Small process helpers shared by the platform back-ends.

use std::ffi::OsStr;
use std::process::Command;

/// Run a command, returning an error (with stderr) on non-zero exit.
pub fn run<S: AsRef<OsStr>>(cmd: &str, args: &[S]) -> Result<(), String> {
    let status = Command::new(cmd)
        .args(args)
        .status()
        .map_err(|e| format!("failed to run {cmd}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{cmd} exited with {status}"))
    }
}

/// Run a command and capture stdout as a string.
pub fn output<S: AsRef<OsStr>>(cmd: &str, args: &[S]) -> Result<String, String> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run {cmd}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "{cmd} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Like [`run`], but returns Ok even on failure (best-effort steps).
pub fn try_run<S: AsRef<OsStr>>(cmd: &str, args: &[S]) {
    let _ = Command::new(cmd).args(args).status();
}

/// Human-readable byte size.
pub fn human(bytes: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", U[i])
}
