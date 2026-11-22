//! The right half: the selected instance. Head and actions on top, then two columns: what the
//! instance is (version, folders, statistics, sharing) and what is in it (mods, plugins).

use super::instance_grid::icon_key;
use super::kit::icons;
use super::kit::theme;
use super::kit::{BUTTON_H, Btn, Cx, GAP};
use super::shell::{Action, Purpose, Shell, Status};
use crate::catalog;
use crate::play::PlayState;
use crate::store::global::{self, Kind};
use crate::store::instances::{self, Engine, Instance, LOCAL_VERSION};
use super::kit::Rect;

const HEAD_H: f32 = 48.0;
const SIDE_W: f32 = 178.0;
const ROW: f32 = 18.0;
const MEMORY_STEPS: [u32; 6] = [1024, 2048, 3072, 4096, 6144, 8192];
/// Folders an instance has buttons for: text key and path under the instance.
const FOLDERS: [(&str, &str); 7] = [
    ("folder.instance", ""),
    ("folder.worlds", "saves"),
    ("folder.mods", "mods"),
    ("folder.plugins", "plugins"),
    ("folder.resourcepacks", "resourcepacks"),
    ("folder.logs", "logs"),
    ("folder.screenshots", "screenshots"),
];

fn kind_key(kind: Kind) -> &'static str {
    match kind {
        Kind::Saves => "folder.worlds",
        Kind::ResourcePacks => "folder.resourcepacks",
        Kind::Mods => "folder.mods",
        Kind::Plugins => "folder.plugins",
    }
}

impl Shell {
    pub(super) fn instance_panel(&mut self, cx: &mut Cx, area: Rect, status: &Status, out: &mut Vec<Action>) {
        cx.frame(area, theme::PANEL, theme::EDGE);
        let Some(instance) = self.selected().cloned() else {
            let rect = Rect::new(area.x + 16.0, area.y + 24.0, area.w - 32.0, 40.0);
            cx.text(cx.t("panel.none"), rect.x, rect.y, theme::TEXT);
            cx.paragraph(cx.t("panel.none_hint"), rect.x, rect.y + 14.0, rect.w, theme::DIM);
            let create = Rect::new(rect.x, rect.y + 44.0, 110.0, BUTTON_H);
            if cx.button(create, cx.t("instances.new"), Btn::PRIMARY.icon(icons::PLUS)) {
                self.open_prompt(cx, Purpose::NewInstance, "");
            }
            return;
        };
        self.panel_head(cx, Rect::new(area.x, area.y, area.w, HEAD_H), &instance, status, out);
        cx.fill(Rect::new(area.x + 1.0, area.y + HEAD_H, area.w - 2.0, 1.0), theme::EDGE);

        let top = area.y + HEAD_H + 7.0;
        let height = area.bottom() - 7.0 - top;
        let side_w = SIDE_W.min((area.w - 24.0) * 0.5);
        let side = Rect::new(area.x + 8.0, top, side_w, height);
        let content_x = side.right() + 8.0;
        let content = Rect::new(content_x, top, area.right() - 8.0 - content_x, height);
        self.side_column(cx, side, &instance, status);
        self.content_column(cx, content);
    }

