//! Boot a real ISO by exposing it as a virtual CD.
//!
//! The chosen ISO lives on the exFAT data partition, which UEFI firmware
//! cannot mount. We install our own `BlockIO` protocol whose backing store is
//! that ISO file: `read_blocks` maps 2048-byte CD sectors onto the file's
//! on-disk extents ([`exfat`]) and pulls the bytes from the underlying disk
//! `BlockIO` on demand — nothing is copied into RAM. We then let the firmware
//! bind its El Torito + FAT drivers to the new handle (`connect_controller`)
//! and chainload the ISO's own `\EFI\BOOT\BOOTX64.EFI`.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::ffi::c_void;

use remboot_core::exfat::{self, Extent};
use uefi::proto::device_path::DevicePath;
use uefi::proto::device_path::build::{self, DevicePathBuilder};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::{Handle, Status, boot, cstr16, guid};
use uefi_raw::{Boolean, Guid};
use uefi_raw::protocol::block::{BlockIoMedia, BlockIoProtocol};

const CD_BLOCK: u64 = 2048;
const VDISK_MEDIA_ID: u32 = 0x52_45_4D_42; // "REMB"
const BLOCK_IO_GUID: Guid = BlockIoProtocol::GUID;
const DEVICE_PATH_GUID: Guid = guid!("09576e91-6d3f-11d2-8e39-00a0c969723b");
/// Vendor GUID identifying our virtual disk's device-path root node.
const VDISK_VENDOR_GUID: Guid = guid!("52454d42-0000-4b4f-4f54-000000000001");

/// One extent placed at its cumulative file offset.
struct MappedExtent {
    file_start: u64,
    disk_offset: u64,
    len: u64,
}

/// Backing store + installed protocol for one virtual CD. `proto` MUST be the
/// first field: the firmware hands `read_blocks` a `*BlockIoProtocol` that we
/// cast straight back to `*VDisk`.
#[repr(C)]
struct VDisk {
    proto: BlockIoProtocol,
    media: BlockIoMedia,
    real: *const BlockIoProtocol,
    real_media_id: u32,
    real_block_size: u64,
    extents: Vec<MappedExtent>,
    file_size: u64,
}

impl VDisk {
    /// Read `buf.len()` bytes of the ISO starting at byte `file_off`,
    /// zero-filling past end-of-file.
    fn read_file(&self, file_off: u64, buf: &mut [u8]) -> Result<(), ()> {
        let mut done = 0usize;
        while done < buf.len() {
            let want = file_off + done as u64;
            if want >= self.file_size {
                for b in &mut buf[done..] {
                    *b = 0;
                }
                break;
            }
            let ext = self.extents.iter().find(|e| want >= e.file_start && want < e.file_start + e.len);
            let Some(ext) = ext else {
                // Hole in the map (shouldn't happen): zero-fill and stop.
                for b in &mut buf[done..] {
                    *b = 0;
                }
                break;
            };
            let into_ext = want - ext.file_start;
            let avail = (ext.len - into_ext).min((self.file_size - want) as u64);
            let chunk = avail.min((buf.len() - done) as u64) as usize;
            self.read_disk(ext.disk_offset + into_ext, &mut buf[done..done + chunk])?;
            done += chunk;
        }
        Ok(())
    }

    /// Read an arbitrary byte range from the underlying disk BlockIO, using a
    /// bounce buffer for the (usually aligned) partial head/tail blocks.
    fn read_disk(&self, byte_off: u64, buf: &mut [u8]) -> Result<(), ()> {
        let bs = self.real_block_size;
        let first = byte_off / bs;
        let end = (byte_off + buf.len() as u64).div_ceil(bs);
        let nblocks = end - first;
        let mut tmp = vec![0u8; (nblocks * bs) as usize];
        let read = unsafe { (*self.real).read_blocks };
        let st = unsafe {
            read(self.real, self.real_media_id, first, tmp.len(), tmp.as_mut_ptr().cast())
        };
        if st != Status::SUCCESS {
            return Err(());
        }
        let start = (byte_off - first * bs) as usize;
        buf.copy_from_slice(&tmp[start..start + buf.len()]);
        Ok(())
    }
}

