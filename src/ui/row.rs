//! One account in the launcher's list: its face (or an initial on a

use melon::auth::accounts::Account;
use melon::gui::text::font::LINE_HEIGHT;
use melon::gui::widgets::{Rect, Ui};
use melon::renderer::TextureId;

/// The row's height, for the list.
pub const HEIGHT: f32 = 32.0;
/// Three pixels of face per pixel of the 8x8 head.
pub const FACE: f32 = 24.0;
const NAME: [f32; 4] = [0.95, 0.95, 0.95, 1.0];
const DIM: [f32; 4] = [0.6, 0.6, 0.6, 1.0];
const GOOD: [f32; 4] = [0.5, 0.82, 0.55, 1.0];
const BAD: [f32; 4] = [0.92, 0.5, 0.45, 1.0];

/// What the launcher knows of a Microsoft account's saved session.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Session {
    /// Not checked yet (or an offline profile).
    Unknown,
    /// Being refreshed right now.
    Checking,
    Valid,
    Expired,
}

/// What a row shows besides the account itself.
pub struct Look {
    pub session: Session,
    pub locked: bool,
    pub face: Option<TextureId>,
}

pub fn draw(ui: &mut Ui, rect: Rect, account: &Account, look: &Look, text_right: f32) {
    let tile = Rect::new(rect.x + 4.0, rect.y + (rect.h - FACE) / 2.0, FACE, FACE);
    match look.face {
        Some(face) => ui.sprite(face, tile),
        None => draw_initial(ui, tile, account),
    }

    let text_x = tile.right() + 6.0;
    let max_w = (text_right - text_x).max(8.0);
    let top = rect.y + (rect.h - 2.0 * LINE_HEIGHT) / 2.0;
    let name = ui.ellipsize(account.username(), max_w);
    ui.text(&name, text_x, top, NAME);
    let (key, color) = if look.locked {
        ("melon.launcher.locked", BAD)
    } else {
        match (account.is_microsoft(), look.session) {
            (false, _) => ("melon.launcher.kind.offline", DIM),
            (true, Session::Checking) => ("melon.launcher.session.checking", DIM),
            (true, Session::Valid) => ("melon.launcher.session.valid", GOOD),
            (true, Session::Expired) => ("melon.launcher.session.expired", BAD),
            (true, Session::Unknown) => ("melon.launcher.kind.microsoft", DIM),
        }
    };
    let line = ui.ellipsize(ui.tr(key), max_w);
    ui.text(&line, text_x, top + LINE_HEIGHT + 1.0, color);
}

/// The initial on a muted tile: shown while the face loads, or if it can't.
fn draw_initial(ui: &mut Ui, tile: Rect, account: &Account) {
    ui.fill(tile, tile_color(&account.key()));
    let initial = account
        .username()
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    let center = tile.x + tile.w / 2.0;
    ui.centered(&initial, center, tile.y + (tile.h - 8.0) / 2.0, [1.0; 4]);
}

/// A muted colour per account, the same one every time.
fn tile_color(key: &str) -> [f32; 4] {
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
