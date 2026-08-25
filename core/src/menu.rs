//! Menu state machine + animation timing. Pure and host-testable.

use libm::sinf;

use crate::fx::ease_out_back;

/// Duration of the selection-bar slide, seconds.
pub const EASE_DURATION: f32 = 0.26;
/// Glow pulse cycle, seconds.
pub const PULSE_CYCLE: f32 = 1.1;

const TAU: f32 = core::f32::consts::TAU;

pub struct Menu {
    pub count: usize,
    pub selected: usize,
    /// Bar flight origin in item-index space (fractional mid-flight).
    from: f32,
    anim_t: f32,
    time: f32,
    prev_pos: f32,
    vel: f32,
    flash: f32,
}

impl Menu {
    pub fn new(count: usize) -> Self {
        Self {
            count,
            selected: 0,
            from: 0.0,
            anim_t: EASE_DURATION,
            time: 0.0,
            prev_pos: 0.0,
            vel: 0.0,
            flash: 0.0,
        }
    }

    pub fn move_up(&mut self) -> bool {
        self.select(self.selected.checked_sub(1).unwrap_or(0))
    }

    pub fn move_down(&mut self) -> bool {
        self.select((self.selected + 1).min(self.count.saturating_sub(1)))
    }

    /// Returns true if the selection changed (starts a bar animation from the
    /// bar's *current* interpolated position).
    pub fn select(&mut self, idx: usize) -> bool {
        if idx == self.selected || idx >= self.count {
            return false;
        }
        // Re-anchor at the bar's current interpolated position so a key
        // press mid-flight animates smoothly instead of jumping.
        self.from = self.bar_pos();
        self.selected = idx;
        self.anim_t = 0.0;
        self.kick();
        true
    }

    /// Trigger the glow flash (also used for ENTER feedback).
    pub fn kick(&mut self) {
        self.flash = 1.0;
    }

    pub fn tick(&mut self, dt: f32) {
        self.time += dt;
        if self.anim_t < EASE_DURATION {
            self.anim_t = (self.anim_t + dt).min(EASE_DURATION);
        }
        let pos = self.bar_pos();
        self.vel = if dt > 0.0 { (pos - self.prev_pos) / dt } else { 0.0 };
        self.prev_pos = pos;
        self.flash = (self.flash - dt * 3.2).max(0.0);
    }

    /// Eased 0..=1(+overshoot) progress of the bar flight.
    pub fn progress(&self) -> f32 {
        ease_out_back(self.anim_t / EASE_DURATION)
    }

    /// Interpolated bar position in *item index space* (e.g. 1.4 = between
    /// item 1 and 2; briefly overshoots the target thanks to the back ease).
    pub fn bar_pos(&self) -> f32 {
        let p = self.progress();
        self.from + (self.selected as f32 - self.from) * p
    }

    /// Bar velocity in item-index units per second (drives squash/stretch).
    pub fn velocity(&self) -> f32 {
        self.vel
    }

    pub fn settled(&self) -> bool {
        self.anim_t >= EASE_DURATION
    }

    /// Glyph glow pulse alpha (0..=255), ~1.1 s cycle + key flash.
    pub fn glyph_pulse(&self) -> u32 {
        let base = 205.0 + 40.0 * sinf(TAU * self.time / PULSE_CYCLE) + self.flash * 22.0;
        (base as u32).min(255)
    }

    /// Selection-bar glow pulse alpha (0..=255), same cycle, offset phase,
    /// boosted by the key flash.
    pub fn bar_pulse(&self) -> u32 {
        let base = 172.0 + 55.0 * sinf(TAU * self.time / PULSE_CYCLE + 0.9) + self.flash * 70.0;
        (base as u32).min(255)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_at_ends() {
        let mut m = Menu::new(3);
        assert!(!m.move_up());
        assert_eq!(m.selected, 0);
        assert!(m.move_down());
        assert!(m.move_down());
        assert!(!m.move_down());
        assert_eq!(m.selected, 2);
    }

    #[test]
    fn bar_animates_toward_target() {
        let mut m = Menu::new(5);
        m.move_down();
        assert_eq!(m.bar_pos(), 0.0);
        m.tick(0.08);
        let mid = m.bar_pos();
        assert!(mid > 0.0, "should have left the origin, got {mid}");
        m.tick(0.30);
        assert!(m.settled());
        assert!((m.bar_pos() - 1.0).abs() < 1e-4, "should settle at 1.0");
    }

    #[test]
    fn overshoot_is_bounded() {
        let mut m = Menu::new(5);
        m.move_down();
        let mut max = 0.0f32;
        for _ in 0..40 {
            m.tick(0.016);
            max = max.max(m.bar_pos());
        }
        assert!(max < 1.25, "overshoot too large: {max}");
    }

    #[test]
    fn velocity_settles_to_zero() {
        let mut m = Menu::new(5);
        m.move_down();
        m.tick(0.05);
        assert!(m.velocity().abs() > 0.1);
        for _ in 0..30 {
            m.tick(0.033);
        }
        assert!(m.velocity().abs() < 1e-3);
    }

    #[test]
    fn pulses_stay_in_alpha_range() {
        let mut m = Menu::new(2);
        m.move_down();
        for _ in 0..200 {
            m.tick(0.033);
            assert!(m.glyph_pulse() <= 255);
            assert!(m.bar_pulse() <= 255);
        }
    }
}
