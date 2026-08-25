//! Precomputed scene: aurora background + brand letter-R glyph.
//!
//! Direct port of `gen_theme.py` from the ventoy-dark GRUB theme, with two
//! changes: the radial gaussians are computed separably (per-axis factor
//! tables) so the whole background is O(W*H) multiplies instead of exp()
//! calls, and the glyph glow is kept as a separate additive sprite so the
//! frame loop can pulse it. The glyph is drawn from a single pen path so the
//! intro can reveal it stroke-by-stroke.

use alloc::vec;
use alloc::vec::Vec;
use libm::{cosf, expf, sinf, sqrtf};

use crate::theme::*;

/// Additive sprite (premultiplied glow intensity per channel).
pub struct Sprite {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
    pub rgb: Vec<u32>,
}

/// Sprite with straight color + coverage alpha + a per-pixel stroke
/// parameter `t` (0..=255 along the pen path) driving the reveal animation.
pub struct GlyphSprite {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
    pub rgb: Vec<u32>,
    pub a: Vec<u8>,
    pub t: Vec<u8>,
}

pub struct Scene {
    /// Aurora + grid + vignette only; everything else is composed per frame.
    pub bg: Vec<u32>,
    pub glyph: GlyphSprite,
    pub glyph_glow: GlyphSprite,
    /// Baseline y of the glyph (bottom point) — layout anchor for the wordmark.
    pub glyph_bottom: usize,
}

/// Uniform scale factor relative to the 1024x768 reference design.
pub fn scale(w: usize, h: usize) -> f32 {
    let sw = w as f32 / 1024.0;
    let sh = h as f32 / 768.0;
    if sw < sh { sw } else { sh }
}

pub fn build(w: usize, h: usize) -> Scene {
    let bg = aurora(w, h);
    let (glyph, glyph_glow, glyph_bottom) = glyph_r(w, h);
    Scene { bg, glyph, glyph_glow, glyph_bottom }
}

