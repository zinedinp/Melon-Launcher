//! The launcher's instance sidebar: separate game folders. The default

use super::window::{BUTTON_H, GAP};
use melon::gui::screen::{CursorKind, TextEvent};
use melon::gui::text::font::LINE_HEIGHT;
use melon::gui::widgets::Ui;
use melon::gui::widgets::button::Button;
use melon::gui::widgets::scroll_list::{ListClick, ScrollList};
use melon::instances::{self, Engine, Instance, LauncherSettings};
use std::path::PathBuf;

const ROW_H: f32 = 28.0;
const NAME: [f32; 4] = [0.95, 0.95, 0.95, 1.0];
const DIM: [f32; 4] = [0.6, 0.6, 0.6, 1.0];

pub enum Action {
    None,
    Play,
    NameNew,
}

struct Entry {
    instance: Instance,
    last_server: Option<String>,
}

pub struct InstancePage {
    main_dir: PathBuf,
    entries: Vec<Entry>,
    list: ScrollList,
    new: Button,
}

impl InstancePage {
    pub fn new(main_dir: PathBuf, settings: &LauncherSettings) -> Self {
        let mut page = Self {
            main_dir,
            entries: Vec::new(),
            list: ScrollList::new(200.0, ROW_H),
            new: Button::sized("melon.launcher.new_instance", 0.0, BUTTON_H),
        };
        page.reload(settings);
        page
    }

    pub fn reload(&mut self, settings: &LauncherSettings) {
        let main = &self.main_dir;
        self.entries = instances::list(main)
            .into_iter()
            .map(|instance| {
                let last_server = instances::quick_join(&instance);
                Entry {
                    last_server,
                    instance,
                }
            })
            .collect();
        self.list.set_len(self.entries.len());
        let current = settings.instance_in(main);
        let index = self
            .entries
            .iter()
            .position(|e| current.as_deref() == Some(e.instance.id.as_str()))
            .unwrap_or(0);
        self.list.select(Some(index));
    }

    pub fn selected(&self) -> Option<&Instance> {
        self.list
            .selected()
            .and_then(|i| self.entries.get(i))
            .map(|e| &e.instance)
    }

    pub fn selected_name(&self) -> Option<&str> {
        self.selected()
            .filter(|i| !i.is_default)
            .map(|i| i.name.as_str())
    }

    pub fn column_x(&self) -> f32 {
        self.new.x
    }

    pub fn add_row_y(&self) -> f32 {
        self.new.y
    }

    pub fn resize_column(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let bottom = y + h;
        let row_w = (w - 8.0).max(1.0);
        self.list.set_row_width(row_w);
        self.new.w = (w - GAP).max(1.0);
        self.new.x = x;
        self.new.y = bottom - BUTTON_H;
        let end = (self.new.y - 4.0).max(y + ROW_H);
        self.list
            .set_area(x + 1.0, y + 1.0, end, (w - 2.0).max(1.0));
    }

    pub fn hover(&mut self, x: f32, y: f32, active: bool) {
        let (x, y) = if active {
            (x, y)
        } else {
            (f32::NEG_INFINITY, f32::NEG_INFINITY)
        };
        self.list.on_mouse_move(x, y);
        self.new.update_hover(x, y);
    }

    pub fn cursor(&self, naming: bool) -> CursorKind {
        if !naming && self.new.enabled && self.new.hovered {
            CursorKind::Pointer
        } else {
            self.list.cursor().unwrap_or_default()
        }
    }

    pub fn release(&mut self) {
        self.list.on_mouse_up();
    }

    pub fn scroll(&mut self, delta: f32) {
        self.list.on_scroll(delta);
    }

    pub fn click(
        &mut self,
        x: f32,
        y: f32,
        naming: bool,
        settings: &mut LauncherSettings,
    ) -> Action {
        match self.list.on_click(x, y) {
            ListClick::Row { index, double } => {
                let _ = double;
                self.select(index, settings);
                return Action::None;
            }
            ListClick::Scrollbar => return Action::None,
            ListClick::None => {}
        }
        if naming {
            return Action::None;
        }
        if self.new.clicked(x, y) {
            Action::NameNew
        } else {
            Action::None
        }
    }

