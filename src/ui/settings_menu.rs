//! The launcher's settings: language, Java, folders, assets, shared folders, cleaning up.

use super::kit::theme;
use super::kit::{BUTTON_H, Btn, Cx, GAP, Scroll};
use super::shell::{Action, Purpose, Shell, Status};
use crate::catalog;
use crate::cdn;
use crate::lang::LANGUAGES;
use crate::play::Install;
use crate::store::cleanup::{self, Item};
use crate::store::shared;
use crate::store::settings::Settings;
use super::jar::JarSource;
use super::kit::Rect;
use super::kit::TextField;
use melon_base::prepare;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

const TAB_W: f32 = 96.0;
const ROW: f32 = 20.0;
const MEMORY_STEPS: [u32; 6] = [1024, 2048, 3072, 4096, 6144, 8192];
const SHARED_ROW: f32 = 26.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    General,
    Java,
    Folders,
    Assets,
    Shared,
    Cleanup,
    Advanced,
}

const TABS: [(Tab, &str); 7] = [
    (Tab::General, "settings.general"),
    (Tab::Java, "settings.java"),
    (Tab::Folders, "settings.folders"),
    (Tab::Assets, "settings.assets"),
    (Tab::Shared, "settings.shared_tab"),
    (Tab::Cleanup, "settings.cleanup"),
    (Tab::Advanced, "settings.advanced"),
];

pub struct SettingsMenu {
    tab: Tab,
    jvm_args: TextField,
    assets_path: TextField,
    /// The client jar assets are extracted from.
    pub jar: JarSource,
    /// What cleaning up found; `None` before the first look.
    found_items: Option<Vec<Item>>,
    scan_rx: Option<Receiver<Vec<Item>>>,
    cleanup_scroll: Scroll,
    shared_scroll: Scroll,
}

impl SettingsMenu {
    pub fn new(home: &Path, settings: &Settings) -> Self {
        let mut jvm_args = TextField::new(String::new());
        jvm_args.set_value(settings.jvm_args.as_deref().unwrap_or(""));
        let mut assets_path = TextField::new(String::new());
        assets_path.set_value(&settings.assets_dir(home).display().to_string());
        Self {
            tab: Tab::General,
            jvm_args,
            assets_path,
            jar: JarSource::scan(home),
            found_items: None,
            scan_rx: None,
            cleanup_scroll: Scroll::default(),
            shared_scroll: Scroll::default(),
        }
    }

    /// Opens on the shared folders next time.
    pub fn show_shared(&mut self) {
        self.tab = Tab::Shared;
    }

    fn poll(&mut self) {
        self.jar.poll();
        if let Some(items) = self.scan_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.scan_rx = None;
            self.found_items = Some(items);
        }
    }

    fn start_scan(&mut self, home: PathBuf, settings: Settings) {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let latest = catalog::latest_game();
            let _ = tx.send(cleanup::scan(&home, &settings, latest.as_deref()));
        });
        self.scan_rx = Some(rx);
    }
}

fn kind_key(kind: cleanup::Kind) -> &'static str {
    match kind {
        cleanup::Kind::UnusedVersion => "cleanup.version",
        cleanup::Kind::OldAssets => "cleanup.assets",
        cleanup::Kind::HeadsCache => "cleanup.heads",
        cleanup::Kind::ModCache => "cleanup.mod_cache",
        cleanup::Kind::Natives => "cleanup.natives",
        cleanup::Kind::Partial => "cleanup.partial",
    }
}

/// A row with a label, a path and an Open button. Returns the y below it.
fn folder_row(cx: &mut Cx, x: f32, y: f32, w: f32, label: &str, path: &Path) -> f32 {
    cx.label(label, x, y + 2.0, w - 70.0, theme::TEXT);
    cx.small_label(&path.display().to_string(), x, y + 13.0, w - 70.0, theme::DIM);
    if cx.button(Rect::new(x + w - 60.0, y + 2.0, 60.0, BUTTON_H), cx.t("common.open"), Btn::NORMAL) {
        let _ = std::fs::create_dir_all(path);
        melon_base::platform::open_dir(path);
    }
    y + 26.0
}

