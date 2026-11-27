//! The content column of an instance: its mods and plugins, each with icon, name and
//! description.
// TODO: remote mod/modpack download: a third source of rows next to the two folders.

use super::kit::icons;
use super::kit::theme;
use super::kit::{BUTTON_H, Btn, Cx, Rect, Scroll};
use super::shell::Shell;
use super::textures::{self, Textures};
use crate::mods;
use crate::toolchain::Rust;
use std::path::{Path, PathBuf};

const ROW_H: f32 = 34.0;
const ICON: f32 = 24.0;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Tab {
    #[default]
    Mods,
    Plugins,
}

/// One row: a mod zip or a plugin file.
struct Entry {
    /// Mod id; none for a plugin or a zip that cannot be read.
    id: Option<String>,
    name: String,
    description: String,
    /// A mod's version, or a plugin's file name.
    detail: String,
    /// A mod's kind of code and side, as its manifest names them.
    kinds: Option<(String, String)>,
    error: Option<String>,
    /// The zip an icon may be in.
    icon_zip: Option<PathBuf>,
}

#[derive(Default)]
pub struct ContentList {
    game_dir: Option<PathBuf>,
    tab: Tab,
    state: mods::State,
    mods: Vec<Entry>,
    plugins: Vec<Entry>,
    scroll: Scroll,
}

