//! The selected instance in the centre: its name, the version it runs

use super::stats;
use super::window::{BUTTON_H, GAP};
use crate::cdn;
use melon::gui::screen::CursorKind;
use melon::gui::text::font::LINE_HEIGHT;
use melon::gui::widgets::Rect;
use melon::gui::widgets::Ui;
use melon::gui::widgets::button::Button;
use melon::instances::{self, Engine, Instance};
use melon::platform;
use std::cell::Cell;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const ROW_H: f32 = 24.0;
const NAME: [f32; 4] = [0.95, 0.95, 0.95, 1.0];
const DIM: [f32; 4] = [0.6, 0.6, 0.6, 1.0];
const PATH_SCALE: f32 = 0.75;
const CONFIRM: Duration = Duration::from_secs(3);
/// The folder button's width beside the version dropdown.
const FOLDER_W: f32 = 120.0;

pub enum Action {
    None,
    NameRename,
    NameCopy,
    Deleted,
    /// Name, engine or folder changed; reload the sidebar.
    Changed,
    /// Fetch the published Melon client into the melon folder.
    DownloadClient,
}

/// What the version dropdown offers: the engine and its label key.
fn versions() -> [(Engine, String); 2] {
    [
        (Engine::Melon, "Melon".to_owned()),
        (Engine::Java, format!("Java {}", melon::home::VERSION)),
    ]
}

pub struct InstanceDetail {
    main_dir: PathBuf,
    instance: Option<Instance>,
    rename: Button,
    copy: Button,
    remove: Button,
    folder: Button,
    /// The dropdown's closed face.
    version: Button,
    /// The options are showing under it.
    open: bool,
    confirming: Option<Instant>,
    path_rect: Cell<Rect>,
    cdn_rect: Cell<Rect>,
    mouse: (f32, f32),
    /// Top of the name, and of the lines about the instance.
    top: f32,
    info_y: f32,
    line_y: f32,
}

impl InstanceDetail {
    pub fn new(main_dir: PathBuf) -> Self {
        Self {
            main_dir,
            instance: None,
            rename: Button::sized("melon.launcher.rename", 0.0, BUTTON_H),
            copy: Button::sized("melon.launcher.copy_instance", 0.0, BUTTON_H),
            remove: Button::sized("melon.launcher.remove", 0.0, BUTTON_H),
            folder: Button::sized("melon.launcher.instance_open", 0.0, BUTTON_H),
            version: Button::sized("melon.launcher.engine.melon", 0.0, BUTTON_H),
            open: false,
            confirming: None,
            path_rect: Cell::new(Rect::default()),
            cdn_rect: Cell::new(Rect::default()),
            mouse: (f32::NEG_INFINITY, f32::NEG_INFINITY),
            top: 0.0,
            info_y: 0.0,
            line_y: 0.0,
        }
    }

    pub fn set_instance(&mut self, instance: Option<Instance>) {
        let same = match (&self.instance, &instance) {
            (Some(a), Some(b)) => a.id == b.id && a.path == b.path && a.engine == b.engine,
            (None, None) => true,
            _ => false,
        };
        self.instance = instance;
        if !same {
            self.confirming = None;
            self.open = false;
        }
        self.reload();
    }

    fn reload(&mut self) {
        if let Some(instance) = &self.instance {
            instances::ensure_dirs(&instance.path);
        }
        self.refresh_buttons();
    }

    fn refresh_buttons(&mut self) {
        let own = self.instance.as_ref().is_some_and(|i| !i.is_default);
        let any = self.instance.is_some();
        self.rename.enabled = own;
        self.copy.enabled = any;
        self.remove.enabled = own;
        self.folder.enabled = any;
        self.version.enabled = own;
    }

    /// Left edge, first button row, width: where the name entry sits.
    pub fn name_row(&self) -> (f32, f32, f32) {
        (self.rename.x, self.rename.y, self.rename.w * 2.0 + GAP)
    }