    fn panel_head(&mut self, cx: &mut Cx, head: Rect, instance: &Instance, status: &Status, out: &mut Vec<Action>) {
        let icon = Rect::new(head.x + 8.0, head.y + 8.0, 32.0, 32.0);
        self.instance_icon(cx, instance, icon);
        if cx.hot(icon) {
            cx.pointer();
            cx.frame(icon, theme::alpha(theme::BG, 0.55), theme::ACCENT);
            cx.icon_centered(icons::FOLDER, icon, theme::TEXT);
        }
        cx.tooltip(icon, cx.t("panel.change_icon"));
        if cx.clicked(icon)
            && let Some(png) = melon_base::platform::pick_file(cx.t("panel.change_icon"), "png")
        {
            let set = instances::set_icon(instance, &png);
            if self.report(set).is_some() {
                self.textures.forget(&icon_key(instance));
            }
        }

        let running = status.running == Some(instance.id.as_str());
        let state = self.play_state(status);
        let action_w = 84.0;
        let action = Rect::new(head.right() - 8.0 - action_w, head.y + 12.0, action_w, 24.0);
        if running {
            if cx.button(action, cx.t("play.stop"), Btn::DANGER.icon(icons::STOP)) {
                out.push(Action::Stop);
            }
        } else {
            let (label, btn) = match &state {
                PlayState::Start => ("play.start", Btn::PRIMARY.icon(icons::PLAY)),
                PlayState::Download => ("play.download", Btn::PRIMARY.icon(icons::DOWNLOAD)),
                PlayState::Update => ("play.update", Btn::PRIMARY.icon(icons::UPDATE)),
                PlayState::Locked(_) => ("play.locked", Btn::NORMAL.icon(icons::LOCK).enabled(false)),
            };
            if cx.button(action, cx.t(label), btn) {
                self.press_play(&state, None, status, out);
            }
            if let PlayState::Locked(why) = &state {
                cx.tooltip(action, cx.t(why.key()));
            }
        }
        let mut right = action.x - GAP;
        if let Some(server) = self.disk.last_server.clone() {
            let label = cx.tf("panel.join", &[&server]);
            let w = (cx.text_width(&label) + 14.0).min(150.0);
            let join = Rect::new(right - w, action.y, w, 24.0);
            if join.x > icon.right() + 90.0 {
                let enabled = state == PlayState::Start && !running;
                if cx.button(join, &label, Btn::NORMAL.enabled(enabled)) {
                    self.press_play(&state, Some(server.clone()), status, out);
                }
                cx.tooltip(join, &cx.tf("panel.join_tip", &[&server]));
                right = join.x - GAP;
            }
        }

        let text_x = icon.right() + 8.0;
        let text_w = right - text_x;
        cx.label(&instance.name, text_x, head.y + 11.0, text_w, theme::TEXT);
        let (engine, color) = if instance.is_java() {
            let version = instance.java_version.as_deref().unwrap_or(melon_base::home::VERSION);
            (cx.tf("panel.java_line", &[version]), theme::JAVA)
        } else {
            (cx.tf("panel.melon_line", &[&self.version_label(cx, instance)]), theme::ACCENT)
        };
        let w = cx.small(&engine, text_x, head.y + 24.0, color);
        if let Some(group) = &instance.group {
            let group = cx.tf("panel.group_line", &[group]);
            cx.small_label(&group, text_x + w + 8.0, head.y + 24.0, text_w - w - 8.0, theme::DIM);
        }
    }

    /// "Latest (0.9.7)", a pinned version, or the local binary.
    fn version_label(&self, cx: &Cx, instance: &Instance) -> String {
        match instance.version.as_deref() {
            Some(LOCAL_VERSION) => cx.t("version.local").to_owned(),
            Some(pinned) => pinned.to_owned(),
            None => match catalog::latest_game().or_else(|| self.disk.installed.first().cloned()) {
                Some(latest) => cx.tf("version.latest_is", &[&latest]),
                None => cx.t("version.latest").to_owned(),
            },
        }
    }