impl Shell {
    pub(super) fn settings_page(&mut self, cx: &mut Cx, sheet: Rect, status: &Status, out: &mut Vec<Action>) {
        let body = self.sheet(cx, sheet, cx.t("settings.title"));
        self.settings_menu.poll();
        cx.fill(Rect::new(body.x + TAB_W, body.y, 1.0, body.h), theme::EDGE);
        for (i, (tab, key)) in TABS.iter().enumerate() {
            let rect = Rect::new(body.x + 6.0, body.y + 8.0 + i as f32 * 20.0, TAB_W - 12.0, 18.0);
            let selected = self.settings_menu.tab == *tab;
            if cx.button(rect, cx.t(key), Btn::GHOST.selected(selected)) {
                self.settings_menu.tab = *tab;
            }
        }
        let page = Rect::new(body.x + TAB_W + 12.0, body.y + 10.0, body.w - TAB_W - 24.0, body.h - 18.0);
        match self.settings_menu.tab {
            Tab::General => self.settings_general(cx, page, out),
            Tab::Java => self.settings_java(cx, page),
            Tab::Folders => self.settings_folders(cx, page),
            Tab::Assets => self.settings_assets(cx, page, status, out),
            Tab::Shared => self.settings_shared(cx, page, status),
            Tab::Cleanup => self.settings_cleanup(cx, page, status),
            Tab::Advanced => self.settings_advanced(cx, page),
        }
    }

    fn save_settings(&mut self) {
        self.settings.save(&self.home);
    }

    fn settings_general(&mut self, cx: &mut Cx, page: Rect, out: &mut Vec<Action>) {
        let (x, w) = (page.x, page.w.min(300.0));
        let mut y = cx.section(cx.t("settings.language"), x, page.y, w);
        let current = LANGUAGES.iter().position(|l| *l == cx.tr.language());
        let label = current.map_or("", |i| LANGUAGES[i].name());
        let picked = cx.dropdown("language", Rect::new(x, y, 160.0, 16.0), label, true, move || {
            (LANGUAGES.iter().map(|l| l.name().to_owned()).collect(), current)
        });
        if let Some(index) = picked.filter(|i| Some(*i) != current) {
            self.settings.language = Some(LANGUAGES[index].code().to_owned());
            self.save_settings();
            out.push(Action::LanguageChanged);
        }
        y += 16.0 + 12.0;

        y = cx.section(cx.t("settings.while_playing"), x, y, w);
        let row = Rect::new(x, y, w, ROW);
        if cx.toggle_row(row, cx.t("settings.keep_open"), self.settings.keep_open, true) {
            self.settings.keep_open = !self.settings.keep_open;
            self.save_settings();
        }
        y += ROW - 6.0;
        y += cx.paragraph(cx.t("settings.keep_open_note"), x, y + 2.0, w - 30.0, theme::DIM) + 8.0;
        let row = Rect::new(x, y, w, ROW);
        let reopen = !self.settings.exit_after_game;
        if cx.toggle_row(row, cx.t("settings.reopen"), reopen, !self.settings.keep_open) {
            self.settings.exit_after_game = reopen;
            self.save_settings();
        }
        y += ROW + 10.0;

        y = cx.section(cx.t("settings.updates"), x, y, w);
        let (text, color) = match catalog::launcher_update() {
            Some(version) => (cx.tf("info.launcher_update", &[&version]), theme::ACCENT),
            None => (cx.tf("info.version", &[env!("CARGO_PKG_VERSION")]), theme::DIM),
        };
        cx.label(&text, x, y + 2.0, w, color);
        // TODO: launcher self-update: download and swap the launcher binary.
    }

