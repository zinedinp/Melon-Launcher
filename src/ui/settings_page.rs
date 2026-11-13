//! The launcher's settings page: how the game starts. The validation

use super::window::{BUTTON_H, GAP, ROW_W};
use melon::gui::screen::{CursorKind, TextEvent};
use melon::gui::text::font::LINE_HEIGHT;
use melon::gui::widgets::Ui;
use melon::gui::widgets::Widget;
use melon::gui::widgets::button::Button;
use melon::gui::widgets::edit_box::EditBox;
use melon::i18n::languages::Language;
use melon::instances::{self, Instance, LauncherSettings};
use melon::settings::options::Config;
use std::path::{Path, PathBuf};

const DIM: [f32; 4] = [0.6, 0.6, 0.6, 1.0];
const LANGUAGES: [Language; 2] = [Language::EnUs, Language::DeDe];
const OPEN_W: f32 = 56.0;
const BROWSE_W: f32 = 64.0;

pub enum Action {
    None,
    /// The language changed: the launcher's own text follows it.
    LanguageChanged,
}

pub struct SettingsPage {
    /// Where `options.txt` is read and written (main folder when global).
    options_dir: PathBuf,
    instance_id: String,
    use_global: bool,
    config: Config,
    global: Button,
    validation: Button,
    probe: Button,
    language: Button,
    fullscreen: Button,
    reopen: Button,
    assets_path: EditBox,
    assets_browse: Button,
    assets_open: Button,
}

impl SettingsPage {
    pub fn new(main_dir: &Path, instance: Option<&Instance>) -> Self {
        let mut page = Self {
            options_dir: main_dir.to_path_buf(),
            instance_id: instances::MELON.into(),
            use_global: true,
            config: Config::default(),
            global: Button::sized("melon.launcher.setting.global", ROW_W, BUTTON_H),
            validation: Button::sized("melon.launcher.setting.validation", ROW_W, BUTTON_H),
            probe: Button::sized("melon.launcher.setting.probe", ROW_W, BUTTON_H),
            language: Button::sized("melon.launcher.setting.language", ROW_W, BUTTON_H),
            fullscreen: Button::sized("melon.launcher.setting.fullscreen", ROW_W, BUTTON_H),
            reopen: Button::sized("melon.launcher.setting.reopen", ROW_W, BUTTON_H),
            assets_path: EditBox::new(
                "melon.launcher.setting.assets_hint",
                ROW_W - OPEN_W - BROWSE_W - GAP * 2.0,
                BUTTON_H,
            ),
            assets_browse: Button::sized(
                "melon.launcher.setting.assets_browse",
                BROWSE_W,
                BUTTON_H,
            ),
            assets_open: Button::sized("melon.launcher.setting.assets_open", OPEN_W, BUTTON_H),
        };
        page.set_instance(main_dir, instance);
        page
    }

    /// Another instance was selected: show its options, or the global file.
    pub fn set_instance(&mut self, main_dir: &Path, instance: Option<&Instance>) {
        let melon_dir = instances::dir(main_dir, instances::MELON);
        let (id, path, global) = match instance {
            Some(instance) => (
                instance.id.as_str(),
                instance.path.as_path(),
                instance.use_global_options,
            ),
            None => (instances::MELON, melon_dir.as_path(), true),
        };
        self.instance_id = id.to_string();
        self.use_global = id == instances::MELON || global;
        self.options_dir = if self.use_global {
            melon_dir.clone()
        } else {
            path.to_path_buf()
        };
        self.config = Config::load(&self.options_dir);
        self.global.enabled = id != instances::MELON;
    }

