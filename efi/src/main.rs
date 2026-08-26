#![no_std]
#![no_main]

extern crate alloc;

mod gfx;
mod vdisk;

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::time::Duration;
use remboot_core::catalog;
use remboot_core::exfat;
use remboot_core::fx::Particles;
use remboot_core::menu::Menu;
use remboot_core::text::TextRenderer;
use remboot_core::ui;
use uefi::boot::{EventType, OpenProtocolAttributes, OpenProtocolParams, TimerTrigger, Tpl};
use uefi::fs::{FileSystem, Path};
use uefi::prelude::*;
use uefi::proto::console::gop::{GraphicsOutput, Mode};
use uefi::CString16;
use uefi::proto::console::text::{Key, ScanCode};
use uefi::proto::media::block::BlockIO;
use uefi::proto::media::file::{File, FileAttribute, FileMode};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi_raw::protocol::block::BlockIoProtocol;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const FRAME_DT: f32 = 1.0 / 30.0;
/// Confirm modal open/close transition time, seconds.
const CONFIRM_T: f32 = 0.18;

/// SBAT metadata. shim (Secure Boot, >=15.7) refuses to load a binary that
/// lacks a valid `.sbat` section, even with a correct MOK signature. Baked in
/// at link time (objcopy on this PE lands the section at a bad address).
const SBAT_DATA: &[u8] = b"sbat,1,SBAT Version,sbat,1,https://github.com/rhboot/shim/blob/main/SBAT.md\nremboot,1,RemBoot,remboot,1,https://github.com/rem9000/RemBoot\n";

#[used]
#[link_section = ".sbat"]
static SBAT: [u8; SBAT_DATA.len()] = {
    let mut arr = [0u8; SBAT_DATA.len()];
    let mut i = 0;
    while i < SBAT_DATA.len() {
        arr[i] = SBAT_DATA[i];
        i += 1;
    }
    arr
};

/// Pick 1280x800, else 1024x768, else keep the firmware's current mode.
fn pick_mode(gop: &GraphicsOutput) -> Option<Mode> {
    let mut fallback: Option<Mode> = None;
    for mode in gop.modes() {
        let (w, h) = mode.info().resolution();
        if (w, h) == (1280, 800) {
            return Some(mode);
        }
        if (w, h) == (1024, 768) {
            fallback = Some(mode);
        }
    }
    fallback
}

/// Enumerate *.iso files in the root of every filesystem the firmware sees
/// (on a Ventoy stick the ISOs live on the data partition, not the ESP).
fn iso_entries() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let handles = boot::find_handles::<SimpleFileSystem>().unwrap_or_default();
    log::info!("{} filesystem volumes", handles.len());
    for handle in handles {
        let Ok(sfs) = boot::open_protocol_exclusive::<SimpleFileSystem>(handle) else {
            continue;
        };
        let mut fs = FileSystem::new(sfs);
        let Ok(iter) = fs.read_dir(Path::new(cstr16!("\\"))) else {
            continue;
        };
        for info in iter.filter_map(|e| e.ok()) {
            if info.attribute().contains(FileAttribute::DIRECTORY) {
                continue;
            }
            let name = info.file_name().to_string();
            if name.to_ascii_lowercase().ends_with(".iso") {
                out.push(name);
            }
        }
    }
    out
}

/// UEFI BlockIO as byte-addressed reads for the exFAT walker.
struct BlockDev<'a> {
    bio: &'a BlockIO,
    media_id: u32,
    block_size: u64,
}

impl exfat::ByteRead for BlockDev<'_> {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), ()> {
        let bs = self.block_size;
        let first = offset / bs;
        let blocks = (offset + buf.len() as u64).div_ceil(bs) - first;
        let mut tmp = vec![0u8; (blocks * bs) as usize];
        self.bio.read_blocks(self.media_id, first, &mut tmp).map_err(|_| ())?;
        let start = (offset - first * bs) as usize;
        buf.copy_from_slice(&tmp[start..start + buf.len()]);
        Ok(())
    }
}