    fn settings_java(&mut self, cx: &mut Cx, page: Rect) {
        let (x, w) = (page.x, page.w.min(340.0));
        let mut y = cx.section(cx.t("settings.java_runtime"), x, page.y, w);
        let runtimes = self.javas().to_vec();
        let current = match self.settings.java_path.as_deref() {
            None => Some(0),
            Some(path) => runtimes.iter().position(|r| r.path.to_string_lossy() == path).map(|i| i + 1),
        };
        let label = match (self.settings.java_path.as_deref(), current) {
            (None, _) => cx.t("java.auto").to_owned(),
            (Some(_), Some(i)) => runtimes[i - 1].label(),
            (Some(path), None) => path.to_owned(),
        };
        let (auto, browse) = (cx.t("java.auto").to_owned(), cx.t("java.browse").to_owned());
        let labels: Vec<String> = runtimes.iter().map(|r| r.label()).collect();
        let picked = cx.dropdown("settings-java", Rect::new(x, y, w, 16.0), &label, true, move || {
            let mut items = vec![auto];
            items.extend(labels);
            items.push(browse);
            (items, current)
        });
        if let Some(index) = picked {
            let path = match index {
                0 => Some(None),
                i if i <= runtimes.len() => Some(Some(runtimes[i - 1].path.display().to_string())),
                _ => melon_base::platform::pick_file(cx.t("java.browse"), if cfg!(windows) { "exe" } else { "*" })
                    .map(|p| Some(p.display().to_string())),
            };
            if let Some(path) = path {
                self.settings.java_path = path;
                self.save_settings();
                self.recheck();
            }
        }
        y += 20.0;
        let found = cx.tf("java.found", &[&runtimes.len().to_string()]);
        cx.small_label(&found, x, y + 5.0, w - 150.0, theme::DIM);
        if cx.button(Rect::new(x + w - 70.0, y, 70.0, BUTTON_H), cx.t("java.rescan"), Btn::NORMAL) {
            self.rescan_javas();
        }
        // TODO: download a Mojang Java runtime and the vanilla libraries.
        let download = Rect::new(x + w - 70.0 - GAP - 110.0, y, 110.0, BUTTON_H);
        cx.button(download, cx.t("java.download"), Btn::NORMAL.enabled(false));
        cx.tooltip(download, cx.t("common.soon"));
        y += BUTTON_H + 12.0;

        y = cx.section(cx.t("settings.java_memory"), x, y, w);
        let memory = self.settings.memory_mb();
        let label = cx.tf("java.memory", &[&memory.to_string()]);
        let current = MEMORY_STEPS.iter().position(|s| *s == memory);
        let steps: Vec<String> = MEMORY_STEPS.iter().map(|mb| cx.tf("java.memory", &[&mb.to_string()])).collect();
        let picked = cx.dropdown("settings-memory", Rect::new(x, y, 160.0, 16.0), &label, true, move || (steps, current));
        if let Some(index) = picked {
            self.settings.java_memory_mb = Some(MEMORY_STEPS[index]);
            self.save_settings();
        }
        y += 16.0 + 12.0;

        y = cx.section(cx.t("settings.jvm_args"), x, y, w);
        let mut edit = std::mem::replace(&mut self.settings_menu.jvm_args, TextField::new(""));
        if cx.field(&mut edit, Rect::new(x + 1.0, y + 1.0, w - 2.0, 16.0)).changed {
            let value = edit.value();
            self.settings.jvm_args = Some(value.trim().to_owned()).filter(|v| !v.is_empty());
            self.save_settings();
        }
        self.settings_menu.jvm_args = edit;
        cx.paragraph(cx.t("settings.java_note"), x, y + 26.0, w, theme::DIM);
    }

    fn settings_folders(&mut self, cx: &mut Cx, page: Rect) {
        let (x, w) = (page.x, page.w.min(380.0));
        let mut y = cx.section(cx.t("settings.folders"), x, page.y, w);
        let home = self.home.clone();
        y = folder_row(cx, x, y, w, cx.t("folders.melon"), &home);
        y = folder_row(cx, x, y, w, cx.t("folders.instances"), &home.join("instances"));
        y = folder_row(cx, x, y, w, cx.t("folders.versions"), &cdn::versions_dir(&home));
        y = folder_row(cx, x, y, w, cx.t("folders.shared"), &shared::root(&home));
        folder_row(cx, x, y, w, cx.t("folders.assets"), &self.settings.assets_dir(&home));
    }

