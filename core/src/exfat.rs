//! Minimal read-only exFAT reader: list `*.iso` files in the root directory
//! and resolve a file to its on-disk byte extents.
//!
//! UEFI firmware only mounts FAT; Ventoy's data partition (where the ISOs
//! live) is exFAT. This module walks the on-disk structures directly through
//! a [`ByteRead`] so it works on any UEFI `BlockIO` handle — and on plain
//! files in host tests.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// Byte-addressed read access to a volume (offsets relative to volume start).
pub trait ByteRead {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ()>;
}

const ENTRY_SIZE: usize = 32;
/// Safety cap on directory / FAT chain walks.
const MAX_CLUSTERS: usize = 1 << 20;
/// Largest cluster size we buffer (spec max is 32 MiB).
const MAX_CLUSTER_BYTES: usize = 1 << 25;

const ATTR_DIRECTORY: u16 = 0x10;
const FLAG_NO_FAT_CHAIN: u8 = 0x02;
const FAT_CHAIN_END: u32 = 0xFFFF_FFF7;

/// A file found in the root directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    pub first_cluster: u32,
    pub size: u64,
    /// exFAT "NoFatChain": the file occupies contiguous clusters.
    pub contiguous: bool,
}

/// A contiguous run on disk, in bytes relative to the volume start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extent {
    pub disk_offset: u64,
    pub len: u64,
}

pub struct Volume {
    bytes_per_sector: u64,
    sectors_per_cluster: u64,
    fat_offset: u64,          // in sectors
    cluster_heap_offset: u64, // in sectors
    cluster_count: u32,
    root_cluster: u32,
}

fn le32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

fn le64(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes([
        b[off], b[off + 1], b[off + 2], b[off + 3], b[off + 4], b[off + 5], b[off + 6], b[off + 7],
    ])
}

impl Volume {
    pub fn open(dev: &mut impl ByteRead) -> Option<Volume> {
        let mut bs = [0u8; 512];
        dev.read_at(0, &mut bs).ok()?;
        if &bs[3..11] != b"EXFAT   " || bs[510] != 0x55 || bs[511] != 0xAA {
            return None;
        }
        let bps_shift = bs[108];
        let spc_shift = bs[109];
        if !(9..=12).contains(&bps_shift) || spc_shift > 25 {
            return None;
        }
        Some(Volume {
            bytes_per_sector: 1u64 << bps_shift,
            sectors_per_cluster: 1u64 << spc_shift,
            fat_offset: le32(&bs, 80) as u64,
            cluster_heap_offset: le32(&bs, 88) as u64,
            cluster_count: le32(&bs, 92),
            root_cluster: le32(&bs, 96),
        })
    }

    pub fn cluster_bytes(&self) -> u64 {
        self.bytes_per_sector * self.sectors_per_cluster
    }

    fn cluster_disk_offset(&self, cluster: u32) -> u64 {
        (self.cluster_heap_offset + (cluster as u64 - 2) * self.sectors_per_cluster)
            * self.bytes_per_sector
    }

    fn valid_cluster(&self, c: u32) -> bool {
        c >= 2 && c - 2 < self.cluster_count
    }

    fn next_cluster(&self, dev: &mut impl ByteRead, cluster: u32) -> Option<u32> {
        let mut fe = [0u8; 4];
        dev.read_at(self.fat_offset * self.bytes_per_sector + cluster as u64 * 4, &mut fe)
            .ok()?;
        Some(u32::from_le_bytes(fe))
    }