/// Enumerate *.iso files on exFAT volumes (Ventoy data partitions), which
/// the firmware itself cannot mount.
fn exfat_isos() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for handle in boot::find_handles::<BlockIO>().unwrap_or_default() {
        let params =
            OpenProtocolParams { handle, agent: boot::image_handle(), controller: None };
        // Non-exclusive: we only read, and must not disconnect the firmware's
        // own partition/filesystem drivers.
        let Ok(bio) = (unsafe {
            boot::open_protocol::<BlockIO>(params, OpenProtocolAttributes::GetProtocol)
        }) else {
            continue;
        };
        let media = bio.media();
        if !media.is_media_present() {
            continue;
        }
        let mut dev = BlockDev {
            bio: &bio,
            media_id: media.media_id(),
            block_size: media.block_size() as u64,
        };
        if let Some(mut isos) = exfat::list_isos(&mut dev) {
            log::info!("exFAT volume with {} iso(s)", isos.len());
            out.append(&mut isos);
        }
    }
    out
}

fn approach(cur: f32, target: f32, step: f32) -> f32 {
    if cur < target {
        (cur + step).min(target)
    } else if cur > target {
        (cur - step).max(target)
    } else {
        cur
    }
}

/// Read `\remboot.conf` from the app's own boot volume (the writable, editable
/// copy). Returns None if absent.
fn read_boot_config() -> Option<String> {
    let sfs = boot::get_image_file_system(boot::image_handle()).ok()?;
    let mut fs = FileSystem::new(sfs);
    let bytes = fs.read(Path::new(cstr16!("\\remboot.conf"))).ok()?;
    log::info!("config from boot volume ({} bytes)", bytes.len());
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Write the config back to the app's boot volume (FAT — writable). On a real
/// Ventoy stick this is the VTOYEFI partition the app booted from.
fn write_config(text: &str) -> Result<(), &'static str> {
    let sfs = boot::get_image_file_system(boot::image_handle()).map_err(|_| "no boot filesystem")?;
    let mut fs = FileSystem::new(sfs);
    fs.write(Path::new(cstr16!("\\remboot.conf")), text.as_bytes()).map_err(|_| "write failed")
}

/// Read the effective config: the editable boot-volume copy wins; otherwise
/// fall back to a `remboot.conf` sitting next to the ISOs (exFAT) or on
/// another FAT volume, which acts as a seed.
fn read_config() -> String {
    if let Some(cfg) = read_boot_config() {
        return cfg;
    }
    const EXFAT_NAME: &str = "remboot.conf";
    for handle in boot::find_handles::<BlockIO>().unwrap_or_default() {
        let params = OpenProtocolParams { handle, agent: boot::image_handle(), controller: None };
        let Ok(bio) = (unsafe {
            boot::open_protocol::<BlockIO>(params, OpenProtocolAttributes::GetProtocol)
        }) else {
            continue;
        };
        let media = bio.media();
        if !media.is_media_present() {
            continue;
        }
        let mut dev = BlockDev { bio: &bio, media_id: media.media_id(), block_size: media.block_size() as u64 };
        if let Some(vol) = exfat::Volume::open(&mut dev) {
            if let Some(entry) = vol.find(&mut dev, EXFAT_NAME) {
                if let Some(bytes) = vol.read_file(&mut dev, &entry) {
                    log::info!("config from exFAT ({} bytes)", bytes.len());
                    return String::from_utf8_lossy(&bytes).into_owned();
                }
            }
        }
    }
    for handle in boot::find_handles::<SimpleFileSystem>().unwrap_or_default() {
        if let Ok(sfs) = boot::open_protocol_exclusive::<SimpleFileSystem>(handle) {
            let mut fs = FileSystem::new(sfs);
            if let Ok(bytes) = fs.read(Path::new(cstr16!("\\remboot.conf"))) {
                log::info!("config from FAT ({} bytes)", bytes.len());
                return String::from_utf8_lossy(&bytes).into_owned();
            }
        }
    }
    String::new()
}