    /// The narrow column: version or Java, folders, statistics, sharing, managing.
    fn side_column(&mut self, cx: &mut Cx, area: Rect, instance: &Instance, status: &Status) {
        let java_h = if instance.is_java() { 122.0 } else { 54.0 };
        let content_h = java_h + 8.0 + (11.0 + 4.0 * ROW + 8.0) + (11.0 + 36.0 + 8.0) + (11.0 + 6.0 * ROW + 8.0) + (11.0 + 2.0 * ROW + 4.0);
        let mut scroll = std::mem::take(&mut self.side_scroll);
        let mut y = scroll.begin(cx, area, content_h);
        let (x, w) = (area.x, area.w - 5.0);
        let locked = status.busy || status.running == Some(instance.id.as_str());

        y = if instance.is_java() {
            self.java_block(cx, x, y, w, instance, locked)
        } else {
            self.version_block(cx, x, y, w, instance, locked)
        };
        y += 8.0;

        y = cx.section(cx.t("panel.folders"), x, y, w);
        let half = ((w - 4.0) / 2.0).floor();
        for (i, (key, folder)) in FOLDERS.iter().enumerate() {
            let rect = Rect::new(x + (i % 2) as f32 * (half + 4.0), y + (i / 2) as f32 * ROW, half, ROW - 2.0);
            if cx.button(rect, cx.t(key), Btn::NORMAL.small()) {
                let path = instance.path.join(folder);
                let _ = std::fs::create_dir_all(&path);
                melon_base::platform::open_dir(&path);
            }
        }
        // TODO: shaderpacks: the client has no shader loader yet.
        let shaders = Rect::new(x + half + 4.0, y + 3.0 * ROW, half, ROW - 2.0);
        cx.button(shaders, cx.t("folder.shaders"), Btn::NORMAL.small().enabled(false));
        cx.tooltip(shaders, cx.t("common.soon"));
        y += 4.0 * ROW + 8.0;

        y = cx.section(cx.t("panel.stats"), x, y, w);
        let played = match instance.played_secs {
            0 => cx.t("stats.never").to_owned(),
            secs => cx.tr.span(secs),
        };
        let last = match instance.last_played {
            Some(at) => cx.tr.ago(crate::store::now_secs().saturating_sub(at)),
            None => cx.t("stats.never").to_owned(),
        };
        let server = self.disk.last_server.clone().unwrap_or_else(|| cx.t("stats.no_server").to_owned());
        for (i, (key, value)) in [("stats.playtime", played), ("stats.last_played", last), ("stats.last_server", server)]
            .iter()
            .enumerate()
        {
            let line_y = y + i as f32 * 12.0;
            let label_w = cx.small(cx.t(key), x, line_y, theme::DIM);
            let value = cx.fit(value, (w - label_w - 6.0) / super::kit::SMALL);
            let value_w = cx.text_width(&value) * super::kit::SMALL;
            cx.small(&value, x + w - value_w, line_y, theme::TEXT);
        }
        y += 36.0 + 8.0;

        y = self.sharing_block(cx, x, y, w, instance, locked);
        y += 8.0;

        y = cx.section(cx.t("panel.manage"), x, y, w);
        if cx.button(Rect::new(x, y, half, ROW - 2.0), cx.t("manage.rename"), Btn::NORMAL.small()) {
            self.open_prompt(cx, Purpose::RenameInstance, &instance.name);
        }
        if cx.button(Rect::new(x + half + 4.0, y, half, ROW - 2.0), cx.t("manage.group"), Btn::NORMAL.small()) {
            self.open_prompt(cx, Purpose::GroupInstance, instance.group.as_deref().unwrap_or(""));
        }
        if cx.button(Rect::new(x, y + ROW, half, ROW - 2.0), cx.t("manage.duplicate"), Btn::NORMAL.small()) {
            self.open_prompt(cx, Purpose::CopyInstance, "");
        }
        let what = format!("delete:{}", instance.id);
        let label = if self.confirming(&what) { "manage.delete_sure" } else { "manage.delete" };
        let delete = Rect::new(x + half + 4.0, y + ROW, half, ROW - 2.0);
        if cx.button(delete, cx.t(label), Btn::DANGER.small().enabled(!locked)) && self.confirmed(&what) {
            let deleted = instances::delete(&self.home, &instance.id);
            self.report(deleted);
            self.reload();
        }
        cx.tooltip(delete, cx.t("manage.delete_tip"));

        scroll.end(cx, area, content_h);
        self.side_scroll = scroll;
    }