    /// List files (not directories) in the root directory.
    pub fn list_root(&self, dev: &mut impl ByteRead) -> Option<Vec<FileEntry>> {
        let cluster_bytes = self.cluster_bytes() as usize;
        if cluster_bytes == 0 || cluster_bytes > MAX_CLUSTER_BYTES {
            return None;
        }
        let mut out = Vec::new();
        let mut buf = vec![0u8; cluster_bytes];
        let mut cluster = self.root_cluster;

        // In-progress entry set being reassembled across 0x85/0xC0/0xC1.
        let mut attrs = 0u16;
        let mut flags = 0u8;
        let mut first_cluster = 0u32;
        let mut size = 0u64;
        let mut name_len = 0u8;
        let mut name_units: Vec<u16> = Vec::new();
        let mut active = false;

        'chain: for _ in 0..MAX_CLUSTERS {
            if !self.valid_cluster(cluster) {
                break;
            }
            dev.read_at(self.cluster_disk_offset(cluster), &mut buf).ok()?;

            for e in buf.chunks_exact(ENTRY_SIZE) {
                match e[0] {
                    0x00 => break 'chain,
                    0x85 => {
                        attrs = u16::from_le_bytes([e[4], e[5]]);
                        flags = 0;
                        first_cluster = 0;
                        size = 0;
                        name_len = 0;
                        name_units.clear();
                        active = true;
                    }
                    0xC0 if active => {
                        flags = e[1];
                        name_len = e[3];
                        first_cluster = le32(e, 20);
                        size = le64(e, 24);
                    }
                    0xC1 if active => {
                        for i in 0..15 {
                            name_units.push(u16::from_le_bytes([e[2 + 2 * i], e[3 + 2 * i]]));
                        }
                    }
                    _ => active = false,
                }

                if active && name_len > 0 && name_units.len() >= name_len as usize {
                    if attrs & ATTR_DIRECTORY == 0 {
                        let name: String = char::decode_utf16(
                            name_units[..name_len as usize].iter().copied(),
                        )
                        .map(|c| c.unwrap_or('\u{FFFD}'))
                        .collect();
                        out.push(FileEntry {
                            name,
                            first_cluster,
                            size,
                            contiguous: flags & FLAG_NO_FAT_CHAIN != 0,
                        });
                    }
                    active = false;
                }
            }

            cluster = self.next_cluster(dev, cluster)?;
            if cluster >= FAT_CHAIN_END {
                break;
            }
        }
        Some(out)
    }

    /// Read a whole (small) file's contents. Intended for config files, not
    /// multi-gigabyte ISOs — callers stream those via [`Self::extents`].
    pub fn read_file(&self, dev: &mut impl ByteRead, entry: &FileEntry) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(entry.size as usize);
        let mut remaining = entry.size;
        for ext in self.extents(dev, entry)? {
            if remaining == 0 {
                break;
            }
            let take = ext.len.min(remaining) as usize;
            let mut buf = vec![0u8; take];
            dev.read_at(ext.disk_offset, &mut buf).ok()?;
            out.extend_from_slice(&buf);
            remaining -= take as u64;
        }
        Some(out)
    }

    /// Find a root-directory file by name (case-insensitive).
    pub fn find(&self, dev: &mut impl ByteRead, name: &str) -> Option<FileEntry> {
        self.list_root(dev)?
            .into_iter()
            .find(|f| f.name.eq_ignore_ascii_case(name))
    }

    /// Resolve a file to its coalesced on-disk byte extents. The final extent
    /// may run past `entry.size` (cluster rounding); callers clamp to size.
    pub fn extents(&self, dev: &mut impl ByteRead, entry: &FileEntry) -> Option<Vec<Extent>> {
        let cb = self.cluster_bytes();
        if entry.size == 0 {
            return Some(Vec::new());
        }
        let n_clusters = entry.size.div_ceil(cb);
        if n_clusters as usize > MAX_CLUSTERS {
            return None;
        }
        let mut runs: Vec<Extent> = Vec::new();
        let push = |c: u32, runs: &mut Vec<Extent>| {
            let off = self.cluster_disk_offset(c);
            match runs.last_mut() {
                Some(r) if r.disk_offset + r.len == off => r.len += cb,
                _ => runs.push(Extent { disk_offset: off, len: cb }),
            }
        };

        if entry.contiguous {
            for i in 0..n_clusters as u32 {
                let c = entry.first_cluster + i;
                if !self.valid_cluster(c) {
                    return None;
                }
                push(c, &mut runs);
            }
        } else {
            let mut c = entry.first_cluster;
            for _ in 0..n_clusters {
                if !self.valid_cluster(c) {
                    break;
                }
                push(c, &mut runs);
                c = self.next_cluster(dev, c)?;
                if c >= FAT_CHAIN_END {
                    break;
                }
            }
        }
        Some(runs)
    }
}

