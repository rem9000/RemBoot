//! Host-side check of the exFAT reader against a real disk image.
//! Usage: exfat_ls <image> [partition-byte-offset, default 1 MiB]

use remboot_core::exfat::{self, ByteRead};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

struct FileDev {
    f: File,
    base: u64,
}

impl ByteRead for FileDev {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ()> {
        self.f.seek(SeekFrom::Start(self.base + offset)).map_err(|_| ())?;
        self.f.read_exact(buf).map_err(|_| ())
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: exfat_ls <image> [offset]");
    let base: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1024 * 1024);
    let f = File::open(&path).unwrap();
    let mut dev = FileDev { f, base };
    match exfat::list_isos(&mut dev) {
        Some(isos) => {
            for i in &isos {
                println!("{i}");
            }
            println!("-- {} iso(s)", isos.len());
        }
        None => println!("not an exFAT volume at offset {base}"),
    }
}
