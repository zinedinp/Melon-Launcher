//! Whether Rust is there for the selected instance's source-code mods, and the popup that
//! walks through installing it.

use super::kit::theme::{self, Color};
use super::kit::{BUTTON_H, Btn, Cx, GAP, LINE, Rect, SMALL};
use super::shell::Shell;
use crate::toolchain::{self, Rust};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const PANEL_W: f32 = 300.0;
const PAD: f32 = 10.0;
const COMMAND_H: f32 = 16.0;
/// How long a Copy button says "Copied".
const COPIED: Duration = Duration::from_secs(2);

/// One part of the popup's text.
enum Part {
    Text(String, Color),
    /// A terminal line, with a Copy button.
    Command(&'static str),
}

/// The looked-for toolchain, and the look in progress.
#[derive(Default)]
pub struct RustCheck {
    found: Option<Rust>,
    looking: Option<mpsc::Receiver<Rust>>,
}

impl RustCheck {
    /// What was found; `None` while looking. The first ask starts a look.
    pub fn get(&mut self) -> Option<&Rust> {
        if let Some(found) = self.looking.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.found = Some(found);
            self.looking = None;
        }
        if self.found.is_none() && self.looking.is_none() {
            self.look_again();
        }
        self.found.as_ref()
    }

    /// Looks again, off the window's thread: `rustc` is started to ask its version.
    pub fn look_again(&mut self) {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(toolchain::detect());
        });
        self.found = None;
        self.looking = Some(rx);
    }
}

impl Shell {
    /// The selected instance has source-code mods switched on, and nothing builds them.
    pub fn rust_missing(&mut self) -> bool {
        self.content.source_mods() > 0
            && matches!(self.rust.get(), Some(Rust::Missing | Rust::NoToolchain))
    }

    /// The install guide, over everything.
    pub(super) fn rust_guide_dialog(&mut self, cx: &mut Cx) {
        let found = self.rust.get().cloned();
        let w = PANEL_W.min(cx.width() - 16.0);
        let inner = w - PAD * 2.0;
        let mut parts = vec![Part::Text(cx.t("rust.intro").to_owned(), theme::DIM)];
        parts.push(match &found {
            None => Part::Text(cx.t("rust.checking").to_owned(), theme::DIM),
            Some(Rust::Found { version, .. }) => Part::Text(cx.tf("rust.found", &[version]), theme::ACCENT),
            Some(Rust::NoToolchain) => Part::Text(cx.t("rust.no_toolchain").to_owned(), theme::WARN),
            Some(Rust::Missing) => Part::Text(cx.t("rust.missing").to_owned(), theme::DANGER),
        });
        let step = |key: &'static str| Part::Text(cx.t(key).to_owned(), theme::TEXT);
        match &found {
            Some(Rust::Found { .. }) | None => {}
            Some(Rust::NoToolchain) => {
                parts.extend([step("rust.step_default"), Part::Command(toolchain::RUSTUP_DEFAULT)]);
            }
            Some(Rust::Missing) if cfg!(windows) => parts.extend([
                step("rust.win_step1"),
                step("rust.win_step2"),
                step("rust.win_or"),
                Part::Command(toolchain::RUSTUP_WINGET),
            ]),
            Some(Rust::Missing) => parts.extend([
                step("rust.unix_step1"),
                Part::Command(toolchain::RUSTUP_SH),
                step("rust.unix_step2"),
            ]),
        }
        if !matches!(found, Some(Rust::Found { .. }) | None) {
            parts.push(Part::Text(cx.t("rust.step_check").to_owned(), theme::DIM));
        }

        let height = |cx: &Cx, part: &Part| match part {
            Part::Text(text, _) => cx.ui.wrap(text, inner / SMALL).len() as f32 * LINE * SMALL + 4.0,
            Part::Command(_) => COMMAND_H + 4.0,
        };
        let body: f32 = parts.iter().map(|part| height(cx, part)).sum();
        let h = (PAD * 2.0 + LINE + GAP + body + GAP + BUTTON_H).min(cx.height() - 8.0);
        let panel = Rect::new(((cx.width() - w) / 2.0).round(), ((cx.height() - h) / 2.0).round(), w, h);
        cx.frame(panel, theme::PANEL, theme::FAINT);
        cx.clip(panel);
        let x = panel.x + PAD;
        cx.label(cx.t("rust.title"), x, panel.y + PAD, inner, theme::TEXT);
        let mut y = panel.y + PAD + LINE + GAP;
        let copied = self.copied_at.is_some_and(|at| at.elapsed() < COPIED);
        for part in &parts {
            match part {
                Part::Text(text, color) => {
                    cx.paragraph(text, x, y, inner, *color);
                }
                Part::Command(command) => {
                    let copy_w = 46.0;
                    let field = Rect::new(x, y, inner - copy_w - 4.0, COMMAND_H);
                    cx.frame(field, theme::FIELD, theme::EDGE);
                    if cx.small_path(command, field.x + 4.0, field.y + 5.0, field.w - 8.0, theme::TEXT) {
                        cx.tooltip(field, command);
                    }
                    let copy = Rect::new(field.right() + 4.0, y, copy_w, COMMAND_H);
                    let label = if copied { "common.copied" } else { "common.copy" };
                    if cx.button(copy, cx.t(label), Btn::NORMAL.small()) {
                        melon_base::platform::copy_to_clipboard(command);
                        self.copied_at = Some(Instant::now());
                    }
                }
            }
            y += height(cx, part);
        }
        let buttons_y = panel.bottom() - PAD - BUTTON_H;
        let close = Rect::new(panel.right() - PAD - 60.0, buttons_y, 60.0, BUTTON_H);
        let again = Rect::new(close.x - GAP - 84.0, buttons_y, 84.0, BUTTON_H);
        let site = Rect::new(again.x - GAP - 92.0, buttons_y, 92.0, BUTTON_H);
        if cx.button(site, cx.t("rust.open_site"), Btn::NORMAL) {
            melon_base::platform::open_url(toolchain::RUSTUP_URL);
        }
        if cx.button(again, cx.t("rust.check_again"), Btn::NORMAL.enabled(found.is_some())) {
            self.rust.look_again();
        }
        let primary = if matches!(found, Some(Rust::Found { .. })) { Btn::PRIMARY } else { Btn::NORMAL };
        if cx.button(close, cx.t("common.close"), primary) {
            self.rust_guide = false;
        }
        cx.unclip();
    }
}