    /// A Melon instance's client version and engine.
    fn version_block(&mut self, cx: &mut Cx, x: f32, y: f32, w: f32, instance: &Instance, locked: bool) -> f32 {
        let y = cx.section(cx.t("panel.version"), x, y, w);
        let mut versions = catalog::game_versions();
        for installed in &self.disk.installed {
            if !versions.contains(installed) {
                versions.push(installed.clone());
            }
        }
        versions.sort_by(|a, b| catalog::compare_versions(b, a));
        let label = self.version_label(cx, instance);
        let installed = self.disk.installed.clone();
        let (latest, local, tag) = (
            cx.t("version.latest").to_owned(),
            cx.t("version.local").to_owned(),
            cx.t("version.installed").to_owned(),
        );
        let current = match instance.version.as_deref() {
            None => Some(0),
            Some(LOCAL_VERSION) => Some(versions.len() + 1),
            Some(pinned) => versions.iter().position(|v| v == pinned).map(|i| i + 1),
        };
        let listed = versions.clone();
        let picked = cx.dropdown("version", Rect::new(x, y, w, ROW - 2.0), &label, !locked, move || {
            let mut items = vec![latest];
            items.extend(listed.iter().map(|v| match installed.contains(v) {
                true => format!("{v}  {tag}"),
                false => v.clone(),
            }));
            items.push(local);
            (items, current)
        });
        if let Some(index) = picked {
            let version = match index {
                0 => None,
                i if i <= versions.len() => Some(versions[i - 1].clone()),
                _ => Some(LOCAL_VERSION.to_owned()),
            };
            let set = instances::set_version(&self.home, &instance.id, version);
            self.report(set);
            self.reload();
        }
        self.engine_dropdown(cx, Rect::new(x, y + ROW + 2.0, w, ROW - 2.0), instance, locked);
        y + 2.0 * ROW + 2.0
    }

    fn engine_dropdown(&mut self, cx: &mut Cx, rect: Rect, instance: &Instance, locked: bool) {
        let engines = [cx.t("engine.melon").to_owned(), cx.t("engine.java").to_owned()];
        let current = usize::from(instance.is_java());
        let label = cx.tf("panel.engine", &[&engines[current]]);
        let picked = cx.dropdown("engine", rect, &label, !locked, move || (engines.to_vec(), Some(current)));
        if let Some(index) = picked.filter(|i| *i != current) {
            let engine = if index == 1 { Engine::Java } else { Engine::Melon };
            let set = instances::set_engine(&self.home, &instance.id, engine);
            self.report(set);
            self.reload();
        }
    }

    /// A Java instance's block, set apart in orange: Minecraft version, runtime, memory.
    fn java_block(&mut self, cx: &mut Cx, x: f32, y: f32, w: f32, instance: &Instance, locked: bool) -> f32 {
        let block = Rect::new(x, y, w, 122.0);
        cx.frame(block, theme::JAVA_SOFT, theme::alpha(theme::JAVA, 0.45));
        cx.fill(Rect::new(block.x, block.y, 2.0, block.h), theme::JAVA);
        let (ix, iw) = (x + 8.0, w - 14.0);
        let mut iy = y + 6.0;
        cx.small(&cx.t("java.title").to_uppercase(), ix, iy, theme::JAVA);
        iy += 11.0;
        let version = instance.java_version.as_deref().unwrap_or(melon_base::home::VERSION);
        cx.label(&cx.tf("java.minecraft", &[version]), ix, iy, iw, theme::TEXT);
        iy += 12.0;

        let runtimes = self.javas().to_vec();
        let current = match instance.java_path.as_deref() {
            None => Some(0),
            Some(path) => runtimes
                .iter()
                .position(|r| r.path.to_string_lossy() == path)
                .map(|i| i + 1),
        };
        let label = match (instance.java_path.as_deref(), current) {
            (None, _) => cx.t("java.default").to_owned(),
            (Some(_), Some(i)) => runtimes[i - 1].label(),
            (Some(path), None) => path.to_owned(),
        };
        let (default, browse) = (cx.t("java.default").to_owned(), cx.t("java.browse").to_owned());
        let labels: Vec<String> = runtimes.iter().map(|r| r.label()).collect();
        let picked = cx.dropdown("java-runtime", Rect::new(ix, iy, iw, ROW - 2.0), &label, !locked, move || {
            let mut items = vec![default];
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
                let set = instances::set_java_path(&self.home, &instance.id, path);
                self.report(set);
                self.reload();
            }
        }
        iy += ROW + 2.0;

        let default_memory = cx.tf("java.memory_default", &[&self.settings.memory_mb().to_string()]);
        let label = match instance.memory_mb {
            Some(mb) => cx.tf("java.memory", &[&mb.to_string()]),
            None => default_memory.clone(),
        };
        let current = match instance.memory_mb {
            None => Some(0),
            Some(mb) => MEMORY_STEPS.iter().position(|s| *s == mb).map(|i| i + 1),
        };
        let steps: Vec<String> = MEMORY_STEPS.iter().map(|mb| cx.tf("java.memory", &[&mb.to_string()])).collect();
        let picked = cx.dropdown("java-memory", Rect::new(ix, iy, iw, ROW - 2.0), &label, !locked, move || {
            let mut items = vec![default_memory];
            items.extend(steps);
            (items, current)
        });
        if let Some(index) = picked {
            let memory = index.checked_sub(1).map(|i| MEMORY_STEPS[i]);
            let set = instances::set_memory(&self.home, &instance.id, memory);
            self.report(set);
            self.reload();
        }
        iy += ROW + 2.0;

        self.engine_dropdown(cx, Rect::new(ix, iy, iw, ROW - 2.0), instance, locked);
        iy += ROW + 3.0;

        let (key, color) = match (self.disk.java_jar, self.disk.java_runtime) {
            (false, _) => ("java.no_jar", theme::WARN),
            (true, false) => ("java.no_runtime", theme::WARN),
            (true, true) => ("java.ready", theme::DIM),
        };
        cx.paragraph(cx.t(key), ix, iy, iw, color);
        block.bottom()
    }