fn lerpf(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

struct Blob {
    cx: f32,
    cy: f32,
    sigma: f32,
    col: (u8, u8, u8),
    k: f32,
}

fn aurora(w: usize, h: usize) -> Vec<u32> {
    let wf = w as f32;
    let hf = h as f32;
    let s = scale(w, h);
    let blobs = [
        Blob { cx: 0.14 * wf, cy: 0.16 * hf, sigma: 300.0 * s, col: CYAN, k: 0.16 },
        Blob { cx: 0.88 * wf, cy: 0.82 * hf, sigma: 340.0 * s, col: INDIGO, k: 0.15 },
        Blob { cx: 0.62 * wf, cy: 0.10 * hf, sigma: 240.0 * s, col: VIOLET, k: 0.08 },
        Blob { cx: 0.35 * wf, cy: 0.95 * hf, sigma: 300.0 * s, col: TEAL, k: 0.12 },
    ];

    // Separable gaussian factor tables: exp(-(d2x+d2y)/2s^2) = fx[x]*fy[y].
    let mut fxs: Vec<Vec<f32>> = Vec::new();
    let mut fys: Vec<Vec<f32>> = Vec::new();
    for b in &blobs {
        let inv = 1.0 / (2.0 * b.sigma * b.sigma);
        let fx: Vec<f32> = (0..w)
            .map(|x| {
                let dx = x as f32 - b.cx;
                b.k * expf(-dx * dx * inv)
            })
            .collect();
        let fy: Vec<f32> = (0..h)
            .map(|y| {
                let dy = y as f32 - b.cy;
                expf(-dy * dy * inv)
            })
            .collect();
        fxs.push(fx);
        fys.push(fy);
    }

    let mut px = vec![0u32; w * h];
    for y in 0..h {
        let ty = y as f32 / hf;
        let base_r = lerpf(BASE_TOP.0 as f32, BASE_BOTTOM.0 as f32, ty);
        let base_g = lerpf(BASE_TOP.1 as f32, BASE_BOTTOM.1 as f32, ty);
        let base_b = lerpf(BASE_TOP.2 as f32, BASE_BOTTOM.2 as f32, ty);
        let gy = y % 44 == 0;
        let ey = (y.min(h - y)) as f32 / (hf * 0.5);
        let vy = (y as f32 - hf / 2.0) / (hf / 2.0);
        let row = y * w;
        for x in 0..w {
            let mut r = base_r;
            let mut g = base_g;
            let mut b = base_b;
            for i in 0..blobs.len() {
                let f = fxs[i][x] * fys[i][y];
                r += blobs[i].col.0 as f32 * f;
                g += blobs[i].col.1 as f32 * f;
                b += blobs[i].col.2 as f32 * f;
            }
            // subtle grid, fading towards the edges
            if gy || x % 44 == 0 {
                let ex = (x.min(w - x)) as f32 / (wf * 0.5);
                let gk = 10.0 * ex * ey;
                r += gk;
                g += gk * 1.4;
                b += gk * 2.0;
            }
            // vignette
            let vx = (x as f32 - wf / 2.0) / (wf / 2.0);
            let v = 1.0 - 0.32 * (vx * vx + vy * vy);
            let pr = (r * v).clamp(0.0, 255.0) as u32;
            let pg = (g * v).clamp(0.0, 255.0) as u32;
            let pb = (b * v).clamp(0.0, 255.0) as u32;
            px[row + x] = (pr << 16) | (pg << 8) | pb;
        }
    }
    px
}

/// Nearest point on a polyline: returns (distance, pen-parameter 0..=1 along
/// the whole path by arc length).
fn nearest_on_path(px: f32, py: f32, pts: &[(f32, f32)], cum: &[f32], total: f32) -> (f32, f32) {
    let mut best_d = f32::MAX;
    let mut best_pen = 0.0;
    for i in 1..pts.len() {
        let (ax, ay) = pts[i - 1];
        let (bx, by) = pts[i];
        let (vx, vy) = (bx - ax, by - ay);
        let seg2 = vx * vx + vy * vy;
        let t = if seg2 > 0.0 {
            (((px - ax) * vx + (py - ay) * vy) / seg2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (dx, dy) = (px - (ax + vx * t), py - (ay + vy * t));
        let d = sqrtf(dx * dx + dy * dy);
        if d < best_d {
            best_d = d;
            best_pen = (cum[i - 1] + t * sqrtf(seg2)) / total;
        }
    }
    (best_d, best_pen)
}

/// Build the brand letter **R** as two sprites (core + glow), both carrying a
/// per-pixel pen-path parameter so the reveal animation "draws" the glyph in
/// one continuous stroke: up the stem, around the bowl, down the leg.
fn glyph_r(w: usize, h: usize) -> (GlyphSprite, GlyphSprite, usize) {
    let s = scale(w, h);
    let cx = w as f32 / 2.0;
    let top = 46.0 * s;
    let btm = 172.0 * s;
    let stroke = 7.5 * s;
    let glow_reach = 34.0 * s;
    let glow_sigma = 13.0 * s;

    // Letter geometry (design units * s).
    let bowl_w = 62.0 * s;
    let stem_x = cx - 34.0 * s;
    let waist = top + 64.0 * s;
    let vrad = (waist - top) / 2.0;
    let midb = (top + waist) / 2.0;
    let leg_foot_x = stem_x + 60.0 * s;
    // gradient spans the ink horizontally: cyan (stem) -> indigo (bowl/leg)
    let grad_lo = stem_x;
    let grad_hi = stem_x + bowl_w;

    // Pen path: stem bottom -> top, bowl (clockwise -90deg..+90deg), leg -> foot.
    let mut pts: Vec<(f32, f32)> = Vec::new();
    pts.push((stem_x, btm));
    pts.push((stem_x, top));
    let steps = 24;
    for i in 1..=steps {
        let th = -core::f32::consts::FRAC_PI_2
            + core::f32::consts::PI * (i as f32 / steps as f32);
        pts.push((stem_x + bowl_w * cosf(th), midb + vrad * sinf(th)));
    }
    pts.push((leg_foot_x, btm));

    let mut cum = vec![0f32; pts.len()];
    for i in 1..pts.len() {
        let (dx, dy) = (pts[i].0 - pts[i - 1].0, pts[i].1 - pts[i - 1].1);
        cum[i] = cum[i - 1] + sqrtf(dx * dx + dy * dy);
    }
    let total = cum[pts.len() - 1].max(1.0);

    let x0 = ((stem_x - glow_reach - 6.0).max(0.0)) as usize;
    let x1 = (((stem_x + bowl_w + glow_reach + 6.0) as usize) + 1).min(w);
    let y0 = ((top - glow_reach - 6.0).max(0.0)) as usize;
    let y1 = (((btm + glow_reach + 6.0) as usize) + 1).min(h);
    let (sw, sh) = (x1 - x0, y1 - y0);
    let mut core = GlyphSprite {
        x: x0,
        y: y0,
        w: sw,
        h: sh,
        rgb: vec![0u32; sw * sh],
        a: vec![0u8; sw * sh],
        t: vec![0u8; sw * sh],
    };
    let mut glow = GlyphSprite {
        x: x0,
        y: y0,
        w: sw,
        h: sh,
        rgb: vec![0u32; sw * sh],
        a: vec![0u8; sw * sh],
        t: vec![0u8; sw * sh],
    };

    for y in y0..y1 {
        for x in x0..x1 {
            let (xf, yf) = (x as f32, y as f32);
            let (d, pen) = nearest_on_path(xf, yf, &pts, &cum, total);
            let t = ((xf - grad_lo) / (grad_hi - grad_lo)).clamp(0.0, 1.0);
            let r = lerpf(CYAN.0 as f32, INDIGO.0 as f32, t);
            let g = lerpf(CYAN.1 as f32, INDIGO.1 as f32, t);
            let b = lerpf(CYAN.2 as f32, INDIGO.2 as f32, t);
            let col = ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
            let i = (y - y0) * sw + (x - x0);
            let pen_b = (pen * 255.0) as u8;
            if d < glow_reach {
                let amt = 0.28 * expf(-(d * d) / (2.0 * glow_sigma * glow_sigma));
                glow.rgb[i] = col;
                glow.a[i] = (amt * 255.0).clamp(0.0, 255.0) as u8;
                glow.t[i] = pen_b;
            }
            let a = (stroke - d + 0.5).clamp(0.0, 1.0);
            if a > 0.0 {
                core.rgb[i] = col;
                core.a[i] = (a * 255.0) as u8;
                core.t[i] = pen_b;
            }
        }
    }
    (core, glow, btm as usize)
}