    pub fn resize_in(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let row_w = w.min(ROW_W).max(1.0);
        let left = x + ((w - row_w) / 2.0).floor();
        let rows = [
            &mut self.global,
            &mut self.validation,
            &mut self.probe,
            &mut self.language,
            &mut self.fullscreen,
            &mut self.reopen,
        ];
        let total_h = 6.0 * (BUTTON_H + GAP) + BUTTON_H + 48.0;
        let top = y + ((h - total_h) / 2.0).max(16.0);
        for (i, button) in rows.into_iter().enumerate() {
            button.x = left;
            button.w = row_w;
            button.y = top + i as f32 * (BUTTON_H + GAP);
        }
        self.assets_path.x = left;
        self.assets_path.y = self.reopen.y + BUTTON_H + GAP * 2.0 + LINE_HEIGHT + 4.0;
        self.assets_path.w = (row_w - OPEN_W - BROWSE_W - GAP * 2.0).max(1.0);
        self.assets_path.h = BUTTON_H;
        self.assets_browse.x = left + self.assets_path.w + GAP;
        self.assets_browse.y = self.assets_path.y;
        self.assets_browse.w = BROWSE_W;
        self.assets_browse.h = BUTTON_H;
        self.assets_open.x = self.assets_browse.x + BROWSE_W + GAP;
        self.assets_open.y = self.assets_path.y;
        self.assets_open.w = OPEN_W;
        self.assets_open.h = BUTTON_H;
    }

    pub fn hover(&mut self, x: f32, y: f32, active: bool) {
        let (x, y) = if active {
            (x, y)
        } else {
            (f32::NEG_INFINITY, f32::NEG_INFINITY)
        };
        for button in [
            &mut self.global,
            &mut self.validation,
            &mut self.probe,
            &mut self.language,
            &mut self.fullscreen,
            &mut self.reopen,
            &mut self.assets_browse,
            &mut self.assets_open,
        ] {
            button.update_hover(x, y);
        }
        self.assets_path.update_hover(x, y);
    }

    pub fn cursor(&self) -> CursorKind {
        let buttons = [
            &self.global,
            &self.validation,
            &self.probe,
            &self.language,
            &self.fullscreen,
            &self.reopen,
            &self.assets_browse,
            &self.assets_open,
        ];
        if buttons.iter().any(|b| b.hovered) {
            CursorKind::Pointer
        } else if self.assets_path.cursor().is_some() {
            CursorKind::Text
        } else {
            CursorKind::Default
        }
    }

    pub fn key(
        &mut self,
        event: &TextEvent,
        settings: &mut LauncherSettings,
        main_dir: &Path,
    ) -> bool {
        if !self.assets_path.focused() {
            return false;
        }
        if matches!(event, TextEvent::Enter) {
            self.save_assets(settings, main_dir);
            self.assets_path.set_focused(false);
        } else {
            self.assets_path.handle(event);
        }
        true
    }

    pub fn click(
        &mut self,
        x: f32,
        y: f32,
        settings: &mut LauncherSettings,
        main_dir: &Path,
        naming: bool,
    ) -> Action {
        if naming {
            return Action::None;
        }
        if self.assets_path.click(x, y) {
            if self.assets_path.value().is_empty() {
                self.assets_path
                    .set_value(&settings.assets_dir(main_dir).display().to_string());
            }
            return Action::None;
        }
        if self.assets_path.focused() {
            self.save_assets(settings, main_dir);
            self.assets_path.set_focused(false);
        }
        if self.assets_browse.clicked(x, y) {
            if let Some(path) = melon::platform::pick_folder("Melon assets") {
                settings.set_assets_path(main_dir, path);
                self.assets_path.set_value("");
            }
            return Action::None;
        }
        if self.assets_open.clicked(x, y) {
            let dir = settings.assets_dir(main_dir);
            let _ = std::fs::create_dir_all(&dir);
            if let Err(e) = open::that(&dir) {
                eprintln!("assets-Ordner konnte nicht geöffnet werden: {e}");
            }
            return Action::None;
        }
        if self.global.clicked(x, y) {
            let on = !self.use_global;
            match instances::set_use_global_options(main_dir, &self.instance_id, on) {
                Ok(updated) => {
                    let before = self.config.lang;
                    self.set_instance(main_dir, Some(&updated));
                    if self.config.lang != before {
                        return Action::LanguageChanged;
                    }
                }
                Err(e) => eprintln!("Globale Optionen konnten nicht gesetzt werden: {e}"),
            }
        } else if self.validation.clicked(x, y) {
            settings.validation = !settings.validation;
            settings.save(main_dir);
        } else if self.probe.clicked(x, y) {
            settings.probe = !settings.probe;
            settings.save(main_dir);
        } else if self.reopen.clicked(x, y) {
            settings.exit_after_game = !settings.exit_after_game;
            settings.save(main_dir);
        } else if self.fullscreen.clicked(x, y) {
            self.config.fullscreen = !self.config.fullscreen;
            self.config.save(&self.options_dir);
        } else if self.language.clicked(x, y) {
            self.config.lang = next_language(self.config.lang);
            self.config.save(&self.options_dir);
            return Action::LanguageChanged;
        }
        Action::None
    }