/// Locate `name` and boot it as a virtual CD. Tries exFAT data partitions
/// first (where big ISOs live), then plain FAT volumes (a simple stick the
/// firmware can already read). On success this never returns.
fn boot_iso(name: &str) -> Result<(), &'static str> {
    // exFAT volumes.
    for handle in boot::find_handles::<BlockIO>().unwrap_or_default() {
        let params = OpenProtocolParams { handle, agent: boot::image_handle(), controller: None };
        let Ok(bio) = (unsafe {
            boot::open_protocol::<BlockIO>(params, OpenProtocolAttributes::GetProtocol)
        }) else {
            continue;
        };
        let media = bio.media();
        if !media.is_media_present() {
            continue;
        }
        let media_id = media.media_id();
        let block_size = media.block_size() as u64;
        let raw: *const BlockIoProtocol = (&*bio as *const BlockIO).cast();

        let mut dev = BlockDev { bio: &bio, media_id, block_size };
        let Some(volume) = exfat::Volume::open(&mut dev) else {
            continue;
        };
        let Some(files) = volume.list_root(&mut dev) else {
            continue;
        };
        if let Some(entry) = files.into_iter().find(|f| f.name == name) {
            log::info!("booting {} from exFAT ({} bytes)", entry.name, entry.size);
            // Keep the disk protocol open for the life of the boot: the vdisk
            // callback reads through `raw`.
            core::mem::forget(bio);
            return vdisk::boot_iso_exfat(raw, media_id, block_size, &volume, &entry);
        }
    }

    // FAT volumes (the firmware can read the file itself).
    let Ok(cname) = CString16::try_from(name) else {
        return Err("bad ISO name");
    };
    for handle in boot::find_handles::<SimpleFileSystem>().unwrap_or_default() {
        let params = OpenProtocolParams { handle, agent: boot::image_handle(), controller: None };
        let Ok(mut sfs) = (unsafe {
            boot::open_protocol::<SimpleFileSystem>(params, OpenProtocolAttributes::GetProtocol)
        }) else {
            continue;
        };
        let Ok(mut root) = sfs.open_volume() else {
            continue;
        };
        let Ok(fh) = root.open(&cname, FileMode::Read, FileAttribute::empty()) else {
            continue;
        };
        let Some(mut rf) = fh.into_regular_file() else {
            continue;
        };
        // Size via the UEFI "seek to end" convention, then rewind.
        if rf.set_position(u64::MAX).is_err() {
            continue;
        }
        let Ok(size) = rf.get_position() else { continue };
        if rf.set_position(0).is_err() {
            continue;
        }
        log::info!("booting {} from FAT ({} bytes)", name, size);
        // Keep the filesystem protocol open; the vdisk reads the file handle.
        core::mem::forget(sfs);
        return vdisk::boot_iso_file(rf, size);
    }
    Err("selected ISO not found")
}

struct App {
    frame: gfx::Frame,
    layout: ui::Layout,
    assets: ui::FrameAssets,
    tr: TextRenderer,
    items: Vec<catalog::Entry>,
    menu: Menu,
    particles: Particles,
    time: f32,
    intro_skip: bool,
    confirm_open: bool,
    confirm_t: f32,
    have_isos: bool,
    note: Option<(String, bool)>,
    // in-menu config editor
    edit_open: bool,
    edit_t: f32,
    edit_field: usize,
    edit_name: String,
    edit_version: String,
    edit_position: String,
    edit_note: Option<String>,
}

impl App {
    fn intro_running(&self) -> bool {
        !self.intro_skip && self.time < ui::INTRO_LEN
    }

    fn tick(&mut self, dt: f32) {
        self.time += dt;
        self.menu.tick(dt);
        self.particles.tick(dt);
        let ct = if self.confirm_open { 1.0 } else { 0.0 };
        let et = if self.edit_open { 1.0 } else { 0.0 };
        let step = dt / CONFIRM_T;
        self.confirm_t = approach(self.confirm_t, ct, step);
        self.edit_t = approach(self.edit_t, et, step);
    }

    fn open_editor(&mut self) {
        let e = &self.items[self.menu.selected];
        self.edit_name = e.label.clone();
        self.edit_version = e.version.clone().unwrap_or_default();
        self.edit_position = e.position.map(|p| p.to_string()).unwrap_or_default();
        self.edit_field = 0;
        self.edit_note = None;
        self.edit_open = true;
    }

