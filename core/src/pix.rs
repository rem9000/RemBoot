//! Integer blend helpers for `0x00RRGGBB` u32 pixels.

#[inline]
pub const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

#[inline]
pub const fn rgb_t(c: (u8, u8, u8)) -> u32 {
    rgb(c.0, c.1, c.2)
}

/// Source-over blend: `dst*(1-a) + src*a`, `a` in 0..=255.
#[inline]
pub fn over(dst: u32, src: u32, a: u32) -> u32 {
    debug_assert!(a <= 255);
    let na = 255 - a;
    let r = (((dst >> 16) & 0xFF) * na + ((src >> 16) & 0xFF) * a + 127) / 255;
    let g = (((dst >> 8) & 0xFF) * na + ((src >> 8) & 0xFF) * a + 127) / 255;
    let b = ((dst & 0xFF) * na + (src & 0xFF) * a + 127) / 255;
    (r << 16) | (g << 8) | b
}

/// Saturating additive blend: `dst + src*a/255` (glow layers).
#[inline]
pub fn add_scaled(dst: u32, src: u32, a: u32) -> u32 {
    debug_assert!(a <= 255);
    let r = (((dst >> 16) & 0xFF) + (((src >> 16) & 0xFF) * a + 127) / 255).min(255);
    let g = (((dst >> 8) & 0xFF) + (((src >> 8) & 0xFF) * a + 127) / 255).min(255);
    let b = ((dst & 0xFF) + ((src & 0xFF) * a + 127) / 255).min(255);
    (r << 16) | (g << 8) | b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn over_endpoints() {
        assert_eq!(over(0x000000, 0xFFFFFF, 0), 0x000000);
        assert_eq!(over(0x000000, 0xFFFFFF, 255), 0xFFFFFF);
        assert_eq!(over(0x22d3ee, 0x818cf8, 0), 0x22d3ee);
    }

    #[test]
    fn add_saturates() {
        assert_eq!(add_scaled(0xF0F0F0, 0xFFFFFF, 255), 0xFFFFFF);
        assert_eq!(add_scaled(0x101010, 0x202020, 255), 0x303030);
        assert_eq!(add_scaled(0x101010, 0x202020, 0), 0x101010);
    }
}
