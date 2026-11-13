//! What the launcher shows when the game ended with an error: the last

use super::modal::{self, BODY_COLOR, PADDING, TITLE_COLOR};
use melon::gui::text::font::LINE_HEIGHT;
use melon::gui::widgets::button::Button;
use melon::gui::widgets::{Rect, Ui};
use std::path::PathBuf;

/// How many of the log's last lines the popup quotes.
pub const TAIL_LINES: usize = 12;
const BUTTON_W: f32 = 100.0;
const BUTTON_H: f32 = 20.0;
const GAP: f32 = 6.0;
const LOG_COLOR: [f32; 4] = [0.7, 0.7, 0.75, 1.0];

/// How the game ended.
pub struct CrashReport {
    /// The exit code, or what stands for none (a signal).
    pub code: String,
    /// The last lines it wrote.
    pub lines: Vec<String>,
    pub log: PathBuf,
}

pub enum Choice {
    None,
    Close,
    OpenLog,
}

pub struct CrashModal {
    report: CrashReport,
    mouse: (f32, f32),
}

/// Where the popup and its two buttons are, for a window and a font.
struct Layout {
    panel: Rect,
    lines: Vec<String>,
    open_log: Rect,
    close: Rect,
}

impl CrashModal {
    pub fn new(report: CrashReport) -> Self {
        Self {
            report,
            mouse: (-1.0, -1.0),
        }
    }

    pub fn log(&self) -> &std::path::Path {
        &self.report.log
    }

    pub fn hover(&mut self, x: f32, y: f32) {
        self.mouse = (x, y);
    }

    fn layout(&self, width: f32, height: f32, wrap: impl Fn(&str, f32) -> Vec<String>) -> Layout {
        let panel_w = 320.0f32.min(width - 16.0).max(140.0);
        let inner_w = panel_w - PADDING * 2.0;
        let lines: Vec<String> = self
            .report
            .lines
            .iter()
            .flat_map(|line| wrap(line, inner_w))
            .collect();
        let lines = lines[lines.len().saturating_sub(TAIL_LINES + 4)..].to_vec();
        let panel_h = PADDING * 2.0
            + LINE_HEIGHT
            + GAP * 2.0
            + lines.len() as f32 * LINE_HEIGHT
            + GAP
            + BUTTON_H;
        let panel = Rect::new(
            ((width - panel_w) / 2.0).round(),
            ((height - panel_h) / 2.0).round(),
            panel_w,
            panel_h,
        );
        let y = panel.bottom() - PADDING - BUTTON_H;
        let pair_w = BUTTON_W * 2.0 + GAP;
        let x = panel.x + (panel_w - pair_w) / 2.0;
        Layout {
            panel,
            lines,
            open_log: Rect::new(x, y, BUTTON_W, BUTTON_H),
            close: Rect::new(x + BUTTON_W + GAP, y, BUTTON_W, BUTTON_H),
        }
    }

    pub fn click(
        &self,
        x: f32,
        y: f32,
        size: (f32, f32),
        wrap: impl Fn(&str, f32) -> Vec<String>,
    ) -> Choice {
        let layout = self.layout(size.0, size.1, wrap);
        if layout.open_log.contains(x, y) {
            Choice::OpenLog
        } else if layout.close.contains(x, y) {
            Choice::Close
        } else {
            Choice::None
        }
    }

    /// Whether the pointer is over a button, for the cursor.
    pub fn over_button(&self, size: (f32, f32), wrap: impl Fn(&str, f32) -> Vec<String>) -> bool {
        let layout = self.layout(size.0, size.1, wrap);
        let (x, y) = self.mouse;
        layout.open_log.contains(x, y) || layout.close.contains(x, y)
    }

    pub fn draw(&self, ui: &mut Ui) {
        let font = ui.font();
        let layout = self.layout(ui.width, ui.height, |text, width| font.wrap(text, width));
        let p = layout.panel;
        modal::draw_panel(ui, p);
        let title = ui.lang.format("melon.launcher.crash", &[&self.report.code]);
        ui.centered(&title, p.x + p.w / 2.0, p.y + PADDING, TITLE_COLOR);
        let top = p.y + PADDING + LINE_HEIGHT + GAP * 2.0;
        for (i, line) in layout.lines.iter().enumerate() {
            let color = if i + 1 == layout.lines.len() {
                BODY_COLOR
            } else {
                LOG_COLOR
            };
            ui.text(line, p.x + PADDING, top + i as f32 * LINE_HEIGHT, color);
        }
        for (rect, label) in [
            (layout.open_log, "melon.launcher.open_log"),
            (layout.close, "melon.login.ok"),
        ] {
            let mut button = Button::new(label, rect.x, rect.y, rect.w, rect.h);
            button.hovered = rect.contains(self.mouse.0, self.mouse.1);
            button.draw(ui);
        }
    }
}
