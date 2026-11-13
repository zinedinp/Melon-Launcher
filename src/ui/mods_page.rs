//! The launcher's mods page: `.zip` files in the selected instance's

use super::window::{BUTTON_H, GAP, ROW_W};
use melon::gui::screen::CursorKind;
use melon::gui::text::font::LINE_HEIGHT;
use melon::gui::widgets::Ui;
use melon::gui::widgets::button::Button;
use melon::gui::widgets::scroll_list::{ListClick, ScrollList};
use melon::instances;
use pumpkin_mod::{CodeKind, Manifest, ModsState, Side, list_mods, save_state};
use std::path::{Path, PathBuf};

const ROW_H: f32 = 28.0;
const NAME: [f32; 4] = [0.95, 0.95, 0.95, 1.0];
const DIM: [f32; 4] = [0.6, 0.6, 0.6, 1.0];
const OFF: [f32; 4] = [0.92, 0.5, 0.45, 1.0];

struct Entry {
    path: PathBuf,
    manifest: Result<Manifest, String>,
}

pub struct ModsPage {
    game_dir: PathBuf,
    state: ModsState,
    entries: Vec<Entry>,
    list: ScrollList,
    open: Button,
}

impl ModsPage {
    pub fn new(game_dir: &Path) -> Self {
        let mut page = Self {
            game_dir: game_dir.to_path_buf(),
            state: ModsState::default(),
            entries: Vec::new(),
            list: ScrollList::new(300.0, ROW_H),
            open: Button::sized("melon.launcher.mods_open", 0.0, BUTTON_H),
        };
        page.reload();
        page
    }

    /// Another instance was selected.
    pub fn set_instance(&mut self, game_dir: &Path) {
        self.game_dir = game_dir.to_path_buf();
        self.reload();
    }

    pub fn reload(&mut self) {
        instances::ensure_dirs(&self.game_dir);
        self.state = load_state(&self.game_dir);
        self.entries = list_mods(&instances::mods_dir(&self.game_dir))
            .into_iter()
            .map(|(path, manifest)| Entry { path, manifest })
            .collect();
        self.list.set_len(self.entries.len());
        if self.entries.is_empty() {
            self.list.select(None);
        } else if self.list.selected().is_none_or(|i| i >= self.entries.len()) {
            self.list.select(Some(0));
        }
    }

    pub fn resize_in(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let row_w = w.min(ROW_W).max(1.0);
        let left = x + ((w - row_w) / 2.0).floor();
        self.list.set_row_width(row_w);
        self.open.w = row_w;
        self.open.x = left;
        self.open.y = y + 4.0;
        let list_y = self.open.y + BUTTON_H + GAP;
        let end = (y + h - 4.0).max(list_y + ROW_H);
        self.list.set_area(left, list_y, end, row_w);
    }

    pub fn hover(&mut self, x: f32, y: f32, active: bool) {
        let (x, y) = if active {
            (x, y)
        } else {
            (f32::NEG_INFINITY, f32::NEG_INFINITY)
        };
        self.list.on_mouse_move(x, y);
        self.open.update_hover(x, y);
    }

    pub fn cursor(&self) -> CursorKind {
        if self.open.enabled && self.open.hovered {
            CursorKind::Pointer
        } else {
            self.list.cursor().unwrap_or(CursorKind::Pointer)
        }
    }

    pub fn release(&mut self) {
        self.list.on_mouse_up();
    }

    pub fn scroll(&mut self, delta: f32) {
        self.list.on_scroll(delta);
    }

    pub fn click(&mut self, x: f32, y: f32) {
        if self.open.clicked(x, y) {
            let dir = instances::mods_dir(&self.game_dir);
            if let Err(e) = open::that(&dir) {
                eprintln!("mods-Ordner konnte nicht geöffnet werden: {e}");
            }
            return;
        }
        match self.list.on_click(x, y) {
            ListClick::Row { index, .. } => {
                self.list.select(Some(index));
                self.toggle_at(index);
            }
            ListClick::Scrollbar | ListClick::None => {}
        }
    }

    fn toggle_at(&mut self, index: usize) {
        let Some(id) = self
            .entries
            .get(index)
            .and_then(|e| e.manifest.as_ref().ok().map(|m| m.id.clone()))
        else {
            return;
        };
        let on = !self.state.is_enabled(&id);
        self.state.set_enabled(&id, on);
        if let Err(e) = save_state(&self.game_dir, &self.state) {
            eprintln!("mods.json konnte nicht gespeichert werden: {e}");
        }
    }

    pub fn draw(&self, ui: &mut Ui) {
        self.open.draw(ui);
        if self.entries.is_empty() {
            let note = ui.tr("melon.launcher.mods_empty");
            let tw = ui.text_width(note);
            ui.text(
                note,
                self.open.x + ((self.open.w - tw) / 2.0).floor(),
                self.list.row_rect(0).y + 4.0,
                DIM,
            );
        }
        self.list.draw(ui, |ui, index, rect| {
            let entry = &self.entries[index];
            let top = rect.y + (rect.h - 2.0 * LINE_HEIGHT) / 2.0;
            match &entry.manifest {
                Ok(manifest) => {
                    let on = self.state.is_enabled(&manifest.id);
                    let flag = ui.tr(if on { "options.on" } else { "options.off" });
                    let flag_w = ui.text_width(flag) + 10.0;
                    let name =
                        ui.ellipsize(manifest.display_name(), (rect.w - 12.0 - flag_w).max(8.0));
                    ui.text(&name, rect.x + 6.0, top, if on { NAME } else { OFF });
                    ui.text(
                        flag,
                        rect.right() - flag_w,
                        top,
                        if on { NAME } else { OFF },
                    );
                    let code = code_key(manifest.code);
                    let side = side_key(manifest.side);
                    let line = format!(
                        "{}  ·  {}  ·  {}",
                        manifest.version,
                        ui.tr(code),
                        ui.tr(side)
                    );
                    let line = ui.ellipsize(&line, rect.w - 12.0);
                    ui.text(&line, rect.x + 6.0, top + LINE_HEIGHT + 1.0, DIM);
                }
                Err(err) => {
                    let stem = entry.path.file_name().unwrap_or_default().to_string_lossy();
                    ui.text(&stem, rect.x + 6.0, top, OFF);
                    ui.text(err, rect.x + 6.0, top + LINE_HEIGHT + 1.0, DIM);
                }
            }
        });
    }
}

fn load_state(game_dir: &Path) -> ModsState {
    let path = game_dir.join("mods.json");
    melon::json_file::load(&path).unwrap_or_default()
}

fn code_key(code: CodeKind) -> &'static str {
    match code {
        CodeKind::None => "melon.launcher.mod.code.none",
        CodeKind::Wasm => "melon.launcher.mod.code.wasm",
        CodeKind::Source => "melon.launcher.mod.code.source",
        CodeKind::Prebuilt => "melon.launcher.mod.code.prebuilt",
    }
}

fn side_key(side: Side) -> &'static str {
    match side {
        Side::Client => "melon.launcher.mod.side.client",
        Side::Server => "melon.launcher.mod.side.server",
        Side::Both => "melon.launcher.mod.side.both",
    }
}
