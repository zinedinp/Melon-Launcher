//! The popups over the window: work in progress, and a game that ended with an error.

use super::kit::theme;
use super::kit::{BUTTON_H, Btn, Cx, GAP, LINE, Rect};
use std::path::PathBuf;

const PANEL_W: f32 = 340.0;
const PAD: f32 = 10.0;
/// Log lines a progress popup keeps.
const KEPT: usize = 10;
/// How many of the game's last lines a crash popup quotes.
pub const TAIL_LINES: usize = 12;

/// A panel centred in the window, over a dimmed background.
fn panel(cx: &mut Cx, height: f32) -> Rect {
    cx.shade();
    let width = PANEL_W.min(cx.width() - 16.0);
    let rect = Rect::new(
        ((cx.width() - width) / 2.0).round(),
        ((cx.height() - height) / 2.0).round(),
        width,
        height,
    );
    cx.frame(rect, theme::PANEL, theme::FAINT);
    rect
}

/// Shown while something is downloaded or prepared: its last lines, a bar, and the error.
pub struct Progress {
    title: String,
    working: String,
    failed: String,
    lines: Vec<String>,
    /// Bytes done and in all, while a download reports them.
    bytes: Option<(u64, u64)>,
    error: Option<String>,
}

impl Progress {
    pub fn new(title: &str, working: &str, failed: &str) -> Self {
        Self {
            title: title.to_owned(),
            working: working.to_owned(),
            failed: failed.to_owned(),
            lines: Vec::new(),
            bytes: None,
            error: None,
        }
    }

    pub fn push(&mut self, line: &str) {
        self.lines.push(line.to_owned());
        if self.lines.len() > KEPT {
            self.lines.remove(0);
        }
    }

    pub fn set_bytes(&mut self, bytes: (u64, u64)) {
        self.bytes = Some(bytes);
    }

    pub fn fail(&mut self, error: String) {
        self.error = Some(error);
    }

    pub fn failed(&self) -> bool {
        self.error.is_some()
    }

    /// Draws the popup. True when its Close button was pressed.
    pub fn frame(&self, cx: &mut Cx) -> bool {
        let log_h = KEPT as f32 * LINE;
        let foot = if self.failed() { 3.0 * LINE + GAP + BUTTON_H } else { LINE + 10.0 };
        let rect = panel(cx, PAD * 2.0 + LINE + GAP + log_h + GAP + foot);
        let (x, w) = (rect.x + PAD, rect.w - PAD * 2.0);
        let mut y = rect.y + PAD;
        cx.text(&self.title, x, y, theme::TEXT);
        y += LINE + GAP;
        for (i, line) in self.lines.iter().enumerate() {
            cx.small_label(line, x, y + i as f32 * LINE, w, theme::DIM);
        }
        y += log_h + GAP;
        let Some(error) = &self.error else {
            cx.small_label(&self.working, x, y, w, theme::DIM);
            let bar = Rect::new(x, y + LINE, w, 5.0);
            cx.fill(bar, theme::FIELD);
            if let Some((done, total)) = self.bytes {
                let fraction = (done as f32 / total.max(1) as f32).clamp(0.0, 1.0);
                cx.fill(Rect::new(bar.x, bar.y, (bar.w * fraction).round(), bar.h), theme::ACCENT_FILL);
            }
            return false;
        };
        cx.text(&self.failed, x, y, theme::DANGER);
        cx.paragraph(error, x, y + LINE + 2.0, w, theme::TEXT);
        let close = Rect::new(rect.right() - PAD - 90.0, rect.bottom() - PAD - BUTTON_H, 90.0, BUTTON_H);
        cx.button(close, cx.t("common.close"), Btn::NORMAL)
    }
}

/// How the game ended.
pub struct CrashReport {
    /// The exit code, or what stands for none (a signal).
    pub code: String,
    /// Text key explaining the code, when it is a known load failure.
    pub hint: Option<&'static str>,
    /// The last lines it wrote.
    pub lines: Vec<String>,
    pub log: PathBuf,
}

/// What the crash popup was asked to do.
pub enum Choice {
    None,
    Close,
    OpenLog,
}

impl CrashReport {
    pub fn frame(&self, cx: &mut Cx) -> Choice {
        let width = PANEL_W.min(cx.width() - 16.0) - PAD * 2.0;
        let hint = self.hint.map(|key| cx.t(key).to_owned());
        let lines: Vec<String> = hint
            .iter()
            .chain(&self.lines)
            .flat_map(|line| cx.ui.wrap(line, width / super::kit::SMALL))
            .collect();
        let lines = &lines[lines.len().saturating_sub(TAIL_LINES + 4)..];
        let step = LINE * super::kit::SMALL + 1.0;
        let rect = panel(cx, PAD * 2.0 + LINE + GAP + lines.len() as f32 * step + GAP + BUTTON_H);
        let x = rect.x + PAD;
        let title = cx.tf("crash.title", &[&self.code]);
        cx.label(&title, x, rect.y + PAD, width, theme::DANGER);
        let top = rect.y + PAD + LINE + GAP;
        for (i, line) in lines.iter().enumerate() {
            let color = if i + 1 == lines.len() { theme::TEXT } else { theme::DIM };
            cx.small(line, x, top + i as f32 * step, color);
        }
        let y = rect.bottom() - PAD - BUTTON_H;
        let ok = Rect::new(rect.right() - PAD - 80.0, y, 80.0, BUTTON_H);
        let open = Rect::new(ok.x - GAP - 100.0, y, 100.0, BUTTON_H);
        if cx.button(open, cx.t("crash.open_log"), Btn::NORMAL) {
            return Choice::OpenLog;
        }
        if cx.button(ok, cx.t("common.ok"), Btn::PRIMARY) {
            return Choice::Close;
        }
        Choice::None
    }
}
