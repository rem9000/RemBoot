//! Animation toolkit: easing curves, intro staging, drifting particles and
//! the scanline light sweep. Pure and host-testable.

use alloc::vec::Vec;
use libm::{expf, fmodf, sinf};

use crate::pix::add_scaled;

const TAU: f32 = core::f32::consts::TAU;

// ---------------------------------------------------------------- easing --

pub fn ease_out_cubic(t: f32) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

/// Ease-out with a mild overshoot (springy landing).
pub fn ease_out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    const C1: f32 = 1.20;
    const C3: f32 = C1 + 1.0;
    let u = t - 1.0;
    1.0 + C3 * u * u * u + C1 * u * u
}

/// 0..=1 progress of a stage that starts at `start` and lasts `dur` seconds.
pub fn stage(t: f32, start: f32, dur: f32) -> f32 {
    ((t - start) / dur).clamp(0.0, 1.0)
}

// ------------------------------------------------------------- particles --

struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    fn f32(&mut self) -> f32 {
        (self.next() >> 8) as f32 / 16_777_216.0
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }
}

struct Particle {
    x: f32,
    y: f32,
    vy: f32,
    sway: f32,
    phase: f32,
    size: u8,
    col: u32,
    base_a: u32,
}

/// Slowly rising, softly swaying dust motes in the brand palette.
pub struct Particles {
    p: Vec<Particle>,
    w: f32,
    h: f32,
    t: f32,
    rng: Rng,
}

const PARTICLE_COLS: [(u8, u8, u8); 3] = [(34, 211, 238), (129, 140, 248), (185, 205, 255)];

impl Particles {
    pub fn new(w: usize, h: usize) -> Self {
        let mut rng = Rng(0x52454D42); // "REMB"
        let count = (w * h / 22_000).clamp(24, 72);
        let mut p = Vec::with_capacity(count);
        for i in 0..count {
            let col = PARTICLE_COLS[i % PARTICLE_COLS.len()];
            let size = match i % 10 {
                0 => 3u8,
                1..=3 => 2,
                _ => 1,
            };
            p.push(Particle {
                x: rng.range(0.0, w as f32),
                y: rng.range(0.0, h as f32),
                vy: rng.range(7.0, 24.0),
                sway: rng.range(2.0, 8.0),
                phase: rng.range(0.0, TAU),
                size,
                col: crate::pix::rgb_t(col),
                base_a: rng.range(22.0, 64.0) as u32,
            });
        }
        Self { p, w: w as f32, h: h as f32, t: 0.0, rng }
    }

    pub fn tick(&mut self, dt: f32) {
        self.t += dt;
        let (w, h) = (self.w, self.h);
        for pt in &mut self.p {
            pt.y -= pt.vy * dt;
            if pt.y < -6.0 {
                pt.y = h + 6.0;
                pt.x = self.rng.range(0.0, w);
            }
        }
    }

    /// Additively draw all particles. Inside `glass` (x, y, w, h) they are
    /// dimmed to read as floating behind the panel.
    pub fn draw(&self, buf: &mut [u32], stride: usize, glass: (usize, usize, usize, usize), alpha: u32) {
        if alpha == 0 {
            return;
        }
        let height = buf.len() / stride;
        for pt in &self.p {
            let x = pt.x + sinf(self.t * 0.45 + pt.phase) * pt.sway;
            let twinkle = 0.72 + 0.28 * sinf(self.t * 1.4 + pt.phase * 2.3);
            let mut a = (pt.base_a as f32 * twinkle) as u32 * alpha / 255;
            let (xi, yi) = (x as i32, pt.y as i32);
            if xi < 0 || yi < 0 {
                continue;
            }
            let (xu, yu) = (xi as usize, yi as usize);
            if xu >= glass.0 && xu < glass.0 + glass.2 && yu >= glass.1 && yu < glass.1 + glass.3 {
                a = a * 3 / 10;
            }
            if a == 0 {
                continue;
            }
            let mut plot = |px: i32, py: i32, aa: u32| {
                if px >= 0 && py >= 0 && (px as usize) < stride && (py as usize) < height {
                    let i = py as usize * stride + px as usize;
                    buf[i] = add_scaled(buf[i], pt.col, aa.min(255));
                }
            };
            match pt.size {
                1 => plot(xi, yi, a),
                2 => {
                    plot(xi, yi, a);
                    plot(xi + 1, yi, a * 3 / 4);
                    plot(xi, yi + 1, a * 3 / 4);
                    plot(xi + 1, yi + 1, a / 2);
                }
                _ => {
                    plot(xi, yi, a);
                    plot(xi - 1, yi, a / 2);
                    plot(xi + 1, yi, a / 2);
                    plot(xi, yi - 1, a / 2);
                    plot(xi, yi + 1, a / 2);
                }
            }
        }
    }
}