    fn save_assets(&mut self, settings: &mut LauncherSettings, main_dir: &Path) {
        let value = self.assets_path.value();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            settings.assets_dir = None;
            settings.save(main_dir);
            return;
        }
        settings.set_assets_path(main_dir, PathBuf::from(trimmed));
    }

    /// The language the selected instance is set to.
    pub fn language(&self) -> Language {
        self.config.lang
    }

    pub fn draw(&self, ui: &mut Ui, settings: &LauncherSettings, main_dir: &Path) {
        let on_off = |on: bool| ui.tr(if on { "options.on" } else { "options.off" });
        let global = ui
            .lang
            .format("melon.launcher.setting.global", &[on_off(self.use_global)]);
        let validation = ui.lang.format(
            "melon.launcher.setting.validation",
            &[on_off(settings.validation)],
        );
        let probe = ui
            .lang
            .format("melon.launcher.setting.probe", &[on_off(settings.probe)]);
        let language = ui.lang.format(
            "melon.launcher.setting.language",
            &[self.config.lang.name()],
        );
        let fullscreen = ui.lang.format(
            "melon.launcher.setting.fullscreen",
            &[on_off(self.config.fullscreen)],
        );
        let reopen = ui.lang.format(
            "melon.launcher.setting.reopen",
            &[on_off(!settings.exit_after_game)],
        );
        self.global.draw_labeled(ui, &global);
        self.validation.draw_labeled(ui, &validation);
        self.probe.draw_labeled(ui, &probe);
        self.language.draw_labeled(ui, &language);
        self.fullscreen.draw_labeled(ui, &fullscreen);
        self.reopen.draw_labeled(ui, &reopen);

        let cx = self.validation.x + self.validation.w / 2.0;
        let version = format!("Melon {}", env!("CARGO_PKG_VERSION"));
        ui.centered(&version, cx, self.global.y - 16.0, [1.0; 4]);

        let label = ui.tr("melon.launcher.setting.assets");
        ui.centered(label, cx, self.assets_path.y - LINE_HEIGHT - 2.0, DIM);
        if self.assets_path.focused() {
            self.assets_path.draw(ui);
        } else {
            let rect = self.assets_path.rect();
            ui.fill(rect, [0.16, 0.18, 0.17, 1.0]);
            let path = settings.assets_dir(main_dir).display().to_string();
            let shown = ui.ellipsize(&path, rect.w - 8.0);
            ui.text(
                &shown,
                rect.x + 4.0,
                rect.y + (rect.h - 8.0) / 2.0,
                [0.9; 4],
            );
        }
        self.assets_browse.draw(ui);
        self.assets_open.draw(ui);
    }
}

/// The next of [`LANGUAGES`]; any other language goes to the first.
fn next_language(current: Language) -> Language {
    match LANGUAGES.iter().position(|&l| l == current) {
        Some(i) => LANGUAGES[(i + 1) % LANGUAGES.len()],
        None => LANGUAGES[0],
    }
}
