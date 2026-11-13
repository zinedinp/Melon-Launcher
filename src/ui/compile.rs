//! Overlay while the launcher extracts zips and runs rustc on source mods

use super::modal::{self, BODY_COLOR, PADDING, TITLE_COLOR};
use melon::gui::text::font::LINE_HEIGHT;
use melon::gui::widgets::button::Button;
use melon::gui::widgets::{Rect, Ui};

const PANEL_W: f32 = 320.0;
const BUTTON_W: f32 = 100.0;
const BUTTON_H: f32 = 20.0;
const GAP: f32 = 6.0;
const LOG_COLOR: [f32; 4] = [0.7, 0.7, 0.75, 1.0];
const KEPT: usize = 10;

pub struct CompileOverlay {
    title: &'static str,
    working: &'static str,
    failed: &'static str,
    lines: Vec<String>,
    error: Option<String>,
    close: Button,
}

impl CompileOverlay {
    pub fn new() -> Self {
        Self::named(
            "melon.launcher.compile.title",
            "melon.launcher.compile.working",
            "melon.launcher.compile.failed",
        )
    }

    pub fn download() -> Self {
        Self::named(
            "melon.launcher.download.title",
            "melon.launcher.download.working",
            "melon.launcher.download.failed",
        )
    }

    fn named(title: &'static str, working: &'static str, failed: &'static str) -> Self {
        Self {
            title,
            working,
            failed,
            lines: Vec::new(),
            error: None,
            close: Button::sized("melon.launcher.compile.close", BUTTON_W, BUTTON_H),
        }
    }

    pub fn push(&mut self, id: &str, message: &str) {
        self.lines.push(format!("{id}: {message}"));
        if self.lines.len() > KEPT {
            self.lines.remove(0);
        }
    }

    pub fn fail(&mut self, error: String) {
        self.error = Some(error);
    }

    pub fn failed(&self) -> bool {
        self.error.is_some()
    }

    pub fn hover(&mut self, x: f32, y: f32, size: (f32, f32)) {
        self.layout_close(size);
        self.close.update_hover(x, y);
    }

    pub fn click(&mut self, x: f32, y: f32, size: (f32, f32)) -> bool {
        self.layout_close(size);
        self.failed() && self.close.clicked(x, y)
    }

    pub fn over_button(&self) -> bool {
        self.failed() && self.close.enabled && self.close.hovered
    }

    fn panel(&self, size: (f32, f32)) -> Rect {
        let log_h = KEPT as f32 * LINE_HEIGHT;
        let extra = if self.failed() {
            BUTTON_H + GAP * 2.0 + LINE_HEIGHT * 2.0
        } else {
            LINE_HEIGHT
        };
        let panel_h = PADDING * 2.0 + LINE_HEIGHT + GAP + log_h + extra;
        Rect::new(
            ((size.0 - PANEL_W) / 2.0).round(),
            ((size.1 - panel_h) / 2.0).round(),
            PANEL_W,
            panel_h,
        )
    }

    fn layout_close(&mut self, size: (f32, f32)) {
        let panel = self.panel(size);
        self.close.x = panel.x + (panel.w - BUTTON_W) / 2.0;
        self.close.y = panel.bottom() - PADDING - BUTTON_H;
    }

    pub fn draw(&mut self, ui: &mut Ui) {
        let panel = self.panel((ui.width, ui.height));
        modal::draw_panel(ui, panel);
        let x = panel.x + PADDING;
        let mut y = panel.y + PADDING;
        let title = ui.tr(self.title);
        ui.text(title, x, y, TITLE_COLOR);
        y += LINE_HEIGHT + GAP;
        for line in &self.lines {
            ui.text(line, x, y, LOG_COLOR);
            y += LINE_HEIGHT;
        }
        y = panel.y + PADDING + LINE_HEIGHT + GAP + KEPT as f32 * LINE_HEIGHT;
        if let Some(error) = &self.error {
            ui.text(ui.tr(self.failed), x, y, TITLE_COLOR);
            y += LINE_HEIGHT + 2.0;
            let inner_w = PANEL_W - PADDING * 2.0;
            for wrapped in ui.font().wrap(error, inner_w).into_iter().take(2) {
                ui.text(&wrapped, x, y, BODY_COLOR);
                y += LINE_HEIGHT;
            }
            self.layout_close((ui.width, ui.height));
            self.close.draw(ui);
        } else {
            ui.text(ui.tr(self.working), x, y, BODY_COLOR);
        }
    }
}

impl Default for CompileOverlay {
    fn default() -> Self {
        Self::new()
    }
}
