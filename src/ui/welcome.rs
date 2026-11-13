//! First screen: extract a local 26.3 client into `assets/<version>/`,

use melon::gui::screen::{CursorKind, TextEvent};
use melon::gui::text::font::LINE_HEIGHT;
use melon::gui::widgets::Rect;
use melon::gui::widgets::Ui;
use melon::gui::widgets::button::Button;
use melon::home;
use melon::platform;
use melon::prepare::hashes::Verdict;
use melon::prepare::{self, Found};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

const BUTTON_W: f32 = 260.0;
const BUTTON_H: f32 = 24.0;
const GAP: f32 = 8.0;
/// Extra space above **Set melon folder**, under Extract / jar / Account.
const FOLDER_GAP: f32 = 20.0;
/// Nudge Extract / jar / Account (and the title) up from vertical centre.
const SHIFT_UP: f32 = 36.0;
/// SHA-1 and folder paths, a notch under the buttons.
const PATH_SCALE: f32 = 0.75;
const RULE: [f32; 4] = [0.5, 0.55, 0.48, 1.0];
const TITLE: [f32; 4] = [0.95, 0.95, 0.95, 1.0];
const DIM: [f32; 4] = [0.7, 0.7, 0.7, 1.0];
const ERR: [f32; 4] = [0.95, 0.45, 0.4, 1.0];
const WARN: [f32; 4] = [0.95, 0.75, 0.35, 1.0];
const OK: [f32; 4] = [0.35, 0.85, 0.4, 1.0];
const COPY_FEEDBACK: std::time::Duration = std::time::Duration::from_secs(2);

pub enum Action {
    None,
    /// Microsoft sign-in, then the main window.
    Account,
    /// Unzip the current jar into Melon's assets folder.
    Extract,
    /// Persist this folder as the Melon folder (assets and later downloads).
    SetAssetsDir(PathBuf),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HashStatus {
    Idle,
    Checking,
    Match,
    Mismatch,
    Unknown,
    Offline,
}

pub struct Welcome {
    extract: Button,
    choose: Button,
    folder: Button,
    account: Button,
    found: Option<Found>,
    sha1: Option<String>,
    home: PathBuf,
    assets_dir: PathBuf,
    stale: bool,
    progress: Option<String>,
    error: Option<String>,
    busy: bool,
    hash_status: HashStatus,
    hash_rx: Option<Receiver<Verdict>>,
    hash_rect: Cell<Rect>,
    copied_at: Cell<Option<Instant>>,
    mouse: (f32, f32),
}

impl Welcome {
    pub fn new(home: &Path, assets_dir: PathBuf) -> Self {
        let found = prepare::scan(home);
        let mut welcome = Self {
            extract: Button::sized("melon.launcher.welcome.extract", BUTTON_W, BUTTON_H),
            choose: Button::sized("melon.launcher.welcome.choose", BUTTON_W, BUTTON_H),
            folder: Button::sized("melon.launcher.welcome.folder", BUTTON_W, BUTTON_H),
            account: Button::sized("melon.launcher.welcome.account", BUTTON_W, BUTTON_H),
            sha1: found.as_ref().and_then(|f| prepare::sha1_file(&f.jar)),
            stale: prepare::needs_extract(home, &assets_dir) && home::is_prepared_at(&assets_dir),
            found,
            home: home.to_path_buf(),
            assets_dir,
            progress: None,
            error: None,
            busy: false,
            hash_status: HashStatus::Idle,
            hash_rx: None,
            hash_rect: Cell::new(Rect::default()),
            copied_at: Cell::new(None),
            mouse: (-1.0, -1.0),
        };
        welcome.begin_hash_check();
        welcome.refresh();
        welcome
    }

    pub fn found(&self) -> Option<&Found> {
        self.found.as_ref()
    }

    pub fn set_assets_dir(&mut self, assets_dir: PathBuf) {
        self.stale =
            prepare::needs_extract(&self.home, &assets_dir) && home::is_prepared_at(&assets_dir);
        self.assets_dir = assets_dir;
    }