    fn settings_assets(&mut self, cx: &mut Cx, page: Rect, status: &Status, out: &mut Vec<Action>) {
        let (x, w) = (page.x, page.w.min(380.0));
        let home = self.home.clone();
        let mut y = cx.section(cx.t("settings.assets"), x, page.y, w);
        let play_assets = self.settings.melon_play_assets(&home);
        let stale = self.disk.assets_ready && prepare::needs_extract(&home, &play_assets);
        let (key, color) = match (self.disk.assets_ready, stale) {
            (false, _) => ("assets.missing", theme::WARN),
            (true, true) => ("assets.stale", theme::WARN),
            (true, false) => ("assets.ready", theme::ACCENT),
        };
        cx.label(&cx.tf(key, &[melon_base::home::VERSION]), x, y + 1.0, w, color);
        y += 16.0;

        y = cx.section(cx.t("assets.source"), x, y, w);
        let missing = cx.t("assets.no_jar");
        y = self.settings_menu.jar.draw(cx, x, y, w, missing);
        let can_extract = !status.busy && !self.settings_menu.jar.rejected();
        let extract_key = if self.disk.assets_ready { "assets.extract_again" } else { "assets.extract" };
        if cx.button(Rect::new(x, y, 130.0, BUTTON_H), cx.t(extract_key), Btn::PRIMARY.enabled(can_extract)) {
            let install = Install {
                assets: true,
                version: None,
            };
            out.push(Action::Install(install, self.settings_menu.jar.found.clone()));
        }
        if cx.button(Rect::new(x + 130.0 + GAP, y, 100.0, BUTTON_H), cx.t("assets.choose"), Btn::NORMAL.enabled(!status.busy))
            && let Some(jar) = melon_base::platform::pick_file(cx.t("assets.choose"), "jar")
        {
            self.settings_menu.jar.set(Some(prepare::from_jar(jar)));
        }
        y += BUTTON_H + 12.0;

        y = cx.section(cx.t("folders.assets"), x, y, w);
        let mut edit = std::mem::replace(&mut self.settings_menu.assets_path, TextField::new(""));
        let event = cx.field(&mut edit, Rect::new(x + 1.0, y + 1.0, w - 2.0 - 2.0 * (60.0 + GAP), 16.0));
        let mut chosen = event.submitted.then(|| PathBuf::from(edit.value().trim()));
        if cx.button(Rect::new(x + w - 120.0 - GAP, y, 60.0, BUTTON_H), cx.t("common.browse"), Btn::NORMAL) {
            chosen = melon_base::platform::pick_folder(cx.t("folders.assets"));
        }
        if cx.button(Rect::new(x + w - 60.0, y, 60.0, BUTTON_H), cx.t("common.open"), Btn::NORMAL) {
            melon_base::platform::open_dir(&self.settings.assets_dir(&home));
        }
        if let Some(path) = chosen.filter(|p| !p.as_os_str().is_empty()) {
            self.settings.set_assets_path(&home, &path);
            edit.set_value(&self.settings.assets_dir(&home).display().to_string());
            self.recheck();
        }
        self.settings_menu.assets_path = edit;
        cx.paragraph(cx.t("assets.path_note"), x, y + 26.0, w, theme::DIM);
    }

