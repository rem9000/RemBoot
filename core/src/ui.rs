//! Menu chrome + frame composition.
//!
//! The aurora background is the only fully baked layer; everything else
//! (glyph, wordmark, glass panels, bar, texts) is blitted per frame from
//! prerendered sprites so the intro can stage them in and the idle scene can
//! keep breathing. All per-frame blending is integer math.

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use libm::sqrtf;

use crate::fx::{self, Particles, ease_out_cubic, stage};
use crate::pix::{add_scaled, over, rgb, rgb_t};
use crate::scene::{GlyphSprite, Scene, Sprite, scale};
use crate::text::{Face, TextRenderer};
use crate::theme::*;

/// Length of the boot intro sequence, seconds.
pub const INTRO_LEN: f32 = 2.7;

// ------------------------------------------------------------------ layout

pub struct Layout {
    pub w: usize,
    pub h: usize,
    pub s: f32,
    pub panel_x: usize,
    pub panel_y: usize,
    pub panel_w: usize,
    pub panel_h: usize,
    pub item_h: usize,
    pub items_top: usize,
    pub item_font: u32,
    pub item_text_x: usize,
    pub bar_x: usize,
    pub bar_w: usize,
    pub bar_h: usize,
}

impl Layout {
    #[inline]
    pub fn px(&self, v: f32) -> usize {
        (v * self.s + 0.5) as usize
    }

    /// Top y of the selection bar when item `i` is selected.
    pub fn bar_y(&self, i: usize) -> usize {
        self.items_top + i * self.item_h + self.px(4.0)
    }

    /// Continuous variant for the animated bar (`pos` in item-index space).
    pub fn bar_y_f(&self, pos: f32) -> f32 {
        self.items_top as f32 + pos * self.item_h as f32 + 4.0 * self.s
    }

    /// Text baseline for item `i`.
    pub fn item_baseline(&self, i: usize) -> i32 {
        (self.items_top + i * self.item_h) as i32
            + (self.item_h as f32 * 0.5 + self.item_font as f32 * 0.34) as i32
    }
}

pub fn layout(w: usize, h: usize, n_items: usize) -> Layout {
    let s = scale(w, h);
    let px = |v: f32| (v * s + 0.5) as usize;
    let panel_w = px(680.0);
    let pad = px(14.0);
    let panel_y = px(302.0);
    // Rows shrink when the list is long, so the panel stays above the footer.
    let footer_top = h.saturating_sub(px(56.0));
    let avail = footer_top.saturating_sub(panel_y + 2 * pad);
    let item_h = (avail / n_items.max(1)).min(px(44.0)).max(px(26.0));
    let panel_h = pad * 2 + item_h * n_items.max(1);
    let panel_x = (w - panel_w) / 2;
    let bar_inset = px(10.0);
    Layout {
        w,
        h,
        s,
        panel_x,
        panel_y,
        panel_w,
        panel_h,
        item_h,
        items_top: panel_y + pad,
        item_font: px(16.0) as u32,
        item_text_x: panel_x + px(38.0),
        bar_x: panel_x + bar_inset,
        bar_w: panel_w - 2 * bar_inset,
        bar_h: item_h - px(8.0),
    }
}

// ------------------------------------------------------------- primitives

/// Signed distance to a rounded rectangle centered at (cx, cy).
fn rr_dist(px: f32, py: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = (px - cx).abs() - (hw - r);
    let qy = (py - cy).abs() - (hh - r);
    let ox = if qx > 0.0 { qx } else { 0.0 };
    let oy = if qy > 0.0 { qy } else { 0.0 };
    let inner = if qx > qy { qx } else { qy };
    sqrtf(ox * ox + oy * oy) + if inner < 0.0 { inner } else { 0.0 } - r
}

