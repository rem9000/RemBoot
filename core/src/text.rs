//! Text rendering: fontdue rasterization + a per-(face,size,char) glyph
//! cache, blended onto the u32 pixel buffer.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use fontdue::{Font, FontSettings};

use crate::pix::over;

/// Embedded JetBrains Mono (OFL licensed, see assets/fonts/OFL.txt).
pub mod embedded {
    pub static REGULAR: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");
    pub static MEDIUM: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Medium.ttf");
    pub static BOLD: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Bold.ttf");
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Face {
    Regular = 0,
    Medium = 1,
    Bold = 2,
}

struct Glyph {
    w: usize,
    h: usize,
    xmin: i32,
    ymin: i32,
    advance: f32,
    cov: Vec<u8>,
}

pub struct TextRenderer {
    fonts: [Font; 3],
    cache: BTreeMap<(u8, u32, char), Glyph>,
}

fn load(data: &'static [u8]) -> Font {
    Font::from_bytes(data, FontSettings::default()).expect("font parse")
}

impl TextRenderer {
    pub fn new() -> Self {
        Self {
            fonts: [load(embedded::REGULAR), load(embedded::MEDIUM), load(embedded::BOLD)],
            cache: BTreeMap::new(),
        }
    }

    fn glyph(&mut self, face: Face, size: u32, ch: char) -> &Glyph {
        let key = (face as u8, size, ch);
        if !self.cache.contains_key(&key) {
            let (m, cov) = self.fonts[face as usize].rasterize(ch, size as f32);
            self.cache.insert(
                key,
                Glyph { w: m.width, h: m.height, xmin: m.xmin, ymin: m.ymin, advance: m.advance_width, cov },
            );
        }
        self.cache.get(&key).unwrap()
    }

    /// Draw `text`; returns the pen x position after the last glyph.
    /// `tracking` is extra spacing (px) added after every glyph.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        buf: &mut [u32],
        stride: usize,
        x: i32,
        baseline: i32,
        text: &str,
        face: Face,
        size: u32,
        color: u32,
        alpha: u32,
        tracking: i32,
    ) -> i32 {
        let height = (buf.len() / stride) as i32;
        let mut pen = x as f32;
        for ch in text.chars() {
            let g = self.glyph(face, size, ch);
            let gx = pen as i32 + g.xmin;
            let gy = baseline - (g.h as i32 + g.ymin);
            for row in 0..g.h {
                let py = gy + row as i32;
                if py < 0 || py >= height {
                    continue;
                }
                let brow = py as usize * stride;
                let crow = row * g.w;
                for col in 0..g.w {
                    let px = gx + col as i32;
                    if px < 0 || px >= stride as i32 {
                        continue;
                    }
                    let cov = g.cov[crow + col] as u32;
                    if cov == 0 {
                        continue;
                    }
                    let i = brow + px as usize;
                    buf[i] = over(buf[i], color, cov * alpha / 255);
                }
            }
            pen += g.advance + tracking as f32;
        }
        pen as i32
    }

    /// Width in px of `text` (advance sum incl. tracking between glyphs).
    pub fn width(&mut self, text: &str, face: Face, size: u32, tracking: i32) -> i32 {
        let mut wsum = 0.0f32;
        let mut n = 0;
        for ch in text.chars() {
            wsum += self.glyph(face, size, ch).advance;
            n += 1;
        }
        if n > 0 {
            wsum += (tracking * (n - 1)) as f32;
        }
        wsum as i32
    }

    /// Draw centered on `cx`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_centered(
        &mut self,
        buf: &mut [u32],
        stride: usize,
        cx: i32,
        baseline: i32,
        text: &str,
        face: Face,
        size: u32,
        color: u32,
        alpha: u32,
        tracking: i32,
    ) {
        let w = self.width(text, face, size, tracking);
        self.draw(buf, stride, cx - w / 2, baseline, text, face, size, color, alpha, tracking);
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}