    pub fn set_progress(&mut self, message: String) {
        self.progress = Some(message);
        self.error = None;
        self.busy = true;
        self.refresh();
    }

    pub fn set_error(&mut self, message: String) {
        self.error = Some(message);
        self.busy = false;
        self.refresh();
    }

    fn set_found(&mut self, found: Found) {
        self.sha1 = prepare::sha1_file(&found.jar);
        self.stale = true;
        self.found = Some(found);
        self.error = None;
        self.hash_status = HashStatus::Idle;
        self.begin_hash_check();
        self.refresh();
    }

    fn begin_hash_check(&mut self) {
        self.hash_rx = None;
        let Some(sha1) = self.sha1.clone() else {
            self.hash_status = HashStatus::Idle;
            return;
        };
        let version = home::VERSION.to_string();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(prepare::hashes::verify(&version, &sha1));
        });
        self.hash_rx = Some(rx);
        self.hash_status = HashStatus::Checking;
    }

    fn poll_hash(&mut self) {
        let Some(rx) = &self.hash_rx else {
            return;
        };
        let Ok(verdict) = rx.try_recv() else {
            return;
        };
        self.hash_rx = None;
        self.hash_status = match verdict {
            Verdict::Match => HashStatus::Match,
            Verdict::Mismatch { .. } => HashStatus::Mismatch,
            Verdict::Unknown => HashStatus::Unknown,
            Verdict::Offline => HashStatus::Offline,
        };
        self.refresh();
    }

    fn refresh(&mut self) {
        let hash_ok = self.hash_status != HashStatus::Mismatch;
        self.extract.enabled = !self.busy && self.found.is_some() && hash_ok;
        self.extract.focused = self.extract.enabled;
        self.choose.enabled = !self.busy;
        self.folder.enabled = !self.busy;
        self.account.enabled = !self.busy;
    }

    pub fn resize(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let left = x + ((w - BUTTON_W) / 2.0).floor();
        let stack = BUTTON_H * 4.0 + GAP * 2.0 + FOLDER_GAP;
        let title_room = 36.0 + LINE_HEIGHT + 8.0;
        let top = (y + (h / 2.0 - stack / 2.0) - SHIFT_UP)
            .max(y + title_room)
            .floor();
        self.extract.x = left;
        self.extract.y = top;
        self.choose.x = left;
        self.choose.y = top + BUTTON_H + GAP;
        self.account.x = left;
        self.account.y = self.choose.y + BUTTON_H + GAP;
        self.folder.x = left;
        self.folder.y = self.account.y + BUTTON_H + FOLDER_GAP;
    }

    pub fn hover(&mut self, x: f32, y: f32) {
        self.poll_hash();
        self.mouse = (x, y);
        self.extract.update_hover(x, y);
        self.choose.update_hover(x, y);
        self.folder.update_hover(x, y);
        self.account.update_hover(x, y);
    }

    pub fn cursor(&self) -> CursorKind {
        if [&self.extract, &self.choose, &self.folder, &self.account]
            .iter()
            .any(|b| b.enabled && b.hovered)
            || (self.sha1.is_some() && self.hash_rect.get().contains(self.mouse.0, self.mouse.1))
        {
            CursorKind::Pointer
        } else {
            CursorKind::Default
        }
    }

    pub fn key(&mut self, event: &TextEvent) -> Action {
        self.poll_hash();
        if matches!(event, TextEvent::Enter) && self.extract.enabled {
            Action::Extract
        } else {
            Action::None
        }
    }

    pub fn click(&mut self, x: f32, y: f32) -> Action {
        self.poll_hash();
        if self.extract.clicked(x, y) {
            if self.hash_status == HashStatus::Mismatch {
                return Action::None;
            }
            Action::Extract
        } else if self.choose.clicked(x, y) {
            if let Some(path) = platform::pick_file("Minecraft client jar", "jar") {
                self.set_found(prepare::from_jar(path));
            }
            Action::None
        } else if self.folder.clicked(x, y) {
            if let Some(path) = platform::pick_folder("Melon folder") {
                Action::SetAssetsDir(path)
            } else {
                Action::None
            }
        } else if self.account.clicked(x, y) {
            Action::Account
        } else if self.sha1.is_some() && self.hash_rect.get().contains(x, y) {
            if let Some(sha1) = &self.sha1 {
                platform::copy_to_clipboard(sha1);
                self.copied_at.set(Some(Instant::now()));
            }
            Action::None
        } else {
            Action::None
        }
    }

    pub fn draw(&self, ui: &mut Ui) {
        let title = ui.tr("melon.launcher.welcome.title");
        let tw = ui.text_width(title);
        let cx = self.extract.x + (BUTTON_W - tw) / 2.0;
        ui.text(title, cx, self.extract.y - 36.0, TITLE);

        let sub = ui
            .lang
            .format("melon.launcher.welcome.sub", &[home::VERSION]);
        let sw = ui.text_width(&sub);
        ui.text(
            &sub,
            self.extract.x + (BUTTON_W - sw) / 2.0,
            self.extract.y - 36.0 + LINE_HEIGHT + 4.0,
            DIM,
        );

        self.extract.draw(ui);
        self.choose.draw(ui);
        self.account.draw(ui);
        let rule_y = (self.account.y + BUTTON_H + self.folder.y) * 0.5;
        ui.fill(
            Rect::new(self.extract.x, rule_y.floor(), BUTTON_W, 1.0),
            RULE,
        );
        self.folder.draw(ui);

        let wrap_x = self.extract.x - 20.0;
        let wrap_w = BUTTON_W + 40.0;
        let mut y = self.folder.y + BUTTON_H + GAP * 2.0;
        if let Some(error) = &self.error {
            self.hash_rect.set(Rect::default());
            wrap_line(ui, error, wrap_x, y, wrap_w, ERR);
            return;
        }
        if let Some(progress) = &self.progress {
            self.hash_rect.set(Rect::default());
            wrap_line(ui, progress, wrap_x, y, wrap_w, DIM);
            return;
        }

        let dest = ui.lang.format(
            "melon.launcher.welcome.dest",
            &[&self.melon_folder().display().to_string()],
        );
        y += wrap_scaled(ui, &dest, wrap_x, y, wrap_w, DIM) + 2.0;
        let assets = ui.lang.format(
            "melon.launcher.welcome.assets",
            &[&self.assets_dir.display().to_string()],
        );
        y += wrap_scaled(ui, &assets, wrap_x, y, wrap_w, DIM) + 4.0;

        if self.stale {
            let note = ui.tr("melon.launcher.welcome.stale");
            y += wrap_line(ui, note, wrap_x, y, wrap_w, WARN) as f32 * (LINE_HEIGHT + 1.0) + 4.0;
        }

        if let Some(found) = &self.found {
            let source = ui.lang.format(
                "melon.launcher.welcome.found",
                &[&format!("{} ({})", found.label, found.jar.display())],
            );
            y += wrap_scaled(ui, &source, wrap_x, y, wrap_w, DIM) + 4.0;
            if let Some(sha1) = &self.sha1 {
                let _ = self.draw_hash(ui, sha1, wrap_x, y, wrap_w);
            }
        } else {
            wrap_line(
                ui,
                ui.tr("melon.launcher.welcome.none"),
                wrap_x,
                y,
                wrap_w,
                DIM,
            );
        }
    }

    fn draw_hash(&self, ui: &mut Ui, sha1: &str, x: f32, y: f32, w: f32) -> usize {
        let copied = self
            .copied_at
            .get()
            .is_some_and(|t| t.elapsed() < COPY_FEEDBACK);
        let (color, mark) = match self.hash_status {
            HashStatus::Match => (OK, Mark::Check),
            HashStatus::Mismatch => (ERR, Mark::Cross),
            HashStatus::Checking => (DIM, Mark::None),
            _ => (DIM, Mark::None),
        };
        let mark_w = 12.0;
        draw_mark(ui, x, y, mark);
        let hash = ui.lang.format("melon.launcher.welcome.hash", &[sha1]);
        let h = wrap_scaled(ui, &hash, x + mark_w, y, w - mark_w, color);
        self.hash_rect
            .set(Rect::new(x, y, w, h.max(BUTTON_H * PATH_SCALE)));
        if copied {
            let hint = ui.tr("melon.launcher.welcome.hash_copied");
            wrap_scaled(ui, hint, x + mark_w, y + h, w - mark_w, OK);
            return 2;
        }
        match self.hash_status {
            HashStatus::Checking => {
                wrap_scaled(
                    ui,
                    ui.tr("melon.launcher.welcome.hash_check"),
                    x + mark_w,
                    y + h,
                    w - mark_w,
                    DIM,
                );
                2
            }
            HashStatus::Match => {
                wrap_scaled(
                    ui,
                    ui.tr("melon.launcher.welcome.hash_ok"),
                    x + mark_w,
                    y + h,
                    w - mark_w,
                    OK,
                );
                2
            }
            HashStatus::Mismatch => {
                let note = ui
                    .lang
                    .format("melon.launcher.welcome.hash_bad", &[home::VERSION]);
                wrap_scaled(ui, &note, x + mark_w, y + h, w - mark_w, ERR);
                2
            }
            _ => 1,
        }
    }

    /// The chosen Melon folder (assets base), not the version subfolder.
    fn melon_folder(&self) -> &Path {
        let path = self.assets_dir.as_path();
        if path.file_name().and_then(|n| n.to_str()) == Some(home::VERSION) {
            path.parent().unwrap_or(path)
        } else {
            path
        }
    }
}