fn lerp_col(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> u32 {
    let r = (a.0 as f32 + (b.0 as f32 - a.0 as f32) * t) as u32;
    let g = (a.1 as f32 + (b.1 as f32 - a.1 as f32) * t) as u32;
    let bl = (a.2 as f32 + (b.2 as f32 - a.2 as f32) * t) as u32;
    (r << 16) | (g << 8) | bl
}

/// Truncate `name` with an ellipsis so it fits `max_w` px (monospace math).
fn fit_text(tr: &mut TextRenderer, name: &str, face: Face, size: u32, max_w: i32) -> String {
    if tr.width(name, face, size, 0) <= max_w {
        return String::from(name);
    }
    let adv = tr.width("M", face, size, 0).max(1);
    let max_chars = ((max_w / adv) as usize).saturating_sub(1).max(1);
    let mut s: String = name.chars().take(max_chars).collect();
    s.push('\u{2026}');
    s
}

/// Darken the whole frame (fades, modal backdrop).
pub fn dim(buf: &mut [u32], alpha: u32) {
    if alpha == 0 {
        return;
    }
    for p in buf.iter_mut() {
        *p = over(*p, 0x000000, alpha);
    }
}

/// Additively blend sprite `s` with its top-left at (x, y).
pub fn add_sprite_at(buf: &mut [u32], stride: usize, s: &Sprite, x: usize, y: usize, alpha: u32) {
    if alpha == 0 {
        return;
    }
    let height = buf.len() / stride;
    for sy in 0..s.h {
        let py = y + sy;
        if py >= height {
            break;
        }
        let brow = py * stride;
        let srow = sy * s.w;
        for sx in 0..s.w {
            let c = s.rgb[srow + sx];
            if c != 0 {
                let px = x + sx;
                if px < stride {
                    buf[brow + px] = add_scaled(buf[brow + px], c, alpha);
                }
            }
        }
    }
}

// --------------------------------------------------------- glyph reveal --

const REVEAL_W: i32 = 48;

#[inline]
fn reveal_edge(prog255w: i32, t: u8) -> u32 {
    (((prog255w - t as i32) * 255) / REVEAL_W).clamp(0, 255) as u32
}

fn blit_glyph(buf: &mut [u32], stride: usize, g: &GlyphSprite, progress: f32, alpha: u32, additive: bool) {
    if alpha == 0 || progress <= 0.0 {
        return;
    }
    let prog = (progress.min(1.0) * (255.0 + REVEAL_W as f32)) as i32;
    let height = buf.len() / stride;
    for sy in 0..g.h {
        let py = g.y + sy;
        if py >= height {
            break;
        }
        let brow = py * stride;
        let srow = sy * g.w;
        for sx in 0..g.w {
            let a = g.a[srow + sx] as u32;
            if a == 0 {
                continue;
            }
            let e = reveal_edge(prog, g.t[srow + sx]);
            if e == 0 {
                continue;
            }
            let eff = a * e * alpha / (255 * 255);
            if eff == 0 {
                continue;
            }
            let px = g.x + sx;
            if px < stride {
                let i = brow + px;
                buf[i] = if additive {
                    add_scaled(buf[i], g.rgb[srow + sx], eff)
                } else {
                    over(buf[i], g.rgb[srow + sx], eff)
                };
            }
        }
    }
}

// ---------------------------------------------------------- glass sprite --

/// Prerendered rounded glass rectangle with straight alpha.
pub struct GlassSprite {
    pub w: usize,
    pub h: usize,
    pub margin: usize,
    pub rect_w: usize,
    pub rect_h: usize,
    pub rgb: Vec<u32>,
    pub a: Vec<u8>,
}

pub fn glass_sprite(
    rect_w: usize,
    rect_h: usize,
    s: f32,
    fill: (u8, u8, u8, u8),
    border: (u8, u8, u8),
    border_alpha: u8,
) -> GlassSprite {
    let radius = 14.0 * s;
    let blur = (1.4 * s).max(1.0);
    let m = (blur + 3.0) as usize;
    let (w, h) = (rect_w + 2 * m, rect_h + 2 * m);
    let mut rgbv = vec![0u32; w * h];
    let mut av = vec![0u8; w * h];
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let (hw, hh) = (rect_w as f32 / 2.0, rect_h as f32 / 2.0);
    let (fr, fg, fb) = (fill.0 as f32, fill.1 as f32, fill.2 as f32);
    let (br, bg_, bb) = (border.0 as f32, border.1 as f32, border.2 as f32);
    for y in 0..h {
        for x in 0..w {
            let d = rr_dist(x as f32 + 0.5, y as f32 + 0.5, cx, cy, hw, hh, radius);
            let cov = (0.5 - d).clamp(0.0, 1.0);
            let fa = fill.3 as f32 / 255.0 * cov;
            let bd = d.abs();
            let ba = if bd < blur { border_alpha as f32 / 255.0 * (1.0 - bd / blur) } else { 0.0 };
            let out_a = ba + fa * (1.0 - ba);
            if out_a > 0.003 {
                let r = (br * ba + fr * fa * (1.0 - ba)) / out_a;
                let g = (bg_ * ba + fg * fa * (1.0 - ba)) / out_a;
                let b = (bb * ba + fb * fa * (1.0 - ba)) / out_a;
                let i = y * w + x;
                rgbv[i] = ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
                av[i] = (out_a * 255.0) as u8;
            }
        }
    }
    GlassSprite { w, h, margin: m, rect_w, rect_h, rgb: rgbv, a: av }
}

/// Blit with the *rect* top-left at (rx, ry); the blur margin extends around.
pub fn blit_glass(buf: &mut [u32], stride: usize, g: &GlassSprite, rx: i32, ry: i32, alpha: u32) {
    if alpha == 0 {
        return;
    }
    let height = (buf.len() / stride) as i32;
    let ox = rx - g.margin as i32;
    let oy = ry - g.margin as i32;
    for sy in 0..g.h {
        let py = oy + sy as i32;
        if py < 0 || py >= height {
            continue;
        }
        let brow = py as usize * stride;
        let srow = sy * g.w;
        for sx in 0..g.w {
            let a = g.a[srow + sx] as u32;
            if a == 0 {
                continue;
            }
            let px = ox + sx as i32;
            if px < 0 || px >= stride as i32 {
                continue;
            }
            let i = brow + px as usize;
            buf[i] = over(buf[i], g.rgb[srow + sx], a * alpha / 255);
        }
    }
}

/// Nearest-neighbour scaled blit, centered on (cx, cy) — modal pop-in.
pub fn blit_glass_scaled(buf: &mut [u32], stride: usize, g: &GlassSprite, cx: i32, cy: i32, s: f32, alpha: u32) {
    if alpha == 0 {
        return;
    }
    let height = (buf.len() / stride) as i32;
    let dw = (g.w as f32 * s) as usize;
    let dh = (g.h as f32 * s) as usize;
    if dw == 0 || dh == 0 {
        return;
    }
    let ox = cx - (dw / 2) as i32;
    let oy = cy - (dh / 2) as i32;
    for dy in 0..dh {
        let py = oy + dy as i32;
        if py < 0 || py >= height {
            continue;
        }
        let sy = dy * g.h / dh;
        let brow = py as usize * stride;
        let srow = sy * g.w;
        for dx in 0..dw {
            let px = ox + dx as i32;
            if px < 0 || px >= stride as i32 {
                continue;
            }
            let sx = dx * g.w / dw;
            let a = g.a[srow + sx] as u32;
            if a == 0 {
                continue;
            }
            let i = brow + px as usize;
            buf[i] = over(buf[i], g.rgb[srow + sx], a * alpha / 255);
        }
    }
}

// ----------------------------------------------------------- bar sprites --

/// Selection bar as a premade sprite: rgb + coverage-alpha.
/// Gradient port of gen_theme.py's select_bar().
pub struct BarSprite {
    pub w: usize,
    pub h: usize,
    pub rgb: Vec<u32>,
    pub a: Vec<u8>,
}

pub fn bake_bar(l: &Layout) -> BarSprite {
    let (w, h) = (l.bar_w, l.bar_h);
    let radius = 9.0 * l.s;
    let mut rgbv = vec![0u32; w * h];
    let mut av = vec![0u8; w * h];
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    for y in 0..h {
        let t = y as f32 / (h - 1) as f32;
        let top = (
            (CYAN.0 as f32 * 1.12).min(255.0) as u8,
            (CYAN.1 as f32 * 1.05).min(255.0) as u8,
            (CYAN.2 as f32 * 1.02).min(255.0) as u8,
        );
        let bottom = (
            (CYAN.0 as f32 + (INDIGO.0 as f32 - CYAN.0 as f32) * 0.55) as u8,
            (CYAN.1 as f32 + (INDIGO.1 as f32 - CYAN.1 as f32) * 0.55) as u8,
            (CYAN.2 as f32 + (INDIGO.2 as f32 - CYAN.2 as f32) * 0.55) as u8,
        );
        let mut col = lerp_col(top, bottom, t);
        let mut alpha = 208.0;
        if y == 0 {
            col = rgb(210, 250, 255);
            alpha = 255.0;
        } else if y == h - 1 {
            alpha = 150.0;
        }
        for x in 0..w {
            let d = rr_dist(x as f32 + 0.5, y as f32 + 0.5, cx, cy, cx, cy, radius);
            let cov = (0.5 - d).clamp(0.0, 1.0);
            let i = y * w + x;
            rgbv[i] = col;
            av[i] = (alpha * cov) as u8;
        }
    }
    BarSprite { w, h, rgb: rgbv, a: av }
}

/// Soft additive glow around the bar, pulsed per-frame.
pub fn bake_bar_glow(l: &Layout) -> Sprite {
    let margin = l.px(18.0);
    let w = l.bar_w + margin * 2;
    let h = l.bar_h + margin * 2;
    let sigma = 8.0 * l.s;
    let radius = 9.0 * l.s;
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let hw = l.bar_w as f32 / 2.0;
    let hh = l.bar_h as f32 / 2.0;
    let mut rgbv = vec![0u32; w * h];
    for y in 0..h {
        for x in 0..w {
            let d = rr_dist(x as f32 + 0.5, y as f32 + 0.5, cx, cy, hw, hh, radius);
            if d > 0.0 && d < margin as f32 {
                let amt = 0.30 * libm::expf(-(d * d) / (2.0 * sigma * sigma));
                let t = x as f32 / w as f32;
                let col = lerp_col(CYAN, INDIGO, t);
                let r = (((col >> 16) & 0xFF) as f32 * amt) as u32;
                let g = (((col >> 8) & 0xFF) as f32 * amt) as u32;
                let b = ((col & 0xFF) as f32 * amt) as u32;
                rgbv[y * w + x] = (r << 16) | (g << 8) | b;
            }
        }
    }
    // x/y hold the margins; the blit position is computed per frame.
    Sprite { x: margin, y: margin, w, h, rgb: rgbv }
}

/// Blit the bar vertically stretched to `h_eff` (motion smear), top at `top`.
pub fn blit_bar_stretched(buf: &mut [u32], stride: usize, bar: &BarSprite, x: usize, top: i32, h_eff: usize, alpha: u32) {
    if alpha == 0 || h_eff == 0 {
        return;
    }
    let height = (buf.len() / stride) as i32;
    let moving = h_eff != bar.h;
    for dy in 0..h_eff {
        let py = top + dy as i32;
        if py < 0 || py >= height {
            continue;
        }
        let mut sy = dy * bar.h / h_eff;
        // While smeared in motion, skip the crisp specular top line.
        if moving && sy == 0 {
            sy = 1.min(bar.h - 1);
        }
        let brow = py as usize * stride + x;
        let srow = sy * bar.w;
        for sx in 0..bar.w {
            let a = bar.a[srow + sx] as u32;
            if a > 0 && x + sx < stride {
                buf[brow + sx] = over(buf[brow + sx], bar.rgb[srow + sx], a * alpha / 255);
            }
        }
    }
}

// ---------------------------------------------------------------- chrome --

pub struct Chrome {
    /// Wordmark letters: (char, pen x, color).
    pub letters: Vec<(char, i32, u32)>,
    pub wm_baseline: i32,
    pub wm_size: u32,
    pub wm_center: f32,
    pub wm_half_w: f32,
    pub sub_baseline: i32,
    pub sub_size: u32,
    pub sub_tracking: i32,
    pub version: String,
}

fn build_chrome(l: &Layout, tr: &mut TextRenderer, glyph_bottom: usize, version: &str) -> Chrome {
    let cx = (l.w / 2) as i32;
    let size = l.px(42.0) as u32;
    let tracking = l.px(18.0) as i32;
    let text = "REMBOOT";
    let total = tr.width(text, Face::Bold, size, tracking);
    let mut pen = cx - total / 2;
    let n = text.chars().count();
    let mut letters = Vec::new();
    for (i, ch) in text.chars().enumerate() {
        let t = i as f32 / (n - 1) as f32;
        let col = lerp_col(TEXT, INDIGO, t * 0.55);
        letters.push((ch, pen, col));
        let mut s = String::new();
        s.push(ch);
        pen += tr.width(&s, Face::Bold, size, 0) + tracking;
    }
    Chrome {
        letters,
        wm_baseline: (glyph_bottom + l.px(64.0)) as i32,
        wm_size: size,
        wm_center: cx as f32,
        wm_half_w: total as f32 / 2.0,
        sub_baseline: (glyph_bottom + l.px(96.0)) as i32,
        sub_size: l.px(13.0) as u32,
        sub_tracking: l.px(6.0) as i32,
        version: String::from(version),
    }
}

// ---------------------------------------------------------------- assets --

pub struct FrameAssets {
    pub bg: Vec<u32>,
    pub glyph: GlyphSprite,
    pub glyph_glow: GlyphSprite,
    pub panel: GlassSprite,
    pub modal: GlassSprite,
    pub edit_modal: GlassSprite,
    pub bar: BarSprite,
    pub bar_glow: Sprite,
    pub chrome: Chrome,
}

pub fn build_assets(scene: Scene, l: &Layout, tr: &mut TextRenderer, version: &str) -> FrameAssets {
    let Scene { bg, glyph, glyph_glow, glyph_bottom } = scene;
    let chrome = build_chrome(l, tr, glyph_bottom, version);
    let panel = glass_sprite(l.panel_w, l.panel_h, l.s, PANEL_FILL, PANEL_BORDER, PANEL_BORDER_ALPHA);
    let modal = glass_sprite(l.px(600.0), l.px(190.0), l.s, (7, 11, 20, 224), PANEL_BORDER, 70);
    let edit_modal = glass_sprite(l.px(600.0), l.px(290.0), l.s, (7, 11, 20, 230), PANEL_BORDER, 70);
    FrameAssets { bg, glyph, glyph_glow, panel, modal, edit_modal, bar: bake_bar(l), bar_glow: bake_bar_glow(l), chrome }
}

// --------------------------------------------------------------- compose --

/// Everything time-varying that a frame needs, precomputed by the caller.
pub struct Vis<'a> {
    pub time: f32,
    /// Seconds into the intro; pass >= INTRO_LEN when done/skipped.
    pub intro: f32,
    pub selected: usize,
    /// Bar top in pixels (float, mid-flight).
    pub bar_top: f32,
    /// Bar velocity in px/sec (drives motion stretch).
    pub bar_vel: f32,
    pub glyph_pulse: u32,
    pub bar_pulse: u32,
    /// Confirm modal openness 0..=1.
    pub confirm: f32,
    /// Replaces the modal hint line: (text, is_error).
    pub confirm_note: Option<(&'a str, bool)>,
    /// Edit overlay, when open.
    pub edit: Option<EditView<'a>>,
}

/// State of the in-menu config editor.
pub struct EditView<'a> {
    /// Open/close transition 0..=1.
    pub open: f32,
    /// Active field: 0 = name, 1 = version, 2 = position.
    pub field: usize,
    pub iso: &'a str,
    pub name: &'a str,
    pub version: &'a str,
    pub position: &'a str,
    /// Cursor blink on/off.
    pub cursor: bool,
    /// Optional status line (e.g. save error), amber.
    pub note: Option<&'a str>,
}

