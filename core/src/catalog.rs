//! Optional `remboot.conf`: map raw ISO filenames to a clean display name,
//! version and menu position. Pure and host-testable.
//!
//! Format (records separated by a blank line, or by the next `ISO:` key;
//! keys are case-insensitive, `#` or `;` starts a comment):
//!
//! ```text
//! ISO: memtest.iso
//! NAME: MemTest
//! VERSION: 1.0.0
//! POSITION: 1
//! ```

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Write as _;

/// One record parsed from the config file.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Meta {
    pub iso: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub position: Option<i32>,
}

/// A resolved menu entry: what to show, and which file actually boots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Filename to boot (verbatim, as found on disk).
    pub iso: String,
    /// Display label.
    pub label: String,
    pub version: Option<String>,
    pub position: Option<i32>,
}

/// Default label for an ISO with no config entry: the filename minus a
/// trailing `.iso`.
pub fn default_label(filename: &str) -> String {
    let lower = filename.to_ascii_lowercase();
    if let Some(stem) = lower.strip_suffix(".iso") {
        filename[..stem.len()].to_string()
    } else {
        filename.to_string()
    }
}

/// Parse the config text into records. Malformed lines are skipped.
pub fn parse(text: &str) -> Vec<Meta> {
    let mut out = Vec::new();
    let mut cur: Option<Meta> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let key = k.trim().to_ascii_uppercase();
        let val = v.trim();
        match key.as_str() {
            "ISO" => {
                if let Some(m) = cur.take() {
                    out.push(m);
                }
                cur = Some(Meta { iso: val.to_string(), ..Default::default() });
            }
            "NAME" => {
                if let Some(m) = cur.as_mut() {
                    m.name = non_empty(val);
                }
            }
            "VERSION" => {
                if let Some(m) = cur.as_mut() {
                    m.version = non_empty(val);
                }
            }
            "POSITION" => {
                if let Some(m) = cur.as_mut() {
                    m.position = val.parse().ok();
                }
            }
            _ => {}
        }
    }
    if let Some(m) = cur.take() {
        out.push(m);
    }
    out
}

fn non_empty(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Merge discovered ISO filenames with parsed metadata and order them:
/// entries with a `POSITION` first (ascending), then the rest alphabetically
/// by label (case-insensitive). Config records whose ISO is not present on
/// disk are ignored.
pub fn build(found: &[String], metas: &[Meta]) -> Vec<Entry> {
    let mut entries: Vec<Entry> = found
        .iter()
        .map(|iso| {
            let meta = metas.iter().find(|m| m.iso.eq_ignore_ascii_case(iso));
            Entry {
                iso: iso.clone(),
                label: meta
                    .and_then(|m| m.name.clone())
                    .unwrap_or_else(|| default_label(iso)),
                version: meta.and_then(|m| m.version.clone()),
                position: meta.and_then(|m| m.position),
            }
        })
        .collect();

    sort(&mut entries);
    entries
}

/// Order entries in place: positioned first (ascending), then the rest
/// alphabetically by label (case-insensitive).
pub fn sort(entries: &mut [Entry]) {
    entries.sort_by(|a, b| {
        let pa = a.position.unwrap_or(i32::MAX);
        let pb = b.position.unwrap_or(i32::MAX);
        pa.cmp(&pb)
            .then_with(|| a.label.to_ascii_lowercase().cmp(&b.label.to_ascii_lowercase()))
            .then_with(|| a.iso.cmp(&b.iso))
    });
}

/// Serialize entries back to config text. The current order is frozen with an
/// explicit `POSITION` per entry; `NAME` is written only when it differs from
/// the filename default, to keep the file tidy.
pub fn serialize(entries: &[Entry]) -> String {
    let mut s = String::new();
    for (i, e) in entries.iter().enumerate() {
        let _ = writeln!(s, "ISO: {}", e.iso);
        if e.label != default_label(&e.iso) {
            let _ = writeln!(s, "NAME: {}", e.label);
        }
        if let Some(v) = &e.version {
            let _ = writeln!(s, "VERSION: {v}");
        }
        let _ = writeln!(s, "POSITION: {}", i + 1);
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn parses_a_record() {
        let cfg = "ISO: memtest.iso\nNAME: MemTest\nVERSION: 1.0.0\nPOSITION: 1\n";
        let metas = parse(cfg);
        assert_eq!(
            metas,
            vec![Meta {
                iso: "memtest.iso".into(),
                name: Some("MemTest".into()),
                version: Some("1.0.0".into()),
                position: Some(1),
            }]
        );
    }

    #[test]
    fn parses_multiple_records_and_comments() {
        let cfg = "\
# my images
ISO: a.iso
NAME: Alpha

; second
ISO: b.iso
NAME: Bravo
POSITION: 2
ISO: c.iso
POSITION: 1
";
        let metas = parse(cfg);
        assert_eq!(metas.len(), 3);
        assert_eq!(metas[0].name.as_deref(), Some("Alpha"));
        assert_eq!(metas[1].position, Some(2));
        assert_eq!(metas[2].iso, "c.iso");
    }

    #[test]
    fn keys_are_case_insensitive_and_trimmed() {
        let cfg = "iso:  x.iso \n  Name:  Hello World \nversion:2";
        let m = parse(cfg);
        assert_eq!(m[0].iso, "x.iso");
        assert_eq!(m[0].name.as_deref(), Some("Hello World"));
        assert_eq!(m[0].version.as_deref(), Some("2"));
    }

    #[test]
    fn default_label_strips_iso() {
        assert_eq!(default_label("memtest.iso"), "memtest");
        assert_eq!(default_label("Win11_25H2.ISO"), "Win11_25H2");
        assert_eq!(default_label("noext"), "noext");
    }

    #[test]
    fn build_orders_by_position_then_alpha() {
        let found = vec![
            "zulu.iso".to_string(),
            "memtest.iso".to_string(),
            "alpha.iso".to_string(),
            "bravo.iso".to_string(),
        ];
        let metas = parse("ISO: memtest.iso\nNAME: MemTest\nPOSITION: 1\nISO: zulu.iso\nPOSITION: 2\n");
        let entries = build(&found, &metas);
        let labels: Vec<&str> = entries.iter().map(|e| e.label.as_str()).collect();
        // positioned first (MemTest=1, zulu=2), then alpha, bravo by label
        assert_eq!(labels, vec!["MemTest", "zulu", "alpha", "bravo"]);
        assert_eq!(entries[0].iso, "memtest.iso");
    }

    #[test]
    fn serialize_round_trips() {
        let found = vec!["b.iso".to_string(), "a.iso".to_string()];
        let metas = parse("ISO: a.iso\nNAME: Alpha\nVERSION: 2.0\nPOSITION: 1\nISO: b.iso\nPOSITION: 2\n");
        let entries = build(&found, &metas);
        // b.iso has no NAME (label == default "b"), so it should be omitted.
        let text = serialize(&entries);
        assert!(text.contains("ISO: a.iso"));
        assert!(text.contains("NAME: Alpha"));
        assert!(text.contains("VERSION: 2.0"));
        assert!(!text.contains("NAME: b"));
        // Re-parsing and rebuilding yields the same entries.
        let rebuilt = build(&found, &parse(&text));
        assert_eq!(entries, rebuilt);
    }

    #[test]
    fn config_entries_without_a_matching_file_are_ignored() {
        let found = vec!["present.iso".to_string()];
        let metas = parse("ISO: missing.iso\nNAME: Ghost\nISO: present.iso\nNAME: Here\n");
        let entries = build(&found, &metas);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].label, "Here");
    }
}
