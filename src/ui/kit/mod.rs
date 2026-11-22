//! The launcher's widgets. Each one lays itself out, reacts and draws in a single call, once
//! a frame, so a view reads top to bottom like the screen it makes.

pub mod field;
pub mod icons;
pub mod paint;
pub mod theme;

use crate::lang::Strings;
use icons::Icon;
use melon_base::gfx::TextureId;
use paint::Painter;

pub use field::{Key, TextField};
pub use paint::Rect;
use theme::Color;

/// Height of a line of text.
pub const LINE: f32 = 9.0;
/// Height of the glyphs, for centring text in a box.
const GLYPH: f32 = 8.0;
/// Scale of secondary text.
pub const SMALL: f32 = 0.75;
pub const BUTTON_H: f32 = 18.0;
pub const GAP: f32 = 6.0;
const POPUP_ROW: f32 = 16.0;

/// The pointer a widget asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorKind {
    #[default]
    Default,
    Pointer,
    Text,
}

/// What happened since the last frame.
#[derive(Default)]
pub struct Input {
    pub mouse: (f32, f32),
    /// Where the left button went down.
    pub click: Option<(f32, f32)>,
    pub double_click: bool,
    /// Wheel notches, up positive.
    pub scroll: f32,
    pub keys: Vec<Key>,
}

impl Input {
    /// Forgets the events a frame has seen.
    pub fn end_frame(&mut self) {
        self.click = None;
        self.double_click = false;
        self.scroll = 0.0;
        self.keys.clear();
    }

}

/// How a button looks.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Normal,
    /// The one thing to press.
    Primary,
    Danger,
    /// No plate until hovered.
    Ghost,
}

#[derive(Clone, Copy)]
pub struct Btn {
    pub look: Look,
    pub enabled: bool,
    pub icon: Option<Icon>,
    /// Drawn pressed in: the chosen one of a set.
    pub selected: bool,
    /// The label in the smaller type.
    pub small: bool,
}

impl Btn {
    pub const NORMAL: Btn = Btn::of(Look::Normal);
    pub const PRIMARY: Btn = Btn::of(Look::Primary);
    pub const DANGER: Btn = Btn::of(Look::Danger);
    pub const GHOST: Btn = Btn::of(Look::Ghost);