pub const EDIT_FIELDS: usize = 3;

#[allow(clippy::too_many_arguments)]
pub fn compose(
    px: &mut [u32],
    l: &Layout,
    a: &FrameAssets,
    tr: &mut TextRenderer,
    items: &[crate::catalog::Entry],
    vis: &Vis,
    particles: &Particles,
) {
    let it = vis.intro;
    let w = l.w;

    // 1. background + global fade-in
    px.copy_from_slice(&a.bg);
    let bg_a = stage(it, 0.0, 0.75);
    let env_a = (bg_a * 255.0) as u32;
    if env_a < 255 {
        dim(px, 255 - env_a);
    }

    // 2. ambient motion
    fx::draw_sweep(px, w, l.h, vis.time, env_a);
    particles.draw(px, w, (l.panel_x, l.panel_y, l.panel_w, l.panel_h), env_a);

    // 3. glyph reveal + pulsing glow
    let gprog = ease_out_cubic(stage(it, 0.30, 1.0));
    blit_glyph(px, w, &a.glyph, gprog, 255, false);
    let glow_a = (stage(it, 1.05, 0.7) * vis.glyph_pulse as f32) as u32;
    blit_glyph(px, w, &a.glyph_glow, gprog, glow_a, true);

    // 4. wordmark (staggered rise; occasional specular glint when idle)
    let c = &a.chrome;
    let idle = it >= INTRO_LEN;
    for (i, (ch, x, col)) in c.letters.iter().enumerate() {
        let la = stage(it, 0.75 + i as f32 * 0.07, 0.4);
        if la <= 0.0 {
            continue;
        }
        let dy = ((1.0 - ease_out_cubic(la)) * 12.0) as i32;
        let mut color = *col;
        if idle {
            let g = fx::glint(vis.time, *x as f32, c.wm_center, c.wm_half_w);
            if g > 0.01 {
                color = lerp_col(
                    (((color >> 16) & 0xFF) as u8, ((color >> 8) & 0xFF) as u8, (color & 0xFF) as u8),
                    (235, 245, 255),
                    g,
                );
            }
        }
        let mut s = String::new();
        s.push(*ch);
        tr.draw(px, w, *x, c.wm_baseline + dy, &s, Face::Bold, c.wm_size, color, (la * 255.0) as u32, 0);
    }

    // 5. subtitle
    let sa = stage(it, 1.30, 0.45);
    if sa > 0.0 {
        tr.draw_centered(
            px,
            w,
            (w / 2) as i32,
            c.sub_baseline,
            "NATIVE UEFI BOOT MENU",
            Face::Regular,
            c.sub_size,
            rgb_t(TEXT_FAINT),
            (sa * 255.0) as u32,
            c.sub_tracking,
        );
    }

    // The menu layer recedes while an overlay (confirm or edit) opens.
    let ce = ease_out_cubic(vis.confirm);
    let ee = vis.edit.as_ref().map(|e| ease_out_cubic(e.open)).unwrap_or(0.0);
    let menu_fade = 1.0 - 0.92 * ce.max(ee);

    // 6. glass panel (slides up while fading in)
    let pa = stage(it, 1.45, 0.5) * menu_fade;
    let pdy = ((1.0 - ease_out_cubic(stage(it, 1.45, 0.5))) * 26.0) as i32;
    if pa > 0.0 {
        blit_glass(px, w, &a.panel, l.panel_x as i32, l.panel_y as i32 + pdy, (pa * 255.0) as u32);
    }

    // 7. selection bar with motion stretch + pulsing glow
    let ba = stage(it, 1.85, 0.45) * menu_fade;
    let stretch = 1.0 + (vis.bar_vel.abs() * 0.00075).min(0.30);
    let h_eff = (a.bar.h as f32 * stretch) as usize;
    let bar_top_draw = (vis.bar_top - (h_eff as f32 - a.bar.h as f32) / 2.0) as i32 + pdy;
    if ba > 0.0 {
        let gx = l.bar_x.saturating_sub(a.bar_glow.x);
        let gy = (bar_top_draw - a.bar_glow.y as i32).max(0) as usize;
        add_sprite_at(px, w, &a.bar_glow, gx, gy, vis.bar_pulse * (ba * 255.0) as u32 / 255);
        blit_bar_stretched(px, w, &a.bar, l.bar_x, bar_top_draw, h_eff, (ba * 255.0) as u32);
    }

    // 8. item texts (staggered rise; dark when covered by the bar). Each row
    //    is `NN  label ............... version`, version right-aligned & dim.
    let idx_font = l.px(12.0) as u32;
    let ver_font = l.px(12.0) as u32;
    for (i, entry) in items.iter().enumerate() {
        let ia = stage(it, 1.55 + i as f32 * 0.06, 0.35) * menu_fade;
        if ia <= 0.0 {
            continue;
        }
        let idy = ((1.0 - ease_out_cubic(ia)) * 14.0) as i32 + pdy;
        let center = (l.items_top + i * l.item_h + l.item_h / 2) as i32 + pdy;
        let on_bar = ba > 0.0 && center >= bar_top_draw && center < bar_top_draw + h_eff as i32;
        let (col, icol, vcol) = if on_bar {
            (rgb_t(TEXT_ON_BAR), rgb_t(TEXT_ON_BAR_DIM), rgb_t(TEXT_ON_BAR_DIM))
        } else if i == vis.selected {
            (rgb_t(TEXT), rgb_t(TEXT_DIM), rgb_t(TEXT_DIM))
        } else {
            (rgb_t(TEXT_DIM), rgb_t(TEXT_FAINT), rgb_t(TEXT_FAINT))
        };
        let alpha = (ia * 255.0) as u32;
        let baseline = l.item_baseline(i) + idy;
        let num = format!("{:02}", i + 1);
        tr.draw(px, w, l.item_text_x as i32, baseline, &num, Face::Regular, idx_font, icol, alpha, 0);

        let right_edge = (l.panel_x + l.panel_w - l.px(24.0)) as i32;
        // Version, right-aligned.
        let mut name_right = right_edge;
        if let Some(ver) = &entry.version {
            let vw = tr.width(ver, Face::Regular, ver_font, 0);
            tr.draw(px, w, right_edge - vw, baseline, ver, Face::Regular, ver_font, vcol, alpha, 0);
            name_right = right_edge - vw - l.px(16.0) as i32;
        }
        let name_x = (l.item_text_x + l.px(40.0)) as i32;
        let shown = fit_text(tr, &entry.label, Face::Regular, l.item_font, name_right - name_x);
        tr.draw(px, w, name_x, baseline, &shown, Face::Regular, l.item_font, col, alpha, 0);
    }

    // 9. footer hints + version
    let fa = stage(it, 2.0, 0.5) * menu_fade;
    if fa > 0.0 {
        let alpha = (fa * 255.0) as u32;
        let cx = (w / 2) as i32;
        let fy = (l.h - l.px(30.0)) as i32;
        let fsize = l.px(13.0) as u32;
        let dim_c = rgb_t(TEXT_DIM);
        let faint = rgb_t(TEXT_FAINT);
        let seg = [
            ("\u{2191}\u{2193} ", true), ("SELECT     ", false),
            ("ENTER ", true), ("BOOT     ", false),
            ("E ", true), ("EDIT     ", false),
            ("ESC ", true), ("EXIT", false),
        ];
        let hint_w: i32 = seg.iter().map(|(t, _)| tr.width(t, Face::Regular, fsize, 0)).sum();
        let mut hx = cx - hint_w / 2;
        for (t, key) in seg {
            let colr = if key { dim_c } else { faint };
            hx = tr.draw(px, w, hx, fy, t, Face::Regular, fsize, colr, alpha, 0);
        }

        let vsize = l.px(12.0) as u32;
        let vw = tr.width(&c.version, Face::Regular, vsize, 0);
        let ver = c.version.clone();
        let _ = tr.draw(
            px,
            w,
            w as i32 - vw - l.px(18.0) as i32,
            (l.h - l.px(16.0)) as i32,
            &ver,
            Face::Regular,
            vsize,
            faint,
            alpha * 200 / 255,
            0,
        );
    }

    // 10. confirm modal overlay (scales + fades over a dimmed backdrop)
    if vis.confirm > 0.004 {
        dim(px, (112.0 * ce) as u32);
        let scale_m = 0.94 + 0.06 * ce;
        let (cx, cy) = ((w / 2) as i32, (l.h / 2) as i32);
        blit_glass_scaled(px, w, &a.modal, cx, cy, scale_m, (ce * 255.0) as u32);

        let ta = (stage(vis.confirm, 0.45, 0.55) * 255.0) as u32;
        if ta > 0 {
            let my = (l.h - a.modal.rect_h) / 2;
            tr.draw_centered(
                px,
                w,
                cx,
                (my + l.px(52.0)) as i32,
                "READY TO BOOT",
                Face::Regular,
                l.px(13.0) as u32,
                rgb_t(CYAN),
                ta * 220 / 255,
                l.px(7.0) as i32,
            );
            let sel = items.get(vis.selected);
            let name = sel.map(|e| e.label.as_str()).unwrap_or("");
            let name_size = l.px(19.0) as u32;
            let shown = fit_text(tr, name, Face::Medium, name_size, (a.modal.rect_w - l.px(48.0)) as i32);
            tr.draw_centered(px, w, cx, (my + l.px(104.0)) as i32, &shown, Face::Medium, name_size, rgb_t(TEXT), ta, 0);
            if let Some(ver) = sel.and_then(|e| e.version.as_deref()) {
                tr.draw_centered(px, w, cx, (my + l.px(126.0)) as i32, ver, Face::Regular, l.px(12.0) as u32, rgb_t(TEXT_FAINT), ta, l.px(2.0) as i32);
            }
            let dim_c = rgb_t(TEXT_DIM);
            let faint = rgb_t(TEXT_FAINT);
            let fsize = l.px(13.0) as u32;
            let hy = (my + l.px(152.0)) as i32;
            match vis.confirm_note {
                Some((note, is_err)) => {
                    let col = if is_err { rgb_t(AMBER) } else { rgb_t(CYAN) };
                    tr.draw_centered(px, w, cx, hy, note, Face::Medium, fsize, col, ta, l.px(2.0) as i32);
                }
                None => {
                    let seg = ["ENTER ", "BOOT      ", "ESC ", "BACK"];
                    let total: i32 = seg.iter().map(|t| tr.width(t, Face::Regular, fsize, 0)).sum();
                    let mut hx = cx - total / 2;
                    for (i, t) in seg.iter().enumerate() {
                        let colr = if i % 2 == 0 { dim_c } else { faint };
                        hx = tr.draw(px, w, hx, hy, t, Face::Regular, fsize, colr, ta, 0);
                    }
                }
            }
        }
    }

    // 11. edit overlay (in-menu config editor)
    if let Some(ed) = &vis.edit {
        if ee > 0.004 {
            compose_edit(px, l, a, tr, ed, ee);
        }
    }
}