// ----------------------------------------------------------- light sweep --

const SWEEP_PERIOD: f32 = 9.0;
const SWEEP_ACTIVE: f32 = 3.4;
const SWEEP_COL: u32 = 0x3C96B9; // (60, 150, 185)

/// A soft horizontal light band that occasionally sweeps down the screen.
pub fn draw_sweep(buf: &mut [u32], stride: usize, h: usize, time: f32, alpha: u32) {
    if alpha == 0 {
        return;
    }
    let ph = fmodf(time, SWEEP_PERIOD);
    if ph >= SWEEP_ACTIVE {
        return;
    }
    let yc = -70.0 + (h as f32 + 140.0) * (ph / SWEEP_ACTIVE);
    let sigma = 20.0f32;
    let y0 = ((yc - 55.0).max(0.0)) as usize;
    let y1 = ((yc + 55.0).max(0.0) as usize).min(h);
    for y in y0..y1 {
        let dy = y as f32 - yc;
        let fall = expf(-(dy * dy) / (2.0 * sigma * sigma));
        let a = (11.0 * fall) as u32 * alpha / 255;
        if a == 0 {
            continue;
        }
        for p in buf[y * stride..(y + 1) * stride].iter_mut() {
            *p = add_scaled(*p, SWEEP_COL, a);
        }
    }
}

/// Specular glint factor (0..=1) for a wordmark letter at `x`, sweeping
/// through every ~7 s.
pub fn glint(time: f32, x: f32, center: f32, half_w: f32) -> f32 {
    const PERIOD: f32 = 7.0;
    const DUR: f32 = 0.9;
    let ph = fmodf(time, PERIOD);
    if ph >= DUR {
        return 0.0;
    }
    let span = half_w + 70.0;
    let gx = center - span + 2.0 * span * (ph / DUR);
    let d = x - gx;
    0.55 * expf(-(d * d) / (2.0 * 55.0 * 55.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_endpoints() {
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
        assert!((ease_out_back(0.0)).abs() < 1e-6);
        assert!((ease_out_back(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn back_overshoots_but_bounded() {
        let mut max = 0.0f32;
        for i in 0..=100 {
            let v = ease_out_back(i as f32 / 100.0);
            if v > max {
                max = v;
            }
        }
        assert!(max > 1.001, "should overshoot, got max {max}");
        assert!(max < 1.25, "overshoot too wild: {max}");
    }

    #[test]
    fn stage_clamps() {
        assert_eq!(stage(-1.0, 0.0, 1.0), 0.0);
        assert_eq!(stage(0.5, 0.0, 1.0), 0.5);
        assert_eq!(stage(9.0, 0.0, 1.0), 1.0);
    }

    #[test]
    fn particles_wrap_and_stay_deterministic() {
        let mut a = Particles::new(1280, 800);
        let mut b = Particles::new(1280, 800);
        for _ in 0..600 {
            a.tick(0.033);
            b.tick(0.033);
        }
        for (pa, pb) in a.p.iter().zip(b.p.iter()) {
            assert_eq!(pa.x.to_bits(), pb.x.to_bits());
            assert!(pa.y > -10.0 && pa.y < 810.0);
        }
    }

    #[test]
    fn glint_bounded() {
        for i in 0..200 {
            let g = glint(i as f32 * 0.05, 640.0, 640.0, 200.0);
            assert!((0.0..=0.6).contains(&g));
        }
    }
}