fn file_name(path: &Path) -> String {
    path.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

impl ContentList {
    /// Another instance was selected.
    pub fn set_instance(&mut self, game_dir: Option<PathBuf>) {
        if self.game_dir != game_dir {
            self.scroll.reset();
        }
        self.game_dir = game_dir;
        self.reload();
    }

    /// Reads the folders again.
    fn reload(&mut self) {
        self.mods.clear();
        self.plugins.clear();
        let Some(game_dir) = self.game_dir.clone() else {
            return;
        };
        self.state = mods::State::load(&game_dir);
        for (path, manifest) in mods::list(&game_dir.join("mods")) {
            self.mods.push(match manifest {
                Ok(manifest) => Entry {
                    id: Some(manifest.id.clone()),
                    name: manifest.display_name().to_owned(),
                    description: manifest.description.clone().unwrap_or_default(),
                    detail: manifest.version.clone(),
                    kinds: Some((manifest.code, manifest.side)),
                    error: None,
                    icon_zip: Some(path),
                },
                Err(error) => Entry {
                    name: file_name(&path),
                    id: None,
                    description: String::new(),
                    detail: String::new(),
                    kinds: None,
                    error: Some(error),
                    icon_zip: None,
                },
            });
        }
        let mut files: Vec<PathBuf> = std::fs::read_dir(game_dir.join("plugins"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .collect();
        files.sort();
        for path in files {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            let parent = self.mods.iter().find(|m| m.id.as_deref() == Some(stem.as_str()));
            self.plugins.push(Entry {
                name: parent.map_or_else(|| stem.clone(), |m| m.name.clone()),
                description: parent.map_or_else(String::new, |m| m.description.clone()),
                detail: file_name(&path),
                kinds: None,
                icon_zip: parent.and_then(|m| m.icon_zip.clone()),
                id: None,
                error: None,
            });
        }
    }

    /// Switched-on mods shipped as source code, which the game builds with Rust.
    pub fn source_mods(&self) -> usize {
        self.mods
            .iter()
            .filter(|entry| entry.kinds.as_ref().is_some_and(|(code, _)| code == "source"))
            .filter(|entry| entry.id.as_deref().is_some_and(|id| self.state.is_enabled(id)))
            .count()
    }

    fn toggle(&mut self, id: &str) -> Result<(), String> {
        let Some(game_dir) = &self.game_dir else {
            return Ok(());
        };
        let on = !self.state.is_enabled(id);
        self.state.set_enabled(id, on);
        self.state.save(game_dir).map_err(|e| format!("mods.json: {e}"))
    }
}

fn row_icon(cx: &mut Cx, textures: &mut Textures, entry: &Entry, rect: Rect) {
    let texture = entry.icon_zip.as_ref().and_then(|zip| {
        let key = format!("mod:{}", zip.display());
        textures.get(&key, || textures::png_in_zip(zip, mods::ICON))
    });
    match texture {
        Some(texture) => cx.image(texture, rect),
        None => cx.initial(rect, &entry.name, &entry.name),
    }
}

impl Shell {
    /// The content column: tabs, the folder button, and the rows.
    pub(super) fn content_column(&mut self, cx: &mut Cx, area: Rect) {
        let rust_missing = self.rust_missing();
        let no_toolchain = rust_missing && matches!(self.rust.get(), Some(Rust::NoToolchain));
        let mut list = std::mem::take(&mut self.content);
        let tab_w = 68.0;
        for (i, (tab, key)) in [(Tab::Mods, "content.mods"), (Tab::Plugins, "content.plugins")]
            .into_iter()
            .enumerate()
        {
            let rect = Rect::new(area.x + i as f32 * (tab_w + 2.0), area.y, tab_w, BUTTON_H);
            let count = match tab {
                Tab::Mods => list.mods.len(),
                Tab::Plugins => list.plugins.len(),
            };
            let label = format!("{} {count}", cx.t(key));
            if cx.button(rect, &label, Btn::GHOST.selected(list.tab == tab)) && list.tab != tab {
                list.tab = tab;
                list.scroll.reset();
            }
        }
        let folder = match list.tab {
            Tab::Mods => "mods",
            Tab::Plugins => "plugins",
        };
        let open = Rect::new(area.right() - BUTTON_H, area.y, BUTTON_H, BUTTON_H);
        if cx.button(open, "", Btn::NORMAL.icon(icons::FOLDER))
            && let Some(dir) = &list.game_dir
        {
            melon_base::platform::open_dir(&dir.join(folder));
        }
        cx.tooltip(open, cx.t("content.open_folder"));
        let reload = Rect::new(open.x - 2.0 - BUTTON_H, area.y, BUTTON_H, BUTTON_H);
        if cx.button(reload, "", Btn::GHOST.icon(icons::UPDATE)) {
            list.reload();
        }
        cx.tooltip(reload, cx.t("content.reload"));

        let mut top_y = area.y + BUTTON_H + 4.0;
        if rust_missing && list.tab == Tab::Mods {
            let strip = Rect::new(area.x, top_y, area.w, 28.0);
            cx.frame(strip, theme::alpha(theme::WARN, 0.08), theme::alpha(theme::WARN, 0.5));
            let guide_w = 76.0;
            let text_w = strip.w - guide_w - 14.0;
            cx.small_label(cx.t("content.rust_needed"), strip.x + 6.0, strip.y + 6.0, text_w, theme::WARN);
            let why = if no_toolchain { "content.rust_no_toolchain" } else { "content.rust_missing" };
            cx.small_label(cx.t(why), strip.x + 6.0, strip.y + 16.0, text_w, theme::DIM);
            let guide = Rect::new(strip.right() - guide_w - 5.0, strip.y + 5.0, guide_w, BUTTON_H);
            if cx.button(guide, cx.t("content.rust_guide"), Btn::NORMAL.small()) {
                self.rust_guide = true;
            }
            top_y = strip.bottom() + 4.0;
        }
        let rows = Rect::new(area.x, top_y, area.w, area.bottom() - top_y);
        cx.frame(rows, theme::FIELD, theme::EDGE);
        let inner = Rect::new(rows.x + 1.0, rows.y + 1.0, rows.w - 2.0, rows.h - 2.0);
        let is_mods = list.tab == Tab::Mods;
        let count = if is_mods { list.mods.len() } else { list.plugins.len() };
        if count == 0 {
            let key = if is_mods { "content.no_mods" } else { "content.no_plugins" };
            cx.paragraph(cx.t(key), inner.x + 8.0, inner.y + 10.0, inner.w - 16.0, theme::DIM);
            self.content = list;
            return;
        }
        let content_h = count as f32 * ROW_H;
        let mut scroll = std::mem::take(&mut list.scroll);
        let top = scroll.begin(cx, inner, content_h);
        let mut flipped = None;
        let entries = if is_mods { &list.mods } else { &list.plugins };
        for (i, entry) in entries.iter().enumerate() {
            let row = Rect::new(inner.x, top + i as f32 * ROW_H, inner.w - 3.0, ROW_H);
            if row.bottom() < inner.y || row.y > inner.bottom() {
                continue;
            }
            if cx.hot(row) {
                cx.fill(row, theme::alpha(theme::HOVER, 0.5));
            }
            cx.fill(Rect::new(row.x + 6.0, row.bottom() - 1.0, row.w - 12.0, 1.0), theme::alpha(theme::EDGE, 0.6));
            let icon = Rect::new(row.x + 5.0, row.y + (ROW_H - ICON) / 2.0, ICON, ICON);
            row_icon(cx, &mut self.textures, entry, icon);
            let enabled = entry.id.as_deref().is_none_or(|id| list.state.is_enabled(id));
            let toggle_w = if entry.id.is_some() { 26.0 } else { 0.0 };
            let text_x = icon.right() + 6.0;
            let text_w = row.right() - text_x - toggle_w - 4.0;
            let name_color = match (&entry.error, enabled) {
                (Some(_), _) => theme::DANGER,
                (None, true) => theme::TEXT,
                (None, false) => theme::FAINT,
            };
            cx.label(&entry.name, text_x, row.y + 4.0, text_w, name_color);
            let description = entry.error.as_deref().unwrap_or(&entry.description);
            cx.small_label(description, text_x, row.y + 15.0, text_w, theme::DIM);
            let detail = match &entry.kinds {
                Some((code, side)) => {
                    let code = cx.tr.get(&format!("mod.code.{code}")).to_owned();
                    let side = cx.tr.get(&format!("mod.side.{side}")).to_owned();
                    format!("{} - {code} - {side}", entry.detail)
                }
                None => entry.detail.clone(),
            };
            cx.small_label(&detail, text_x, row.y + 24.0, text_w, theme::FAINT);
            if let Some(id) = &entry.id {
                let toggle = Rect::new(row.right() - 24.0, row.y, 20.0, ROW_H);
                if cx.toggle(toggle, enabled, true) {
                    flipped = Some(id.clone());
                }
            }
        }
        scroll.end(cx, inner, content_h);
        list.scroll = scroll;
        if let Some(id) = flipped
            && let Err(error) = list.toggle(&id)
        {
            self.notify(error);
        }
        self.content = list;
    }
}