enum Mark {
    None,
    Check,
    Cross,
}

fn draw_mark(ui: &mut Ui, x: f32, y: f32, mark: Mark) {
    match mark {
        Mark::None => {}
        Mark::Check => {
            ui.fill(Rect::new(x, y + 5.0, 2.0, 3.0), OK);
            ui.fill(Rect::new(x + 1.0, y + 7.0, 3.0, 2.0), OK);
            ui.fill(Rect::new(x + 3.0, y + 5.0, 2.0, 4.0), OK);
            ui.fill(Rect::new(x + 5.0, y + 3.0, 2.0, 4.0), OK);
            ui.fill(Rect::new(x + 7.0, y + 1.0, 2.0, 4.0), OK);
        }
        Mark::Cross => {
            ui.fill(Rect::new(x + 1.0, y + 1.0, 2.0, 2.0), ERR);
            ui.fill(Rect::new(x + 3.0, y + 3.0, 2.0, 2.0), ERR);
            ui.fill(Rect::new(x + 5.0, y + 5.0, 2.0, 2.0), ERR);
            ui.fill(Rect::new(x + 7.0, y + 7.0, 2.0, 2.0), ERR);
            ui.fill(Rect::new(x + 7.0, y + 1.0, 2.0, 2.0), ERR);
            ui.fill(Rect::new(x + 5.0, y + 3.0, 2.0, 2.0), ERR);
            ui.fill(Rect::new(x + 3.0, y + 5.0, 2.0, 2.0), ERR);
            ui.fill(Rect::new(x + 1.0, y + 7.0, 2.0, 2.0), ERR);
        }
    }
}

fn wrap_line(ui: &mut Ui, text: &str, x: f32, y: f32, w: f32, color: [f32; 4]) -> usize {
    let lines = ui.font().wrap(text, w);
    for (i, line) in lines.iter().enumerate() {
        ui.text(line, x, y + i as f32 * (LINE_HEIGHT + 1.0), color);
    }
    lines.len().max(1)
}

/// Path / SHA lines. Returns the height used.
fn wrap_scaled(ui: &mut Ui, text: &str, x: f32, y: f32, w: f32, color: [f32; 4]) -> f32 {
    let step = (LINE_HEIGHT + 1.0) * PATH_SCALE;
    let lines = ui.font().wrap(text, w / PATH_SCALE);
    for (i, line) in lines.iter().enumerate() {
        ui.text_scaled(line, x, y + i as f32 * step, color, PATH_SCALE);
    }
    lines.len().max(1) as f32 * step
}