    /// Commit the editor: update the entry, re-order, persist to disk.
    fn commit_editor(&mut self) {
        let idx = self.menu.selected;
        let iso = self.items[idx].iso.clone();
        let name = self.edit_name.trim();
        self.items[idx].label = if name.is_empty() {
            catalog::default_label(&iso)
        } else {
            String::from(name)
        };
        let ver = self.edit_version.trim();
        self.items[idx].version = if ver.is_empty() { None } else { Some(String::from(ver)) };
        self.items[idx].position = self.edit_position.trim().parse::<i32>().ok();

        catalog::sort(&mut self.items);
        // keep the edited entry selected after re-ordering
        if let Some(new_idx) = self.items.iter().position(|e| e.iso == iso) {
            self.menu.selected = new_idx;
        }

        match write_config(&catalog::serialize(&self.items)) {
            Ok(()) => {
                log::info!("config saved");
                self.edit_open = false;
            }
            Err(e) => {
                log::warn!("config save failed: {e}");
                self.edit_note = Some(alloc::format!("SAVE FAILED: {e}"));
            }
        }
    }

    fn edit_active_mut(&mut self) -> &mut String {
        match self.edit_field {
            0 => &mut self.edit_name,
            1 => &mut self.edit_version,
            _ => &mut self.edit_position,
        }
    }

