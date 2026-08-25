//! Design language of the WebUI2 / ventoy-dark family, as constants.

pub const CYAN: (u8, u8, u8) = (34, 211, 238);
pub const INDIGO: (u8, u8, u8) = (129, 140, 248);
pub const VIOLET: (u8, u8, u8) = (139, 92, 246);
pub const TEAL: (u8, u8, u8) = (14, 116, 144);
pub const AMBER: (u8, u8, u8) = (251, 191, 36);

pub const TEXT: (u8, u8, u8) = (219, 231, 255);
pub const TEXT_DIM: (u8, u8, u8) = (142, 163, 200);
pub const TEXT_FAINT: (u8, u8, u8) = (91, 108, 143);
/// Text on top of the bright selection bar.
pub const TEXT_ON_BAR: (u8, u8, u8) = (6, 20, 34);
pub const TEXT_ON_BAR_DIM: (u8, u8, u8) = (23, 58, 84);

/// Base background gradient, top and bottom.
pub const BASE_TOP: (u8, u8, u8) = (5, 8, 14);
pub const BASE_BOTTOM: (u8, u8, u8) = (3, 5, 10);

/// Glass panel fill: rgb + alpha (0-255).
pub const PANEL_FILL: (u8, u8, u8, u8) = (10, 16, 28, 168);
pub const PANEL_BORDER: (u8, u8, u8) = (125, 180, 255);
pub const PANEL_BORDER_ALPHA: u8 = 60;