    pub fn resize_in(&mut self, x: f32, y: f32, _w: f32, _h: f32) {
        let left = x + 8.0;
        let row_w = (_w - 16.0).max(1.0);
        self.top = y + 8.0;
        let row_y = self.top + LINE_HEIGHT + 6.0;
        self.folder.w = FOLDER_W.min(row_w / 2.0);
        self.folder.x = left + row_w - self.folder.w;
        self.folder.y = row_y;
        self.version.x = left;
        self.version.y = row_y;
        self.version.w = (row_w - self.folder.w - GAP).max(1.0);
        self.line_y = row_y + BUTTON_H + 8.0;
        let third = ((row_w - GAP * 2.0) / 3.0).floor();
        let buttons_y = self.line_y + 9.0;
        for (i, button) in [&mut self.rename, &mut self.copy, &mut self.remove]
            .into_iter()
            .enumerate()
        {
            button.w = third;
            button.x = left + i as f32 * (third + GAP);
            button.y = buttons_y;
        }
        self.info_y = buttons_y + BUTTON_H + 12.0;
    }

    /// The options of the open dropdown, as rows under its face.
    fn option_rect(&self, index: usize) -> Rect {
        Rect::new(
            self.version.x,
            self.version.y + self.version.h + 2.0 + index as f32 * ROW_H,
            self.version.w,
            ROW_H,
        )
    }

    pub fn hover(&mut self, x: f32, y: f32, active: bool) {
        let (x, y) = if active {
            (x, y)
        } else {
            (f32::NEG_INFINITY, f32::NEG_INFINITY)
        };
        self.mouse = (x, y);
        for b in self.buttons_mut() {
            b.update_hover(x, y);
        }
    }

    pub fn cursor(&self) -> CursorKind {
        let over_option = self.open
            && (0..versions().len())
                .any(|i| self.option_rect(i).contains(self.mouse.0, self.mouse.1));
        if over_option
            || self.buttons().iter().any(|b| b.enabled && b.hovered)
            || self.path_rect.get().contains(self.mouse.0, self.mouse.1)
            || self.cdn_rect.get().contains(self.mouse.0, self.mouse.1)
        {
            CursorKind::Pointer
        } else {
            CursorKind::default()
        }
    }

    pub fn release(&mut self) {}

    pub fn scroll(&mut self, _delta: f32) {}

    pub fn click(&mut self, x: f32, y: f32, naming: bool) -> Action {
        if naming {
            return Action::None;
        }
        if self.open {
            self.open = false;
            for (i, (engine, _)) in versions().into_iter().enumerate() {
                if self.option_rect(i).contains(x, y) {
                    return self.set_engine(engine);
                }
            }
            if self.version.clicked(x, y) {
                return Action::None;
            }
        } else if self.version.clicked(x, y) {
            self.open = true;
            return Action::None;
        }
        if self.cdn_rect.get().contains(x, y) {
            return Action::DownloadClient;
        }
        if self.path_rect.get().contains(x, y)
            && let Some(instance) = &self.instance
        {
            platform::open_dir(&instance.path);
            return Action::None;
        }
        if self.rename.clicked(x, y) {
            return Action::NameRename;
        }
        if self.copy.clicked(x, y) {
            return Action::NameCopy;
        }
        if self.remove.clicked(x, y) {
            return self.remove_selected();
        }
        if self.folder.clicked(x, y) {
            if let Some(instance) = &self.instance {
                platform::open_dir(&instance.path);
            }
            return Action::None;
        }
        Action::None
    }

    fn set_engine(&mut self, engine: Engine) -> Action {
        let Some(instance) = &self.instance else {
            return Action::None;
        };
        if instance.is_default || instance.engine == engine {
            return Action::None;
        }
        match instances::set_engine(&self.main_dir, &instance.id, engine) {
            Ok(updated) => {
                self.set_instance(Some(updated));
                Action::Changed
            }
            Err(e) => {
                eprintln!("Instanz-Engine konnte nicht gesetzt werden: {e}");
                Action::None
            }
        }
    }

    fn remove_selected(&mut self) -> Action {
        let Some(instance) = &self.instance else {
            return Action::None;
        };
        if instance.is_default {
            return Action::None;
        }
        if self.confirming.is_none_or(|at| at.elapsed() > CONFIRM) {
            self.confirming = Some(Instant::now());
            return Action::None;
        }
        let id = instance.id.clone();
        if let Err(e) = instances::delete(&self.main_dir, &id) {
            eprintln!("Instanz '{id}' konnte nicht gelöscht werden: {e}");
            return Action::None;
        }
        self.confirming = None;
        self.instance = None;
        Action::Deleted
    }

