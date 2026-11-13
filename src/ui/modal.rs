//! The launcher's popups share one look: a dimmed backdrop over the whole

use melon::gui::widgets::{Rect, Ui};

pub const PADDING: f32 = 8.0;
pub const TITLE_COLOR: [f32; 4] = [1.0, 0.9, 0.4, 1.0];
pub const BODY_COLOR: [f32; 4] = [0.85, 0.85, 0.85, 1.0];

/// The backdrop and the panel at `panel`.
pub fn draw_panel(ui: &mut Ui, panel: Rect) {
    ui.fill(
        Rect::new(0.0, 0.0, ui.width, ui.height),
        [0.0, 0.0, 0.0, 0.6],
    );
    ui.fill(
        Rect::new(panel.x - 1.0, panel.y - 1.0, panel.w + 2.0, panel.h + 2.0),
        [0.4, 0.4, 0.46, 1.0],
    );
    let white = ui.assets.white;
    ui.draw_list.push_vgradient(
        white,
        panel.x,
        panel.y,
        panel.w,
        panel.h,
        [0.09, 0.09, 0.12, 0.98],
        [0.07, 0.07, 0.09, 0.98],
    );
}