/// Draw the config-editor modal over a dimmed backdrop.
fn compose_edit(px: &mut [u32], l: &Layout, a: &FrameAssets, tr: &mut TextRenderer, ed: &EditView, ease: f32) {
    let w = l.w;
    dim(px, (120.0 * ease) as u32);
    let scale_m = 0.94 + 0.06 * ease;
    let (cx, cy) = ((w / 2) as i32, (l.h / 2) as i32);
    blit_glass_scaled(px, w, &a.edit_modal, cx, cy, scale_m, (ease * 255.0) as u32);

    let ta = (stage(ease, 0.4, 0.6) * 255.0) as u32;
    if ta == 0 {
        return;
    }
    let mx = (w - a.edit_modal.rect_w) / 2;
    let my = (l.h - a.edit_modal.rect_h) / 2;
    let left = (mx + l.px(40.0)) as i32;
    let val_x = (mx + l.px(150.0)) as i32;
    let right = (mx + a.edit_modal.rect_w - l.px(40.0)) as i32;

    tr.draw(px, w, left, (my + l.px(46.0)) as i32, "EDIT ENTRY", Face::Medium, l.px(13.0) as u32, rgb_t(CYAN), ta * 220 / 255, l.px(6.0) as i32);
    // read-only filename reference
    let file = fit_text(tr, ed.iso, Face::Regular, l.px(12.0) as u32, right - left);
    tr.draw(px, w, left, (my + l.px(74.0)) as i32, &file, Face::Regular, l.px(12.0) as u32, rgb_t(TEXT_FAINT), ta, 0);

    let rows = [("NAME", ed.name), ("VERSION", ed.version), ("POSITION", ed.position)];
    let field_font = l.px(16.0) as u32;
    let label_font = l.px(12.0) as u32;
    for (i, (label, value)) in rows.iter().enumerate() {
        let ry = (my + l.px(118.0)) as i32 + (i * l.px(44.0)) as i32;
        let active = i == ed.field;
        // active-field marker + underline
        let lab_col = if active { rgb_t(CYAN) } else { rgb_t(TEXT_FAINT) };
        tr.draw(px, w, left, ry - l.px(16.0) as i32, label, Face::Regular, label_font, lab_col, ta, l.px(1.0) as i32);
        // underline
        let uy = ry + l.px(6.0) as usize as i32;
        let ucol = if active { CYAN } else { TEXT_FAINT };
        fill_hline(px, w, val_x, right, uy, rgb_t(ucol), if active { ta } else { ta / 3 });

        let vcol = if active { rgb_t(TEXT) } else { rgb_t(TEXT_DIM) };
        let shown = fit_text(tr, value, Face::Regular, field_font, right - val_x - l.px(14.0) as i32);
        let end_x = tr.draw(px, w, val_x, ry, &shown, Face::Regular, field_font, vcol, ta, 0);
        if active && ed.cursor {
            fill_hline(px, w, end_x + 2, end_x + l.px(10.0) as i32, ry - l.px(1.0) as i32, rgb_t(CYAN), ta);
        }
    }

    // footer / status
    let hy = (my + a.edit_modal.rect_h - l.px(30.0)) as i32;
    let fsize = l.px(12.0) as u32;
    if let Some(note) = ed.note {
        tr.draw_centered(px, w, cx, hy, note, Face::Medium, fsize, rgb_t(AMBER), ta, l.px(1.0) as i32);
    } else {
        let seg = ["\u{2191}\u{2193} ", "FIELD     ", "ENTER ", "SAVE     ", "ESC ", "CANCEL"];
        let total: i32 = seg.iter().map(|t| tr.width(t, Face::Regular, fsize, 0)).sum();
        let mut hx = cx - total / 2;
        for (i, t) in seg.iter().enumerate() {
            let colr = if i % 2 == 0 { rgb_t(TEXT_DIM) } else { rgb_t(TEXT_FAINT) };
            hx = tr.draw(px, w, hx, hy, t, Face::Regular, fsize, colr, ta, 0);
        }
    }
}

/// Draw a 1px horizontal line from x0..x1 at y.
fn fill_hline(buf: &mut [u32], stride: usize, x0: i32, x1: i32, y: i32, color: u32, alpha: u32) {
    if y < 0 || y as usize >= buf.len() / stride {
        return;
    }
    let row = y as usize * stride;
    for x in x0.max(0)..x1.min(stride as i32) {
        let i = row + x as usize;
        buf[i] = over(buf[i], color, alpha);
    }
}