    pub fn draw(&self, ui: &mut Ui) {
        let left = self.version.x;
        let width = self.folder.x + self.folder.w - left;
        let top = self.top;
        let Some(instance) = &self.instance else {
            return;
        };

        let name = if instance.id == instances::MELON {
            format!("{}  {}", instance.name, cdn::discover::latest_game())
        } else {
            instance.name.clone()
        };
        let name = ui.ellipsize(&name, width);
        ui.text(&name, left, top, NAME);

        let label = match instance.engine {
            Engine::Melon => "Melon".to_owned(),
            Engine::Java => format!(
                "Java {}",
                instance
                    .java_version
                    .as_deref()
                    .unwrap_or(melon::home::VERSION)
            ),
        };
        self.version.draw_labeled(ui, &format!("{label}  v"));
        self.folder.draw(ui);
        ui.fill(
            Rect::new(left, self.line_y, width, 1.0),
            [0.35, 0.35, 0.35, 1.0],
        );

        self.rename.draw(ui);
        self.copy.draw(ui);
        if self.confirming.is_some_and(|at| at.elapsed() <= CONFIRM) {
            let label = ui.tr("melon.launcher.confirm_delete");
            self.remove
                .draw_labeled_colored(ui, label, [1.0, 0.55, 0.5, 1.0]);
        } else {
            self.remove.draw(ui);
        }

        let path = instance.path.display().to_string();
        let path = ui.ellipsize(&path, width / PATH_SCALE);
        self.path_rect
            .set(Rect::new(left, self.info_y, width, LINE_HEIGHT + 2.0));
        ui.text_scaled(&path, left, self.info_y, DIM, PATH_SCALE);
        let stats = play_stats(ui, instance);
        ui.text_scaled(
            &ui.ellipsize(&stats, width / PATH_SCALE),
            left,
            self.info_y + LINE_HEIGHT + 2.0,
            DIM,
            PATH_SCALE,
        );
        if instance.id == instances::MELON && !cdn::discover::client_is_latest(&self.main_dir) {
            let dest = cdn::game_dest(&self.main_dir).display().to_string();
            let cdn_y = self.info_y + LINE_HEIGHT * 2.0 + 4.0;
            self.cdn_rect
                .set(Rect::new(left, cdn_y, width, LINE_HEIGHT + 2.0));
            ui.text_scaled(
                &ui.ellipsize(&dest, width / PATH_SCALE),
                left,
                cdn_y,
                DIM,
                PATH_SCALE,
            );
        } else {
            self.cdn_rect.set(Rect::default());
        }

        if self.open {
            for (i, (engine, name)) in versions().into_iter().enumerate() {
                let rect = self.option_rect(i);
                let hovered = rect.contains(self.mouse.0, self.mouse.1);
                let chosen = engine == instance.engine;
                ui.fill(
                    rect,
                    if hovered {
                        [0.28, 0.32, 0.3, 1.0]
                    } else {
                        [0.16, 0.18, 0.17, 1.0]
                    },
                );
                ui.text(
                    &name,
                    rect.x + 6.0,
                    rect.y + (rect.h - 8.0) / 2.0,
                    if chosen { NAME } else { DIM },
                );
            }
        }
    }

    fn buttons(&self) -> [&Button; 5] {
        [
            &self.rename,
            &self.copy,
            &self.remove,
            &self.folder,
            &self.version,
        ]
    }

    fn buttons_mut(&mut self) -> [&mut Button; 5] {
        [
            &mut self.rename,
            &mut self.copy,
            &mut self.remove,
            &mut self.folder,
            &mut self.version,
        ]
    }
}

fn play_stats(ui: &Ui, instance: &Instance) -> String {
    match instance.last_played {
        None | Some(0) => ui.tr("melon.launcher.never").to_owned(),
        Some(last) => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(last);
            let ago = stats::ago(ui.lang, now.saturating_sub(last));
            ui.lang.format(
                "melon.launcher.stats",
                &[&ago, &stats::span(ui.lang, instance.played_secs)],
            )
        }
    }
}
