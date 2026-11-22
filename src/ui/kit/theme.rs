//! The launcher's one palette: dark panels, green for what to press, red only for danger.

pub type Color = [f32; 4];

pub const BG: Color = [0.063, 0.071, 0.078, 1.0];
pub const BAR: Color = [0.086, 0.098, 0.106, 1.0];
pub const PANEL: Color = [0.098, 0.110, 0.122, 1.0];
pub const RAISED: Color = [0.137, 0.153, 0.169, 1.0];
pub const HOVER: Color = [0.180, 0.200, 0.220, 1.0];
pub const EDGE: Color = [0.196, 0.216, 0.235, 1.0];
pub const FIELD: Color = [0.059, 0.067, 0.075, 1.0];

pub const TEXT: Color = [0.93, 0.94, 0.95, 1.0];
pub const DIM: Color = [0.62, 0.65, 0.68, 1.0];
pub const FAINT: Color = [0.40, 0.43, 0.46, 1.0];

/// Melon green: the primary action, selection, "on".
pub const ACCENT: Color = [0.37, 0.82, 0.23, 1.0];
pub const ACCENT_FILL: Color = [0.118, 0.560, 0.270, 1.0];
pub const ACCENT_HOVER: Color = [0.160, 0.680, 0.330, 1.0];
pub const ACCENT_SOFT: Color = [0.37, 0.82, 0.23, 0.14];

/// Melon red: delete, errors.
pub const DANGER: Color = [1.0, 0.36, 0.44, 1.0];
pub const DANGER_HOVER: Color = [0.76, 0.18, 0.27, 1.0];

pub const WARN: Color = [0.96, 0.76, 0.36, 1.0];
/// Java instances, to tell them from Melon ones at a glance.
pub const JAVA: Color = [0.93, 0.56, 0.22, 1.0];
pub const JAVA_SOFT: Color = [0.93, 0.56, 0.22, 0.12];

pub const SHADE: Color = [0.0, 0.0, 0.0, 0.62];

/// A colour with another alpha.
pub fn alpha(color: Color, a: f32) -> Color {
    [color[0], color[1], color[2], a]
}

/// A muted colour per key, the same one every time.
pub fn tile_color(key: &str) -> Color {
    let hash = key.bytes().fold(2166136261u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(16777619)
    });
    let hue = (hash % 360) as f32 / 60.0;
    let (saturation, value) = (0.45, 0.6);
    let chroma = saturation * value;
    let x = chroma * (1.0 - (hue % 2.0 - 1.0).abs());
    let (r, g, b) = match hue as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = value - chroma;
    [r + m, g + m, b + m, 1.0]
}