    fn settings_shared(&mut self, cx: &mut Cx, page: Rect, status: &Status) {
        let (x, w) = (page.x, page.w.min(400.0));
        let mut y = cx.section(cx.t("settings.shared"), x, page.y, w);
        y += cx.paragraph(cx.t("shared.note"), x, y, w, theme::DIM) + 8.0;
        let free = !status.busy && status.running.is_none();
        if cx.button(Rect::new(x, y, 150.0, BUTTON_H), cx.t("shared.new"), Btn::PRIMARY.enabled(free)) {
            self.open_prompt(cx, Purpose::NewShared, "");
        }
        let root = shared::root(&self.home);
        if cx.button(Rect::new(x + w - 90.0, y, 90.0, BUTTON_H), cx.t("shared.open_all"), Btn::NORMAL) {
            let _ = std::fs::create_dir_all(&root);
            melon_base::platform::open_dir(&root);
        }
        y += BUTTON_H + 8.0;

        let sets = self.disk.shared.clone();
        let list = Rect::new(x, y, w, (sets.len() as f32 * SHARED_ROW + 2.0).min(page.bottom() - y));
        cx.frame(list, theme::FIELD, theme::EDGE);
        let inner = super::kit::inset(list, 1.0);
        let content_h = sets.len() as f32 * SHARED_ROW;
        let mut scroll = std::mem::take(&mut self.settings_menu.shared_scroll);
        let top = scroll.begin(cx, inner, content_h);
        let mut removed = None;
        for (i, name) in sets.iter().enumerate() {
            let row = Rect::new(inner.x, top + i as f32 * SHARED_ROW, inner.w - 3.0, SHARED_ROW);
            if i > 0 {
                cx.fill(Rect::new(row.x, row.y, row.w, 1.0), theme::EDGE);
            }
            let users = self.instances.iter().filter(|instance| instance.shared_folder == *name).count();
            let global = name == shared::GLOBAL;
            let buttons_w = 3.0 * 56.0 + 2.0 * 4.0;
            let text_w = row.w - buttons_w - 14.0;
            cx.label(name, row.x + 6.0, row.y + 4.0, text_w, if global { theme::ACCENT } else { theme::TEXT });
            let mut detail = cx.tf("shared.used_by", &[&users.to_string()]);
            if shared::dir(&self.home, name).join(shared::OPTIONS).is_file() {
                detail.push_str(cx.t("shared.has_options"));
            }
            cx.small_label(&detail, row.x + 6.0, row.y + 15.0, text_w, theme::DIM);

            let button = |n: f32| Rect::new(row.right() - 4.0 - (3.0 - n) * 56.0 - (2.0 - n) * 4.0, row.y + 4.0, 56.0, BUTTON_H);
            if cx.button(button(0.0), cx.t("common.open"), Btn::NORMAL.small()) {
                let path = shared::dir(&self.home, name);
                let _ = std::fs::create_dir_all(&path);
                melon_base::platform::open_dir(&path);
            }
            let rename = button(1.0);
            if cx.button(rename, cx.t("manage.rename"), Btn::NORMAL.small().enabled(free && !global)) {
                self.open_prompt(cx, Purpose::RenameShared, name);
            }
            let delete = button(2.0);
            let what = format!("shared:{name}");
            let label = if self.confirming(&what) { "manage.delete_sure" } else { "manage.delete" };
            if cx.button(delete, cx.t(label), Btn::DANGER.small().enabled(free && !global && users == 0))
                && self.confirmed(&what)
            {
                removed = Some(name.clone());
            }
            if global {
                cx.tooltip(rename, cx.t("shared.global_fixed"));
                cx.tooltip(delete, cx.t("shared.global_fixed"));
            } else if users > 0 {
                cx.tooltip(delete, cx.t("shared.in_use"));
            } else {
                cx.tooltip(delete, cx.t("shared.delete_tip"));
            }
        }
        scroll.end(cx, inner, content_h);
        self.settings_menu.shared_scroll = scroll;
        if let Some(name) = removed {
            let done = shared::remove(&self.home, &name);
            self.report(done);
            self.recheck();
        }
    }