unsafe extern "efiapi" fn vd_reset(_this: *mut BlockIoProtocol, _ext: Boolean) -> Status {
    Status::SUCCESS
}

unsafe extern "efiapi" fn vd_read(
    this: *const BlockIoProtocol,
    media_id: u32,
    lba: u64,
    size: usize,
    buffer: *mut c_void,
) -> Status {
    let vd = this as *const VDisk;
    let vd = unsafe { &*vd };
    if media_id != VDISK_MEDIA_ID {
        return Status::MEDIA_CHANGED;
    }
    if size == 0 {
        return Status::SUCCESS;
    }
    if size % CD_BLOCK as usize != 0 {
        return Status::BAD_BUFFER_SIZE;
    }
    let buf = unsafe { core::slice::from_raw_parts_mut(buffer.cast::<u8>(), size) };
    match vd.read_file(lba * CD_BLOCK, buf) {
        Ok(()) => Status::SUCCESS,
        Err(()) => Status::DEVICE_ERROR,
    }
}

unsafe extern "efiapi" fn vd_write(
    _this: *mut BlockIoProtocol,
    _media_id: u32,
    _lba: u64,
    _size: usize,
    _buffer: *const c_void,
) -> Status {
    Status::WRITE_PROTECTED
}

unsafe extern "efiapi" fn vd_flush(_this: *mut BlockIoProtocol) -> Status {
    Status::SUCCESS
}

/// Build, install and connect a virtual CD for `entry` on exFAT `volume`
/// (backed by disk handle `disk`), then chainload its EFI bootloader.
/// On success this does not return (control passes to the ISO's loader).
pub fn boot_iso(
    disk: Handle,
    real: *const BlockIoProtocol,
    real_media_id: u32,
    real_block_size: u64,
    volume: &exfat::Volume,
    entry: &exfat::FileEntry,
) -> Result<(), &'static str> {
    let _ = disk;
    let raw_extents = resolve_extents(real, real_media_id, real_block_size, volume, entry)?;
    let mut extents = Vec::with_capacity(raw_extents.len());
    let mut file_start = 0u64;
    for e in raw_extents {
        extents.push(MappedExtent { file_start, disk_offset: e.disk_offset, len: e.len });
        file_start += e.len;
    }
    if entry.size == 0 {
        return Err("empty ISO");
    }
    let last_block = entry.size.div_ceil(CD_BLOCK) - 1;

    let mut vd = Box::new(VDisk {
        proto: BlockIoProtocol {
            revision: 1,
            media: core::ptr::null(),
            reset: vd_reset,
            read_blocks: vd_read,
            write_blocks: vd_write,
            flush_blocks: vd_flush,
        },
        media: BlockIoMedia {
            media_id: VDISK_MEDIA_ID,
            removable_media: true.into(),
            media_present: true.into(),
            logical_partition: false.into(),
            read_only: true.into(),
            write_caching: false.into(),
            block_size: CD_BLOCK as u32,
            io_align: 0,
            last_block,
            lowest_aligned_lba: 0,
            logical_blocks_per_physical_block: 1,
            optimal_transfer_length_granularity: 1,
        },
        real,
        real_media_id,
        real_block_size,
        extents,
        file_size: entry.size,
    });
    // Self-referential pointer, fixed once the Box has a stable address.
    vd.proto.media = &vd.media as *const BlockIoMedia;

    // Leak: the firmware keeps these pointers for the lifetime of the boot.
    let vd: &'static mut VDisk = Box::leak(vd);
    let proto_ptr: *const BlockIoProtocol = &vd.proto;

    // Install BlockIO on a fresh handle, then a device path so the firmware's
    // bus drivers can bind and address children.
    let handle = unsafe {
        boot::install_protocol_interface(None, &BLOCK_IO_GUID, proto_ptr.cast())
            .map_err(|_| "install BlockIO failed")?
    };

    let mut dp_buf = Vec::new();
    let dp = DevicePathBuilder::with_vec(&mut dp_buf)
        .push(&build::hardware::Vendor {
            vendor_guid: VDISK_VENDOR_GUID,
            vendor_defined_data: &[],
        })
        .map_err(|_| "device path build failed")?
        .finalize()
        .map_err(|_| "device path finalize failed")?;
    let dp_bytes: &'static [u8] = Box::leak(dp.as_bytes().to_vec().into_boxed_slice());
    unsafe {
        boot::install_protocol_interface(Some(handle), &DEVICE_PATH_GUID, dp_bytes.as_ptr().cast())
            .map_err(|_| "install device path failed")?;
    }

    // Snapshot filesystems, bind drivers to the new disk, then find the child
    // filesystem(s) the El Torito + FAT stack created.
    let before = boot::find_handles::<SimpleFileSystem>().unwrap_or_default();
    let _ = boot::connect_controller(handle, &[], None, true);
    let after = boot::find_handles::<SimpleFileSystem>().unwrap_or_default();

    let fresh: Vec<Handle> = after.into_iter().filter(|h| !before.contains(h)).collect();
    if fresh.is_empty() {
        return Err("firmware exposed no filesystem on the ISO (no EFI boot image?)");
    }

    for fs_handle in fresh {
        if let Some(img) = try_load_bootloader(fs_handle) {
            log::info!("chainloading ISO bootloader");
            // Give the loaded OS a clean slate: cancel our watchdog.
            let _ = boot::set_watchdog_timer(0, 0x1ffff, None);
            match boot::start_image(img) {
                Ok(()) => return Ok(()),
                Err(e) => log::warn!("start_image returned {e:?}"),
            }
        }
    }
    Err("no bootable \\EFI\\BOOT\\BOOTX64.EFI on the ISO")
}