    fn render(&mut self, gop: &mut GraphicsOutput) {
        let vis = ui::Vis {
            time: self.time,
            intro: if self.intro_skip { ui::INTRO_LEN } else { self.time },
            selected: self.menu.selected,
            bar_top: self.layout.bar_y_f(self.menu.bar_pos()),
            bar_vel: self.menu.velocity() * self.layout.item_h as f32,
            glyph_pulse: self.menu.glyph_pulse(),
            bar_pulse: self.menu.bar_pulse(),
            confirm: self.confirm_t,
            confirm_note: self.note.as_ref().map(|(s, e)| (s.as_str(), *e)),
            edit: if self.edit_open || self.edit_t > 0.004 {
                Some(ui::EditView {
                    open: self.edit_t,
                    field: self.edit_field,
                    iso: &self.items[self.menu.selected].iso,
                    name: &self.edit_name,
                    version: &self.edit_version,
                    position: &self.edit_position,
                    cursor: (self.time * 2.0) as u32 % 2 == 0,
                    note: self.edit_note.as_deref(),
                })
            } else {
                None
            },
        };
        ui::compose(
            &mut self.frame.px,
            &self.layout,
            &self.assets,
            &mut self.tr,
            &self.items,
            &vis,
            &self.particles,
        );
        self.frame.present(gop).expect("blt failed");
    }
}

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();
    log::info!("RemBoot {} starting", VERSION);

    let gop_handle = boot::get_handle_for_protocol::<GraphicsOutput>().unwrap();
    let mut gop = boot::open_protocol_exclusive::<GraphicsOutput>(gop_handle).unwrap();
    if let Some(mode) = pick_mode(&gop) {
        gop.set_mode(&mode).unwrap();
    }
    let (w, h) = gop.current_mode_info().resolution();
    log::info!("GOP mode {}x{}", w, h);

    let mut found = iso_entries();
    found.extend(exfat_isos());
    found.sort_unstable_by(|a, b| a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()));
    found.dedup();
    let metas = catalog::parse(&read_config());
    let mut items = catalog::build(&found, &metas);
    log::info!("{} iso entries ({} config records)", items.len(), metas.len());
    let have_isos = !items.is_empty();
    if !have_isos {
        items.push(catalog::Entry {
            iso: String::new(),
            label: String::from("no .iso images found on any volume"),
            version: None,
            position: None,
        });
    }

    let scene = remboot_core::scene::build(w, h);
    log::info!("scene precomputed");
    let layout = ui::layout(w, h, items.len());
    let mut tr = TextRenderer::new();
    let assets = ui::build_assets(scene, &layout, &mut tr, VERSION);
    let menu = Menu::new(items.len());
    let particles = Particles::new(w, h);
    let mut app = App {
        frame: gfx::Frame::new(w, h),
        layout,
        assets,
        tr,
        items,
        menu,
        particles,
        time: 0.0,
        intro_skip: false,
        confirm_open: false,
        confirm_t: 0.0,
        have_isos,
        note: None,
        edit_open: false,
        edit_t: 0.0,
        edit_field: 0,
        edit_name: String::new(),
        edit_version: String::new(),
        edit_position: String::new(),
        edit_note: None,
    };

    // ~30 fps timer + key event; wait_for_event animates while idle and
    // reacts to input without busy-spinning.
    let timer = unsafe { boot::create_event(EventType::TIMER, Tpl::APPLICATION, None, None) }.unwrap();
    boot::set_timer(&timer, TimerTrigger::Periodic(Duration::from_millis(33))).unwrap();
    let key_event = uefi::system::with_stdin(|stdin| stdin.wait_for_key_event().expect("key event"));
    let mut events = [timer, key_event];

    app.render(&mut gop);
    log::info!("menu up");

    loop {
        let which = boot::wait_for_event(&mut events).expect("wait_for_event");
        if which == 0 {
            app.tick(FRAME_DT);
            app.render(&mut gop);
            continue;
        }
        // Drain all pending keys.
        while let Some(key) = uefi::system::with_stdin(|stdin| stdin.read_key().ok().flatten()) {
            if app.intro_running() {
                // First key skips the boot intro.
                app.intro_skip = true;
                continue;
            }
            if app.edit_open {
                match key {
                    Key::Special(ScanCode::UP) => {
                        app.edit_field = (app.edit_field + ui::EDIT_FIELDS - 1) % ui::EDIT_FIELDS;
                    }
                    Key::Special(ScanCode::DOWN) => {
                        app.edit_field = (app.edit_field + 1) % ui::EDIT_FIELDS;
                    }
                    Key::Special(ScanCode::ESCAPE) => {
                        app.edit_open = false;
                    }
                    Key::Printable(c) => {
                        let ch = char::from(c);
                        match ch {
                            '\r' => app.commit_editor(),
                            '\t' => app.edit_field = (app.edit_field + 1) % ui::EDIT_FIELDS,
                            '\u{8}' => {
                                app.edit_active_mut().pop();
                                app.edit_note = None;
                            }
                            c if !c.is_control() => {
                                // Position accepts digits only.
                                if app.edit_field != 2 || c.is_ascii_digit() {
                                    app.edit_active_mut().push(c);
                                    app.edit_note = None;
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
                continue;
            }
            if app.confirm_open || app.confirm_t > 0.5 {
                match key {
                    Key::Special(ScanCode::ESCAPE) => {
                        app.confirm_open = false;
                        app.note = None;
                    }
                    Key::Printable(c) if c == '\r' => {
                        // Show a "starting" state, then attempt the real boot.
                        let iso = app.items[app.menu.selected].iso.clone();
                        app.note = Some((String::from("STARTING\u{2026}"), false));
                        app.render(&mut gop);
                        match boot_iso(&iso) {
                            Ok(()) => return Status::SUCCESS, // unreachable if handoff took
                            Err(e) => {
                                log::warn!("boot failed: {e}");
                                app.note = Some((String::from(e), true));
                                app.render(&mut gop);
                            }
                        }
                    }
                    _ => {}
                }
            } else {
                match key {
                    Key::Special(ScanCode::UP) => {
                        app.menu.move_up();
                    }
                    Key::Special(ScanCode::DOWN) => {
                        app.menu.move_down();
                    }
                    Key::Special(ScanCode::ESCAPE) => {
                        log::info!("ESC — leaving RemBoot");
                        return Status::SUCCESS;
                    }
                    Key::Printable(c) if c == '\r' && app.have_isos => {
                        app.confirm_open = true;
                        app.menu.kick();
                        log::info!("confirm: {}", app.items[app.menu.selected].label);
                    }
                    Key::Printable(c) if (c == 'e' || c == 'E') && app.have_isos => {
                        app.open_editor();
                    }
                    _ => {}
                }
            }
        }
        app.render(&mut gop);
    }
}