    pub fn key(&mut self, event: TextEvent, settings: &mut LauncherSettings) -> Action {
        let selected = self.list.selected().unwrap_or(0);
        match event {
            TextEvent::Up => self.select(selected.saturating_sub(1), settings),
            TextEvent::Down => self.select((selected + 1).min(self.entries.len() - 1), settings),
            TextEvent::Enter => return Action::Play,
            _ => {}
        }
        Action::None
    }

    pub fn create(&mut self, name: &str, settings: &mut LauncherSettings) {
        match instances::create(&self.main_dir, name) {
            Ok(made) => {
                settings.instance = Some(made.id);
                settings.save(&self.main_dir);
                self.reload(settings);
                let index = self.list.selected().unwrap_or(0);
                let height = self.list.viewport_height();
                self.list.center_on(index, height);
            }
            Err(e) => eprintln!("Instanz '{name}' konnte nicht angelegt werden: {e}"),
        }
    }

    pub fn copy_selected(&mut self, name: &str, settings: &mut LauncherSettings) {
        let Some(from) = self.selected().map(|i| i.id.clone()) else {
            return;
        };
        match instances::copy(&self.main_dir, &from, name) {
            Ok(made) => {
                settings.instance = Some(made.id);
                settings.save(&self.main_dir);
                self.reload(settings);
            }
            Err(e) => eprintln!("Instanz '{name}' konnte nicht kopiert werden: {e}"),
        }
    }

    pub fn rename_selected(&mut self, name: &str, settings: &mut LauncherSettings) {
        let Some(id) = self.selected().map(|i| i.id.clone()) else {
            return;
        };
        match instances::rename(&self.main_dir, &id, name) {
            Ok(made) => {
                settings.instance = Some(made.id);
                settings.save(&self.main_dir);
                self.reload(settings);
            }
            Err(e) => eprintln!("Instanz '{name}' konnte nicht umbenannt werden: {e}"),
        }
    }

    fn select(&mut self, index: usize, settings: &mut LauncherSettings) {
        self.list.select(Some(index));
        settings.instance = self.entries.get(index).map(|e| e.instance.id.clone());
        settings.save(&self.main_dir);
    }

    pub fn draw(&self, ui: &mut Ui, naming: bool) {
        self.list.draw(ui, |ui, index, rect| {
            let entry = &self.entries[index];
            let top = rect.y + (rect.h - 2.0 * LINE_HEIGHT) / 2.0;
            let name = if entry.instance.id == instances::MELON {
                format!(
                    "{}  {}",
                    entry.instance.name,
                    crate::cdn::discover::latest_game()
                )
            } else {
                entry.instance.name.clone()
            };
            let name = ui.ellipsize(&name, rect.w - 12.0);
            ui.text(&name, rect.x + 6.0, top, NAME);
            let engine = match entry.instance.engine {
                Engine::Melon => ui.tr("melon.launcher.engine.melon").to_owned(),
                Engine::Java => ui.lang.format(
                    "melon.launcher.engine.java",
                    &[entry
                        .instance
                        .java_version
                        .as_deref()
                        .unwrap_or(melon::home::VERSION)],
                ),
            };
            let line = if entry.instance.id == instances::MELON {
                format!("{engine}  ·  {}", crate::cdn::discover::latest_game())
            } else {
                match &entry.last_server {
                    Some(address) => format!("{engine}  ·  {address}"),
                    None => engine,
                }
            };
            let line = ui.ellipsize(&line, rect.w - 12.0);
            ui.text(&line, rect.x + 6.0, top + LINE_HEIGHT + 1.0, DIM);
        });
        if !naming {
            self.new.draw(ui);
        }
    }
}