    const fn of(look: Look) -> Btn {
        Btn {
            look,
            enabled: true,
            icon: None,
            selected: false,
            small: false,
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Btn {
        self.enabled = enabled;
        self
    }

    pub fn icon(mut self, icon: Icon) -> Btn {
        self.icon = Some(icon);
        self
    }

    pub fn small(mut self) -> Btn {
        self.small = true;
        self
    }

    pub fn selected(mut self, selected: bool) -> Btn {
        self.selected = selected;
        self
    }
}

/// The open dropdown list, drawn over everything at the end of a frame.
#[derive(Default)]
pub struct Popup {
    open: Option<Open>,
    picked: Option<(u64, usize)>,
    opened_now: bool,
}

struct Open {
    id: u64,
    anchor: Rect,
    items: Vec<String>,
    current: Option<usize>,
}

impl Popup {
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub fn close(&mut self) {
        self.open = None;
    }
}

fn id_of(name: &str) -> u64 {
    name.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}

/// What a text field did this frame.
#[derive(Default)]
pub struct FieldEvent {
    pub changed: bool,
    /// Enter was pressed in it.
    pub submitted: bool,
}

/// A frame's painter and input.
pub struct Cx<'a, 'u> {
    pub ui: &'a mut Painter<'u>,
    pub tr: &'a Strings,
    pub input: &'a Input,
    /// False for what lies under a menu, a popup or a modal: it draws but does not react.
    pub active: bool,
    pub cursor: CursorKind,
    pub popup: &'a mut Popup,
    clip: Option<Rect>,
    tip: Option<String>,
}

impl<'a, 'u> Cx<'a, 'u> {
    pub fn new(ui: &'a mut Painter<'u>, tr: &'a Strings, input: &'a Input, popup: &'a mut Popup) -> Self {
        popup.opened_now = false;
        Self {
            ui,
            tr,
            input,
            active: true,
            cursor: CursorKind::Default,
            popup,
            clip: None,
            tip: None,
        }
    }

    pub fn width(&self) -> f32 {
        self.ui.width
    }

    pub fn height(&self) -> f32 {
        self.ui.height
    }

    /// The text for a key in the launcher's language.
    pub fn t(&self, key: &'static str) -> &'a str {
        self.tr.get(key)
    }

    pub fn tf(&self, key: &str, args: &[&str]) -> String {
        self.tr.format(key, args)
    }

    fn reachable(&self, rect: Rect, at: (f32, f32)) -> bool {
        rect.contains(at.0, at.1) && self.clip.is_none_or(|clip| clip.contains(at.0, at.1))
    }

    /// The pointer is over `rect` and nothing covers it.
    pub fn hot(&self, rect: Rect) -> bool {
        self.active && self.reachable(rect, self.input.mouse)
    }

    /// The left button went down on `rect` this frame.
    pub fn clicked(&self, rect: Rect) -> bool {
        self.active && self.input.click.is_some_and(|at| self.reachable(rect, at))
    }

    /// The left button went down somewhere this frame, on this layer.
    pub fn any_click(&self) -> bool {
        self.active && self.input.click.is_some()
    }

    pub fn pointer(&mut self) {
        self.cursor = CursorKind::Pointer;
    }

    pub fn fill(&mut self, rect: Rect, color: Color) {
        self.ui.fill(rect, color);
    }

    /// A filled box with a one pixel edge.
    pub fn frame(&mut self, rect: Rect, fill: Color, edge: Color) {
        self.ui.panel(rect, fill, edge);
    }

    pub fn text(&mut self, text: &str, x: f32, y: f32, color: Color) -> f32 {
        self.ui.text(text, x.round(), y.round(), color)
    }

    /// Secondary text, a notch smaller.
    pub fn small(&mut self, text: &str, x: f32, y: f32, color: Color) -> f32 {
        self.ui.text_scaled(text, x.round(), y.round(), color, SMALL)
    }

    pub fn text_width(&self, text: &str) -> f32 {
        self.ui.text_width(text)
    }

    /// `text` cut with `...` to fit `width`.
    pub fn fit(&self, text: &str, width: f32) -> String {
        self.ui.ellipsize(text, width.max(8.0))
    }

    /// Text cut to the width, one line.
    pub fn label(&mut self, text: &str, x: f32, y: f32, width: f32, color: Color) {
        let text = self.fit(text, width);
        self.text(&text, x, y, color);
    }

    pub fn small_label(&mut self, text: &str, x: f32, y: f32, width: f32, color: Color) {
        let text = self.fit(text, width / SMALL);
        self.small(&text, x, y, color);
    }

    /// Text wrapped to the width. Returns the height used.
    pub fn paragraph(&mut self, text: &str, x: f32, y: f32, width: f32, color: Color) -> f32 {
        let lines = self.ui.wrap(text, width / SMALL);
        for (i, line) in lines.iter().enumerate() {
            self.small(line, x, y + i as f32 * LINE * SMALL, color);
        }
        lines.len().max(1) as f32 * LINE * SMALL
    }

    pub fn centered(&mut self, text: &str, rect: Rect, color: Color) {
        let w = self.text_width(text);
        self.text(text, rect.x + (rect.w - w) / 2.0, rect.y + (rect.h - GLYPH) / 2.0, color);
    }

    pub fn icon(&mut self, icon: Icon, x: f32, y: f32, color: Color) {
        let (x, y) = (x.round(), y.round());
        for (row, line) in icon.iter().enumerate() {
            let bytes = line.as_bytes();
            let mut col = 0;
            while col < bytes.len() {
                if bytes[col] != b'#' {
                    col += 1;
                    continue;
                }
                let start = col;
                while col < bytes.len() && bytes[col] == b'#' {
                    col += 1;
                }
                self.ui.fill(
                    Rect::new(x + start as f32, y + row as f32, (col - start) as f32, 1.0),
                    color,
                );
            }
        }
    }

    pub fn icon_centered(&mut self, icon: Icon, rect: Rect, color: Color) {
        let (w, h) = icons::size(icon);
        self.icon(icon, rect.x + (rect.w - w) / 2.0, rect.y + (rect.h - h) / 2.0, color);
    }

    /// A texture over a rectangle.
    pub fn image(&mut self, texture: TextureId, rect: Rect) {
        self.ui.sprite(texture, rect);
    }

    /// An initial on a muted tile: stands in for an icon.
    pub fn initial(&mut self, rect: Rect, key: &str, name: &str) {
        self.fill(rect, theme::tile_color(key));
        let initial = name
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default();
        self.centered(&initial, rect, [1.0; 4]);
    }

    /// A button. True when it was pressed.
    pub fn button(&mut self, rect: Rect, label: &str, btn: Btn) -> bool {
        let hot = btn.enabled && self.hot(rect);
        if hot {
            self.pointer();
        }
        let (fill, edge, text) = match (btn.look, btn.enabled, hot) {
            (_, false, _) => (theme::PANEL, theme::EDGE, theme::FAINT),
            (Look::Primary, _, false) => (theme::ACCENT_FILL, theme::ACCENT_HOVER, [1.0; 4]),
            (Look::Primary, _, true) => (theme::ACCENT_HOVER, theme::ACCENT, [1.0; 4]),
            (Look::Danger, _, false) => (theme::RAISED, theme::EDGE, theme::DANGER),
            (Look::Danger, _, true) => (theme::DANGER_HOVER, theme::DANGER, [1.0; 4]),
            (Look::Ghost, _, false) => (theme::alpha(theme::RAISED, 0.0), theme::alpha(theme::EDGE, 0.0), theme::DIM),
            (Look::Ghost, _, true) => (theme::HOVER, theme::EDGE, theme::TEXT),
            (Look::Normal, _, false) => (theme::RAISED, theme::EDGE, theme::TEXT),
            (Look::Normal, _, true) => (theme::HOVER, theme::FAINT, theme::TEXT),
        };
        let (fill, edge) = if btn.selected && btn.enabled {
            (theme::ACCENT_SOFT, theme::ACCENT)
        } else {
            (fill, edge)
        };
        self.frame(rect, fill, edge);
        let icon_w = btn.icon.map_or(0.0, |icon| icons::size(icon).0);
        let gap = if btn.icon.is_some() && !label.is_empty() { 4.0 } else { 0.0 };
        let full = label;
        let scale = if btn.small { SMALL } else { 1.0 };
        let label = self.fit(label, (rect.w - 6.0 - icon_w - gap) / scale);
        if label != full {
            self.tooltip(rect, full);
        }
        let label_w = if label.is_empty() { 0.0 } else { self.text_width(&label) * scale };
        let mut x = rect.x + ((rect.w - icon_w - gap - label_w) / 2.0).floor();
        if let Some(icon) = btn.icon {
            let (_, h) = icons::size(icon);
            self.icon(icon, x, rect.y + (rect.h - h) / 2.0, text);
            x += icon_w + gap;
        }
        if !label.is_empty() {
            let y = rect.y + (rect.h - GLYPH * scale) / 2.0;
            self.ui.text_scaled(&label, x.round(), y.round(), text, scale);
        }
        btn.enabled && self.clicked(rect)
    }

    /// A switch. True when it was flipped.
    pub fn toggle(&mut self, rect: Rect, on: bool, enabled: bool) -> bool {
        let hot = enabled && self.hot(rect);
        if hot {
            self.pointer();
        }
        let track = Rect::new(rect.x, rect.y + (rect.h - 10.0) / 2.0, 20.0, 10.0);
        let (fill, knob) = match (enabled, on) {
            (false, _) => (theme::PANEL, theme::FAINT),
            (true, true) => (theme::ACCENT_FILL, [1.0; 4]),
            (true, false) => (theme::FIELD, theme::DIM),
        };
        self.frame(track, fill, if hot { theme::FAINT } else { theme::EDGE });
        let knob_x = if on { track.right() - 9.0 } else { track.x + 1.0 };
        self.fill(Rect::new(knob_x, track.y + 1.0, 8.0, 8.0), knob);
        enabled && self.clicked(rect)
    }

    /// A row with a label on the left and a switch on the right. True when flipped.
    pub fn toggle_row(&mut self, rect: Rect, label: &str, on: bool, enabled: bool) -> bool {
        let color = if enabled { theme::TEXT } else { theme::FAINT };
        self.label(label, rect.x, rect.y + (rect.h - GLYPH) / 2.0, rect.w - 28.0, color);
        self.toggle(Rect::new(rect.right() - 20.0, rect.y, 20.0, rect.h), on, enabled)
    }

    /// A dropdown showing `label`. Returns the index picked from `items`.
    pub fn dropdown(
        &mut self,
        name: &str,
        rect: Rect,
        label: &str,
        enabled: bool,
        items: impl FnOnce() -> (Vec<String>, Option<usize>),
    ) -> Option<usize> {
        let hot = enabled && self.hot(rect);
        let edge = if self.dropdown_open(name) {
            theme::ACCENT
        } else if hot {
            theme::FAINT
        } else {
            theme::EDGE
        };
        self.frame(rect, if enabled { theme::FIELD } else { theme::PANEL }, edge);
        let color = if enabled { theme::TEXT } else { theme::FAINT };
        self.label(label, rect.x + 5.0, rect.y + (rect.h - GLYPH) / 2.0, rect.w - 20.0, color);
        self.icon(
            icons::CHEVRON_DOWN,
            rect.right() - 12.0,
            rect.y + (rect.h - 5.0) / 2.0,
            if enabled { theme::DIM } else { theme::FAINT },
        );
        self.dropdown_over(name, rect, enabled, items)
    }

    /// Whether the dropdown `name` has its list open.
    pub fn dropdown_open(&self, name: &str) -> bool {
        let id = id_of(name);
        self.popup.open.as_ref().is_some_and(|open| open.id == id)
    }

    /// A dropdown's behaviour over a face the caller draws: opens the list on a press of
    /// `rect`, and returns the index picked from it.
    pub fn dropdown_over(
        &mut self,
        name: &str,
        rect: Rect,
        enabled: bool,
        items: impl FnOnce() -> (Vec<String>, Option<usize>),
    ) -> Option<usize> {
        let id = id_of(name);
        if enabled && self.hot(rect) {
            self.pointer();
        }
        if enabled && self.clicked(rect) {
            let (items, current) = items();
            self.popup.open = Some(Open {
                id,
                anchor: rect,
                items,
                current,
            });
            self.popup.opened_now = true;
        }
        match self.popup.picked {
            Some((picked, index)) if picked == id => {
                self.popup.picked = None;
                Some(index)
            }
            _ => None,
        }
    }

    /// A small heading with a rule under it. Returns the y below it.
    pub fn section(&mut self, title: &str, x: f32, y: f32, width: f32) -> f32 {
        let title = title.to_uppercase();
        let w = self.small(&title, x, y, theme::DIM);
        self.fill(
            Rect::new(x + w + 5.0, (y + 3.0).round(), (width - w - 5.0).max(0.0), 1.0),
            theme::EDGE,
        );
        y + 11.0
    }

    /// A one-line text field.
    pub fn field(&mut self, edit: &mut TextField, rect: Rect) -> FieldEvent {
        edit.set_rect(rect);
        let mut event = FieldEvent::default();
        if self.hot(rect) {
            self.cursor = CursorKind::Text;
        }
        if self.any_click()
            && let Some((x, y)) = self.input.click
        {
            edit.click(x, y);
        }
        if self.active && edit.focused() {
            for key in &self.input.keys {
                if matches!(key, Key::Enter) {
                    event.submitted = true;
                } else if edit.handle(key) {
                    event.changed = true;
                }
            }
        }
        self.frame(
            Rect::new(rect.x - 1.0, rect.y - 1.0, rect.w + 2.0, rect.h + 2.0),
            theme::FIELD,
            if edit.focused() { theme::ACCENT } else { theme::EDGE },
        );
        edit.draw(self.ui);
        event
    }

    /// Text shown beside the pointer while it is over `rect`.
    pub fn tooltip(&mut self, rect: Rect, text: &str) {
        if self.active && self.reachable(rect, self.input.mouse) {
            self.tip = Some(text.to_owned());
        }
    }

    /// Clips drawing and input to `rect` until [`Cx::unclip`].
    pub fn clip(&mut self, rect: Rect) {
        self.ui.clip(rect);
        self.clip = Some(rect);
    }

    pub fn unclip(&mut self) {
        self.ui.unclip();
        self.clip = None;
    }

    /// Dims everything drawn so far.
    pub fn shade(&mut self) {
        let all = Rect::new(0.0, 0.0, self.ui.width, self.ui.height);
        self.fill(all, theme::SHADE);
    }

    /// The open dropdown list and the tooltip: call last, over everything.
    pub fn finish(&mut self) {
        self.draw_popup();
        if let Some(tip) = self.tip.take() {
            let w = self.text_width(&tip) * SMALL + 8.0;
            let (mx, my) = self.input.mouse;
            let x = (mx + 10.0).min(self.ui.width - w - 2.0).max(2.0);
            let y = (my + 12.0).min(self.ui.height - 14.0);
            let rect = Rect::new(x.round(), y.round(), w.round(), 12.0);
            self.frame(rect, theme::FIELD, theme::FAINT);
            self.small(&tip, rect.x + 4.0, rect.y + 3.0, theme::TEXT);
        }
    }

    fn draw_popup(&mut self) {
        let Some(open) = &self.popup.open else {
            return;
        };
        let rows = open.items.len().max(1) as f32;
        let widest = open
            .items
            .iter()
            .map(|item| self.ui.text_width(item))
            .fold(0.0f32, f32::max);
        let w = open.anchor.w.max(widest + 12.0).min(self.ui.width - 8.0);
        let h = rows * POPUP_ROW + 2.0;
        let below = open.anchor.bottom() + 1.0;
        let y = if below + h <= self.ui.height - 2.0 {
            below
        } else {
            (open.anchor.y - h - 1.0).max(2.0)
        };
        let x = open.anchor.x.min(self.ui.width - w - 4.0).max(4.0);
        let panel = Rect::new(x, y, w, h);
        let (mouse, click) = (self.input.mouse, self.input.click);
        let (items, current) = (open.items.clone(), open.current);
        self.ui.fill(Rect::new(x + 2.0, y + 2.0, w, h), [0.0, 0.0, 0.0, 0.35]);
        self.ui.panel(panel, theme::RAISED, theme::FAINT);
        let mut picked = None;
        for (i, item) in items.iter().enumerate() {
            let row = Rect::new(x + 1.0, y + 1.0 + i as f32 * POPUP_ROW, w - 2.0, POPUP_ROW);
            let hot = row.contains(mouse.0, mouse.1);
            if hot {
                self.ui.fill(row, theme::HOVER);
                self.cursor = CursorKind::Pointer;
            }
            if current == Some(i) {
                self.ui.fill(Rect::new(row.x, row.y, 2.0, row.h), theme::ACCENT);
            }
            let text = self.ui.ellipsize(item, row.w - 10.0);
            let color = if current == Some(i) { theme::ACCENT } else { theme::TEXT };
            self.ui.text(&text, row.x + 6.0, row.y + (POPUP_ROW - GLYPH) / 2.0, color);
            if click.is_some_and(|(cx, cy)| row.contains(cx, cy)) {
                picked = Some(i);
            }
        }
        if click.is_some() && !self.popup.opened_now {
            let id = self.popup.open.as_ref().map(|open| open.id);
            self.popup.picked = id.zip(picked);
            self.popup.open = None;
        }
    }
}

/// How far a list is scrolled.
#[derive(Default)]
pub struct Scroll {
    offset: f32,
}

impl Scroll {
    /// Starts a scrolled, clipped area. Returns the y its content starts at.
    pub fn begin(&mut self, cx: &mut Cx, rect: Rect, content_h: f32) -> f32 {
        let max = (content_h - rect.h).max(0.0);
        if cx.hot(rect) && cx.input.scroll != 0.0 {
            self.offset -= cx.input.scroll * 24.0;
        }
        self.offset = self.offset.clamp(0.0, max);
        cx.clip(rect);
        rect.y - self.offset.round()
    }

    /// Ends the area and draws the bar, if the content is taller than the area.
    pub fn end(&self, cx: &mut Cx, rect: Rect, content_h: f32) {
        cx.unclip();
        if content_h <= rect.h {
            return;
        }
        let bar_h = (rect.h * rect.h / content_h).max(12.0);
        let travel = rect.h - bar_h;
        let y = rect.y + travel * self.offset / (content_h - rect.h);
        cx.fill(Rect::new(rect.right() - 2.0, y.round(), 2.0, bar_h.round()), theme::FAINT);
    }

    pub fn reset(&mut self) {
        self.offset = 0.0;
    }
}

/// Insets a rectangle on every side.
pub fn inset(rect: Rect, by: f32) -> Rect {
    Rect::new(rect.x + by, rect.y + by, (rect.w - 2.0 * by).max(0.0), (rect.h - 2.0 * by).max(0.0))
}