/// Names of `*.iso` files in the volume's root directory (case-insensitive).
/// Returns `None` if this is not an exFAT volume.
pub fn list_isos(dev: &mut impl ByteRead) -> Option<Vec<String>> {
    let v = Volume::open(dev)?;
    let files = v.list_root(dev)?;
    Some(
        files
            .into_iter()
            .map(|f| f.name)
            .filter(|n| n.to_ascii_lowercase().ends_with(".iso"))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    impl ByteRead for Vec<u8> {
        fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ()> {
            let off = offset as usize;
            if off + buf.len() > self.len() {
                return Err(());
            }
            buf.copy_from_slice(&self[off..off + buf.len()]);
            Ok(())
        }
    }

    /// Synthetic exFAT volume: 512-byte sectors, 1 sector/cluster, FAT at
    /// sector 2, cluster heap at sector 4, root dir chained 2 -> 3 so the last
    /// (long-named) entry set straddles a cluster boundary.
    fn synthetic_volume() -> Vec<u8> {
        let mut v = vec![0u8; 512 * 16];
        v[3..11].copy_from_slice(b"EXFAT   ");
        v[80..84].copy_from_slice(&2u32.to_le_bytes());
        v[88..92].copy_from_slice(&4u32.to_le_bytes());
        v[92..96].copy_from_slice(&8u32.to_le_bytes());
        v[96..100].copy_from_slice(&2u32.to_le_bytes());
        v[108] = 9;
        v[109] = 0;
        v[510] = 0x55;
        v[511] = 0xAA;
        let fat = 2 * 512;
        v[fat + 8..fat + 12].copy_from_slice(&3u32.to_le_bytes());
        v[fat + 12..fat + 16].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());

        let mut entries: Vec<[u8; 32]> = Vec::new();
        let file_set =
            |name: &str, attrs: u16, first: u32, size: u64, flags: u8, entries: &mut Vec<[u8; 32]>| {
                let units: Vec<u16> = name.encode_utf16().collect();
                let n_name = units.len().div_ceil(15);
                let mut e85 = [0u8; 32];
                e85[0] = 0x85;
                e85[1] = (1 + n_name) as u8;
                e85[4..6].copy_from_slice(&attrs.to_le_bytes());
                entries.push(e85);
                let mut c0 = [0u8; 32];
                c0[0] = 0xC0;
                c0[1] = flags;
                c0[3] = units.len() as u8;
                c0[20..24].copy_from_slice(&first.to_le_bytes());
                c0[24..32].copy_from_slice(&size.to_le_bytes());
                entries.push(c0);
                for chunk in units.chunks(15) {
                    let mut c1 = [0u8; 32];
                    c1[0] = 0xC1;
                    for (i, u) in chunk.iter().enumerate() {
                        c1[2 + 2 * i..4 + 2 * i].copy_from_slice(&u.to_le_bytes());
                    }
                    entries.push(c1);
                }
            };

        let mut label = [0u8; 32];
        label[0] = 0x83;
        entries.push(label);
        file_set("test.iso", 0, 5, 900, FLAG_NO_FAT_CHAIN, &mut entries);
        file_set("subdir.iso", ATTR_DIRECTORY, 9, 0, 0, &mut entries);
        file_set("readme.txt", 0, 10, 10, FLAG_NO_FAT_CHAIN, &mut entries);
        file_set("very-long-name-x64.iso", 0, 11, 300, FLAG_NO_FAT_CHAIN, &mut entries);

        let heap = 4 * 512;
        for (i, e) in entries.iter().enumerate() {
            let base = heap + (i / 16) * 512 + (i % 16) * 32;
            v[base..base + 32].copy_from_slice(e);
        }
        v
    }

    #[test]
    fn lists_iso_files_only() {
        let mut vol = synthetic_volume();
        let isos = list_isos(&mut vol).expect("valid exFAT");
        assert_eq!(isos, alloc::vec!["test.iso", "very-long-name-x64.iso"]);
    }

    #[test]
    fn resolves_contiguous_extents() {
        let mut vol = synthetic_volume();
        let v = Volume::open(&mut vol).unwrap();
        let files = v.list_root(&mut vol).unwrap();
        let f = files.iter().find(|f| f.name == "test.iso").unwrap();
        assert!(f.contiguous);
        // 900 bytes over 512-byte clusters starting at cluster 5 -> 2 clusters,
        // contiguous, coalesced into one 1024-byte extent at sector 4+3=7.
        let ext = v.extents(&mut vol, f).unwrap();
        assert_eq!(ext, alloc::vec![Extent { disk_offset: 7 * 512, len: 1024 }]);
    }

    #[test]
    fn resolves_fat_chain_extents() {
        // Fragmented (chained) 3-cluster file 6 -> 7 -> 10: the disk-adjacent
        // 6,7 coalesce into one run; 10 is a separate extent.
        let mut v = vec![0u8; 512 * 24];
        v[3..11].copy_from_slice(b"EXFAT   ");
        v[80..84].copy_from_slice(&2u32.to_le_bytes());
        v[88..92].copy_from_slice(&4u32.to_le_bytes());
        v[92..96].copy_from_slice(&16u32.to_le_bytes());
        v[96..100].copy_from_slice(&2u32.to_le_bytes());
        v[108] = 9;
        v[109] = 0;
        v[510] = 0x55;
        v[511] = 0xAA;
        let fat = 2 * 512;
        v[fat + 6 * 4..fat + 6 * 4 + 4].copy_from_slice(&7u32.to_le_bytes());
        v[fat + 7 * 4..fat + 7 * 4 + 4].copy_from_slice(&10u32.to_le_bytes());
        v[fat + 10 * 4..fat + 10 * 4 + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        let vol = Volume::open(&mut v).unwrap();
        let entry = FileEntry {
            name: "x".into(),
            first_cluster: 6,
            size: 3 * 512,
            contiguous: false,
        };
        let ext = vol.extents(&mut v, &entry).unwrap();
        let off = |c: u32| (4 + (c as u64 - 2)) * 512;
        assert_eq!(
            ext,
            alloc::vec![
                Extent { disk_offset: off(6), len: 1024 },
                Extent { disk_offset: off(10), len: 512 },
            ]
        );
    }

    #[test]
    fn reads_file_contents() {
        // One-cluster volume with a config file whose bytes we can verify.
        let mut v = vec![0u8; 512 * 12];
        v[3..11].copy_from_slice(b"EXFAT   ");
        v[80..84].copy_from_slice(&2u32.to_le_bytes());
        v[88..92].copy_from_slice(&4u32.to_le_bytes());
        v[92..96].copy_from_slice(&8u32.to_le_bytes());
        v[96..100].copy_from_slice(&2u32.to_le_bytes());
        v[108] = 9;
        v[109] = 0;
        v[510] = 0x55;
        v[511] = 0xAA;

        let content = b"ISO: memtest.iso\nNAME: MemTest\n";
        // file entry set at root cluster 2 (heap sector 4), content at cluster 5.
        let mut e85 = [0u8; 32];
        e85[0] = 0x85;
        e85[1] = 2;
        let mut c0 = [0u8; 32];
        c0[0] = 0xC0;
        c0[1] = FLAG_NO_FAT_CHAIN;
        c0[3] = 7; // "cfg.txt"
        c0[20..24].copy_from_slice(&5u32.to_le_bytes());
        c0[24..32].copy_from_slice(&(content.len() as u64).to_le_bytes());
        let mut c1 = [0u8; 32];
        c1[0] = 0xC1;
        for (i, ch) in "cfg.txt".encode_utf16().enumerate() {
            c1[2 + 2 * i..4 + 2 * i].copy_from_slice(&ch.to_le_bytes());
        }
        let heap = 4 * 512;
        v[heap..heap + 32].copy_from_slice(&e85);
        v[heap + 32..heap + 64].copy_from_slice(&c0);
        v[heap + 64..heap + 96].copy_from_slice(&c1);
        // content at cluster 5 -> heap + (5-2)*512
        let cpos = heap + 3 * 512;
        v[cpos..cpos + content.len()].copy_from_slice(content);

        let vol = Volume::open(&mut v).unwrap();
        let entry = vol.find(&mut v, "cfg.txt").expect("file found");
        let data = vol.read_file(&mut v, &entry).unwrap();
        assert_eq!(&data, content);
    }

    #[test]
    fn rejects_non_exfat() {
        let mut junk = vec![0u8; 4096];
        assert!(list_isos(&mut junk).is_none());
        let mut fat32ish = vec![0u8; 4096];
        fat32ish[3..11].copy_from_slice(b"MSDOS5.0");
        fat32ish[510] = 0x55;
        fat32ish[511] = 0xAA;
        assert!(list_isos(&mut fat32ish).is_none());
    }

    #[test]
    fn survives_truncated_device() {
        let mut vol = synthetic_volume();
        vol.truncate(512);
        assert!(list_isos(&mut vol).is_none());
    }
}
