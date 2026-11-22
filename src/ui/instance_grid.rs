//! The left half: instances as tiles, row by row, in groups.

use super::kit::icons;
use super::kit::theme;
use super::kit::{Btn, Cx, SMALL};
use super::shell::{Action, Purpose, Shell, Status};
use super::textures;
use crate::play::PlayState;
use crate::store::instances::Instance;
use super::kit::Rect;

const TILE_W: f32 = 74.0;
const TILE_H: f32 = 56.0;
const ICON: f32 = 28.0;
const GROUP_H: f32 = 16.0;
const HEADER_H: f32 = 22.0;

/// The texture key of an instance's icon.
pub fn icon_key(instance: &Instance) -> String {
    format!("instance:{}", instance.id)
}

impl Shell {
    /// An instance's icon: its `icon.png`, else its initial; Java ones carry an orange foot.
    pub fn instance_icon(&mut self, cx: &mut Cx, instance: &Instance, rect: Rect) {
        let path = instance.icon_path();
        let texture = self
            .textures
            .get(&icon_key(instance), || textures::png_file(&path));
        match texture {
            Some(texture) => cx.image(texture, rect),
            None => cx.initial(rect, &instance.id, &instance.name),
        }
        if instance.is_java() {
            cx.fill(Rect::new(rect.x, rect.bottom() - 3.0, rect.w, 3.0), theme::JAVA);
        }
    }

    pub(super) fn instance_grid(&mut self, cx: &mut Cx, area: Rect, status: &Status, out: &mut Vec<Action>) {
        cx.frame(area, theme::PANEL, theme::EDGE);
        cx.small(&cx.t("instances.title").to_uppercase(), area.x + 8.0, area.y + 8.0, theme::DIM);
        let add = Rect::new(area.right() - 62.0, area.y + 3.0, 58.0, 16.0);
        if cx.button(add, cx.t("instances.new"), Btn::GHOST.icon(icons::PLUS)) {
            self.open_prompt(cx, Purpose::NewInstance, "");
        }
        cx.fill(Rect::new(area.x + 1.0, area.y + HEADER_H - 1.0, area.w - 2.0, 1.0), theme::EDGE);

        let list = Rect::new(area.x + 1.0, area.y + HEADER_H, area.w - 2.0, area.h - HEADER_H - 1.0);
        if self.instances.is_empty() {
            let y = list.y + 20.0;
            cx.paragraph(cx.t("instances.empty"), list.x + 10.0, y, list.w - 20.0, theme::DIM);
            return;
        }

        let mut groups: Vec<(Option<String>, Vec<Instance>)> = Vec::new();
        for instance in &self.instances {
            match groups.last_mut() {
                Some((group, members)) if *group == instance.group => members.push(instance.clone()),
                _ => groups.push((instance.group.clone(), vec![instance.clone()])),
            }
        }
        let named = groups.iter().any(|(group, _)| group.is_some());
        let columns = ((list.w - 8.0) / TILE_W).floor().max(1.0) as usize;
        let folded = |shell: &Shell, group: &Option<String>| {
            let key = group.clone().unwrap_or_default();
            shell.settings.collapsed_groups.contains(&key)
        };
        let content_h: f32 = groups
            .iter()
            .map(|(group, members)| {
                let header = if named { GROUP_H } else { 0.0 };
                let rows = if folded(self, group) { 0 } else { members.len().div_ceil(columns) };
                header + rows as f32 * TILE_H + 4.0
            })
            .sum::<f32>()
            + 4.0;

        let mut scroll = std::mem::take(&mut self.grid_scroll);
        let mut y = scroll.begin(cx, list, content_h) + 4.0;
        let state = self.play_state(status);
        for (group, members) in groups {
            let is_folded = folded(self, &group);
            if named {
                let header = Rect::new(list.x + 2.0, y, list.w - 6.0, GROUP_H);
                let hot = cx.hot(header);
                if hot {
                    cx.pointer();
                }
                let color = if hot { theme::TEXT } else { theme::DIM };
                let icon = if is_folded { icons::CHEVRON_RIGHT } else { icons::CHEVRON_DOWN };
                cx.icon(icon, header.x + 5.0, header.y + if is_folded { 4.0 } else { 6.0 }, color);
                let name = match &group {
                    Some(name) => name.clone(),
                    None => cx.t("instances.ungrouped").to_owned(),
                };
                let w = cx.text(&name, header.x + 16.0, header.y + 4.0, color);
                let count = members.len().to_string();
                cx.small(&count, header.x + 16.0 + w + 5.0, header.y + 5.0, theme::FAINT);
                if cx.clicked(header) {
                    let key = group.clone().unwrap_or_default();
                    if is_folded {
                        self.settings.collapsed_groups.retain(|g| g != &key);
                    } else {
                        self.settings.collapsed_groups.push(key);
                    }
                    self.settings.save(&self.home);
                }
                y += GROUP_H;
            }
            if !is_folded {
                for (i, instance) in members.iter().enumerate() {
                    let tile = Rect::new(
                        list.x + 4.0 + (i % columns) as f32 * TILE_W,
                        y + (i / columns) as f32 * TILE_H,
                        TILE_W - 2.0,
                        TILE_H - 2.0,
                    );
                    self.tile(cx, tile, instance, &state, status, out);
                }
                y += members.len().div_ceil(columns) as f32 * TILE_H;
            }
            y += 4.0;
        }
        scroll.end(cx, list, content_h);
        self.grid_scroll = scroll;
    }

    fn tile(
        &mut self,
        cx: &mut Cx,
        tile: Rect,
        instance: &Instance,
        state: &PlayState,
        status: &Status,
        out: &mut Vec<Action>,
    ) {
        let selected = self.settings.instance.as_deref() == Some(instance.id.as_str());
        let hot = cx.hot(tile);
        if hot {
            cx.pointer();
        }
        if selected {
            cx.frame(tile, theme::ACCENT_SOFT, theme::ACCENT);
        } else if hot {
            cx.frame(tile, theme::HOVER, theme::EDGE);
        }
        let icon = Rect::new(tile.x + ((tile.w - ICON) / 2.0).floor(), tile.y + 6.0, ICON, ICON);
        self.instance_icon(cx, instance, icon);
        if status.running == Some(instance.id.as_str()) {
            cx.fill(Rect::new(icon.right() - 4.0, icon.y - 2.0, 6.0, 6.0), theme::BG);
            cx.fill(Rect::new(icon.right() - 3.0, icon.y - 1.0, 4.0, 4.0), theme::ACCENT);
        }
        let name = cx.fit(&instance.name, (tile.w - 4.0) / SMALL);
        let w = cx.text_width(&name) * SMALL;
        let color = if selected || hot { theme::TEXT } else { theme::DIM };
        cx.small(&name, tile.x + (tile.w - w) / 2.0, icon.bottom() + 6.0, color);
        if name != instance.name {
            cx.tooltip(tile, &instance.name);
        }
        if cx.clicked(tile) {
            let again = selected && cx.input.double_click;
            self.select(&instance.id);
            if again {
                self.press_play(state, None, status, out);
            }
        }
    }
}