fn resolve_extents(
    real: *const BlockIoProtocol,
    media_id: u32,
    block_size: u64,
    volume: &exfat::Volume,
    entry: &exfat::FileEntry,
) -> Result<Vec<Extent>, &'static str> {
    struct Dev {
        real: *const BlockIoProtocol,
        media_id: u32,
        bs: u64,
    }
    impl exfat::ByteRead for Dev {
        fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ()> {
            let first = offset / self.bs;
            let end = (offset + buf.len() as u64).div_ceil(self.bs);
            let mut tmp = vec![0u8; ((end - first) * self.bs) as usize];
            let read = unsafe { (*self.real).read_blocks };
            let st =
                unsafe { read(self.real, self.media_id, first, tmp.len(), tmp.as_mut_ptr().cast()) };
            if st != Status::SUCCESS {
                return Err(());
            }
            let start = (offset - first * self.bs) as usize;
            buf.copy_from_slice(&tmp[start..start + buf.len()]);
            Ok(())
        }
    }
    let mut dev = Dev { real, media_id, bs: block_size };
    volume.extents(&mut dev, entry).ok_or("could not resolve ISO extents")
}

/// Try to load `\EFI\BOOT\BOOTX64.EFI` from `fs_handle`, returning the loaded
/// image handle. Builds the full device path (handle path + file node).
fn try_load_bootloader(fs_handle: Handle) -> Option<Handle> {
    let dp = boot::open_protocol_exclusive::<DevicePath>(fs_handle).ok()?;

    let mut buf = Vec::new();
    let mut builder = DevicePathBuilder::with_vec(&mut buf);
    for node in dp.node_iter() {
        // Skip the trailing end node; the builder appends its own.
        if node.full_type()
            == (
                uefi::proto::device_path::DeviceType::END,
                uefi::proto::device_path::DeviceSubType::END_ENTIRE,
            )
        {
            continue;
        }
        builder = builder.push(&node).ok()?;
    }
    let full = builder
        .push(&build::media::FilePath { path_name: cstr16!("\\EFI\\BOOT\\BOOTX64.EFI") })
        .ok()?
        .finalize()
        .ok()?;

    boot::load_image(
        boot::image_handle(),
        boot::LoadImageSource::FromDevicePath {
            device_path: full,
            boot_policy: uefi::proto::BootPolicy::ExactMatch,
        },
    )
    .ok()
}
