//! Host-side preview: renders key animation moments to PPM files so visuals
//! can be iterated without booting QEMU. Usage: preview <out-dir> [w] [h]

use remboot_core::catalog;
use remboot_core::fx::Particles;
use remboot_core::text::TextRenderer;
use remboot_core::{scene, ui};
use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};

const FOUND: &[&str] = &[
    "Dell_Pro_Max_with_GB10_FCM1253_WW_DGX_OS7_OTA3_Recovery_Image.iso",
    "gparted-live-1.8.1-3-amd64.iso",
    "HBCD_PE_x64.iso",
    "memtest.iso",
    "rescuezilla-2.6.2-64bit.resolute.iso",
    "shredos-2025.11_31_i686_v0.42_20260716_lite.iso",
    "systemrescue-13.02-amd64.iso",
    "ubuntu-26.04-desktop-amd64.iso",
    "Win11_25H2_Dutch_x64_v2.iso",
];

const CONFIG: &str = "\
ISO: Win11_25H2_Dutch_x64_v2.iso
NAME: Windows 11 25H2 (NL)
VERSION: 25H2
POSITION: 1
ISO: ubuntu-26.04-desktop-amd64.iso
NAME: Ubuntu Desktop
VERSION: 26.04 LTS
POSITION: 2
ISO: memtest.iso
NAME: MemTest86+
VERSION: 8.10
POSITION: 3
ISO: gparted-live-1.8.1-3-amd64.iso
NAME: GParted Live
VERSION: 1.8.1
";

struct Shot {
    name: &'static str,
    time: f32,
    intro: f32,
    bar_pos: f32,
    bar_vel: f32,
    confirm: f32,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let dir = args.get(1).cloned().unwrap_or_else(|| ".".into());
    let w: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1280);
    let h: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(800);

    let found: Vec<String> = FOUND.iter().map(|s| s.to_string()).collect();
    let items = catalog::build(&found, &catalog::parse(CONFIG));

    let sc = scene::build(w, h);
    let l = ui::layout(w, h, items.len());
    let mut tr = TextRenderer::new();
    let assets = ui::build_assets(sc, &l, &mut tr, "0.1.0");

    let idle = ui::INTRO_LEN + 3.6;
    let shots = [
        Shot { name: "intro_05", time: 0.5, intro: 0.5, bar_pos: 0.0, bar_vel: 0.0, confirm: 0.0 },
        Shot { name: "intro_10", time: 1.0, intro: 1.0, bar_pos: 0.0, bar_vel: 0.0, confirm: 0.0 },
        Shot { name: "intro_16", time: 1.6, intro: 1.6, bar_pos: 0.0, bar_vel: 0.0, confirm: 0.0 },
        Shot { name: "intro_22", time: 2.2, intro: 2.2, bar_pos: 0.0, bar_vel: 0.0, confirm: 0.0 },
        Shot { name: "idle", time: idle, intro: ui::INTRO_LEN, bar_pos: 1.0, bar_vel: 0.0, confirm: 0.0 },
        Shot { name: "glint", time: 7.35, intro: ui::INTRO_LEN, bar_pos: 1.0, bar_vel: 0.0, confirm: 0.0 },
        Shot {
            name: "flight",
            time: idle + 0.6,
            intro: ui::INTRO_LEN,
            bar_pos: 2.55,
            bar_vel: 620.0,
            confirm: 0.0,
        },
        Shot { name: "confirm_mid", time: idle + 1.0, intro: ui::INTRO_LEN, bar_pos: 3.0, bar_vel: 0.0, confirm: 0.45 },
        Shot { name: "confirm_open", time: idle + 1.4, intro: ui::INTRO_LEN, bar_pos: 3.0, bar_vel: 0.0, confirm: 1.0 },
    ];

    let mut px = vec![0u32; w * h];
    for s in &shots {
        // deterministic particle state at time t
        let mut particles = Particles::new(w, h);
        let steps = (s.time / 0.033) as usize;
        for _ in 0..steps {
            particles.tick(0.033);
        }
        let vis = ui::Vis {
            time: s.time,
            intro: s.intro,
            selected: s.bar_pos.round() as usize,
            bar_top: l.bar_y_f(s.bar_pos),
            bar_vel: s.bar_vel,
            glyph_pulse: 232,
            bar_pulse: 205,
            confirm: s.confirm,
            confirm_note: None,
            edit: None,
        };
        ui::compose(&mut px, &l, &assets, &mut tr, &items, &vis, &particles);

        let path = format!("{dir}/{}.ppm", s.name);
        let f = File::create(&path).unwrap();
        let mut out = BufWriter::new(f);
        write!(out, "P6\n{w} {h}\n255\n").unwrap();
        for p in &px {
            out.write_all(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8]).unwrap();
        }
        println!("wrote {path}");
    }
}