    /// Which folders come from the global reference, and copying from it once.
    fn sharing_block(&mut self, cx: &mut Cx, x: f32, y: f32, w: f32, instance: &Instance, locked: bool) -> f32 {
        let mut y = cx.section(cx.t("panel.sharing"), x, y, w);
        let copy_w = 40.0;
        let row = |y: f32| (Rect::new(x, y, w - copy_w - 30.0, ROW), Rect::new(x + w - copy_w - 24.0, y + 1.0, copy_w, ROW - 2.0), Rect::new(x + w - 20.0, y, 20.0, ROW));
        for kind in Kind::ALL {
            let (label, copy, toggle) = row(y);
            let linked = instance.shared.get(kind);
            let color = if linked { theme::ACCENT } else { theme::TEXT };
            if linked {
                cx.icon(icons::LINK, label.x, label.y + 4.0, theme::ACCENT);
            }
            let indent = if linked { 12.0 } else { 0.0 };
            cx.label(cx.t(kind_key(kind)), label.x + indent, label.y + 5.0, label.w - indent, color);
            if cx.button(copy, cx.t("share.copy"), Btn::GHOST.small().enabled(!linked && !locked)) {
                let copied = global::copy_into(&self.home, &instance.path, kind);
                if self.report(copied).is_some() {
                    self.notify(cx.tf("share.copied", &[cx.t(kind_key(kind))]));
                    self.content.set_instance(Some(instance.path.clone()));
                }
            }
            cx.tooltip(copy, cx.t("share.copy_tip"));
            if cx.toggle(toggle, linked, !locked) {
                let set = instances::set_shared(&self.home, &instance.id, kind, !linked);
                self.report(set);
                self.reload();
            }
            cx.tooltip(toggle, cx.t("share.link_tip"));
            y += ROW;
        }
        let (label, copy, toggle) = row(y);
        let linked = instance.use_global_options;
        cx.label(cx.t("share.options"), label.x, label.y + 5.0, label.w, if linked { theme::ACCENT } else { theme::TEXT });
        if cx.button(copy, cx.t("share.copy"), Btn::GHOST.small().enabled(!linked && !locked)) {
            let copied = global::copy_options_into(&self.home, &instance.path);
            if self.report(copied).is_some() {
                self.notify(cx.tf("share.copied", &[cx.t("share.options")]));
            }
        }
        cx.tooltip(copy, cx.t("share.copy_tip"));
        if cx.toggle(toggle, linked, !locked) {
            let set = instances::set_use_global_options(&self.home, &instance.id, !linked);
            self.report(set);
            self.reload();
        }
        cx.tooltip(toggle, cx.t("share.options_tip"));
        y += ROW;
        // TODO: shaderpacks: share them like the other folders once the client loads shaders.
        let (label, _, toggle) = row(y);
        cx.label(cx.t("folder.shaders"), label.x, label.y + 5.0, label.w, theme::FAINT);
        cx.toggle(toggle, false, false);
        cx.tooltip(Rect::new(x, y, w, ROW), cx.t("common.soon"));
        y + ROW
    }
}