    fn settings_cleanup(&mut self, cx: &mut Cx, page: Rect, status: &Status) {
        let (x, w) = (page.x, page.w.min(400.0));
        let mut y = cx.section(cx.t("settings.cleanup"), x, page.y, w);
        y += cx.paragraph(cx.t("cleanup.note"), x, y, w, theme::DIM) + 6.0;
        let scanning = self.settings_menu.scan_rx.is_some();
        let scan_key = if scanning { "cleanup.scanning" } else { "cleanup.scan" };
        if cx.button(Rect::new(x, y, 150.0, BUTTON_H), cx.t(scan_key), Btn::NORMAL.enabled(!scanning && !status.busy)) {
            self.settings_menu.start_scan(self.home.clone(), self.settings.clone());
        }
        let Some(items) = self.settings_menu.found_items.clone() else {
            return;
        };
        let total: u64 = items.iter().map(|i| i.bytes).sum();
        let blocked = status.busy || status.running.is_some();
        if !items.is_empty() {
            let label = if self.confirming("cleanup:all") {
                cx.t("manage.delete_sure").to_owned()
            } else {
                cx.tf("cleanup.delete_all", &[&cleanup::human_size(total)])
            };
            let all = Rect::new(x + w - 170.0, y, 170.0, BUTTON_H);
            if cx.button(all, &label, Btn::DANGER.enabled(!blocked)) && self.confirmed("cleanup:all") {
                for item in &items {
                    let removed = cleanup::remove(item);
                    self.report(removed);
                }
                self.settings_menu.found_items = Some(Vec::new());
                self.recheck();
                return;
            }
        }
        y += BUTTON_H + 8.0;
        if items.is_empty() {
            cx.label(cx.t("cleanup.nothing"), x, y, w, theme::ACCENT);
            return;
        }
        let list = Rect::new(x, y, w, page.bottom() - y);
        cx.frame(list, theme::FIELD, theme::EDGE);
        let inner = super::kit::inset(list, 1.0);
        let content_h = items.len() as f32 * 24.0;
        let mut scroll = std::mem::take(&mut self.settings_menu.cleanup_scroll);
        let top = scroll.begin(cx, inner, content_h);
        let mut removed = None;
        for (i, item) in items.iter().enumerate() {
            let row = Rect::new(inner.x, top + i as f32 * 24.0, inner.w - 3.0, 24.0);
            let title = match item.label.is_empty() {
                true => cx.t(kind_key(item.kind)).to_owned(),
                false => format!("{}: {}", cx.t(kind_key(item.kind)), item.label),
            };
            cx.label(&title, row.x + 6.0, row.y + 3.0, row.w - 140.0, theme::TEXT);
            cx.small_label(&item.path.display().to_string(), row.x + 6.0, row.y + 14.0, row.w - 140.0, theme::FAINT);
            let size = cleanup::human_size(item.bytes);
            let size_w = cx.text_width(&size);
            cx.text(&size, row.right() - 66.0 - size_w, row.y + 8.0, theme::DIM);
            let delete = Rect::new(row.right() - 58.0, row.y + 3.0, 54.0, BUTTON_H);
            if cx.button(delete, cx.t("manage.delete"), Btn::DANGER.enabled(!blocked)) {
                removed = Some(i);
            }
        }
        scroll.end(cx, inner, content_h);
        self.settings_menu.cleanup_scroll = scroll;
        if let Some(index) = removed {
            let done = cleanup::remove(&items[index]);
            if self.report(done).is_some()
                && let Some(items) = &mut self.settings_menu.found_items
            {
                items.remove(index);
            }
            self.recheck();
        }
    }

    fn settings_advanced(&mut self, cx: &mut Cx, page: Rect) {
        let (x, w) = (page.x, page.w.min(320.0));
        let mut y = cx.section(cx.t("settings.advanced"), x, page.y, w);
        if cx.toggle_row(Rect::new(x, y, w, ROW), cx.t("advanced.validation"), self.settings.validation, true) {
            self.settings.validation = !self.settings.validation;
            self.save_settings();
        }
        y += ROW - 6.0;
        y += cx.paragraph(cx.t("advanced.validation_note"), x, y + 2.0, w - 30.0, theme::DIM) + 8.0;
        if cx.toggle_row(Rect::new(x, y, w, ROW), cx.t("advanced.probe"), self.settings.probe, true) {
            self.settings.probe = !self.settings.probe;
            self.save_settings();
        }
        y += ROW - 6.0;
        cx.paragraph(cx.t("advanced.probe_note"), x, y + 2.0, w - 30.0, theme::DIM);
    }
}
