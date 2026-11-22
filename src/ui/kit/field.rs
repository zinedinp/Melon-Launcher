//! A one-line text field and the keys it takes.

use super::paint::{Painter, Rect, text_width};
use super::theme;
use std::time::Instant;

/// A key press a text field understands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    Enter,
    /// Control+A, Control+C, Control+X.
    SelectAll,
    Copy,
    Cut,
    /// Control+V: the clipboard's text.
    Paste(String),
}

pub struct TextField {
    chars: Vec<char>,
    cursor: usize,
    /// Control+A was pressed: the next edit replaces everything.
    all_selected: bool,
    focused: bool,
    max_len: usize,
    /// Shows `*` for each character and keeps the text off the clipboard.
    password: bool,
    hint: String,
    /// When the cursor last moved, so it shows right after.
    moved: Instant,
    rect: Rect,
}

impl TextField {
    pub fn new(hint: impl Into<String>) -> Self {
        Self {
            chars: Vec::new(),
            cursor: 0,
            all_selected: false,
            focused: false,
            max_len: usize::MAX,
            password: false,
            hint: hint.into(),
            moved: Instant::now(),
            rect: Rect::default(),
        }
    }

    pub fn value(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn set_value(&mut self, value: &str) {
        self.chars = value.chars().take(self.max_len).collect();
        self.cursor = self.chars.len();
        self.all_selected = false;
    }

    pub fn set_max_length(&mut self, max: usize) {
        self.max_len = max;
        self.chars.truncate(max);
        self.cursor = self.cursor.min(self.chars.len());
    }

    /// A password box: masked, and never copied out.
    pub fn set_password(&mut self, password: bool) {
        self.password = password;
    }

    /// The characters as they are drawn.
    fn shown(&self) -> Vec<char> {
        match self.password {
            true => vec!['*'; self.chars.len()],
            false => self.chars.clone(),
        }
    }

    pub fn focused(&self) -> bool {
        self.focused
    }

    pub fn set_focused(&mut self, focused: bool) {
        if focused && !self.focused {
            self.moved = Instant::now();
        }
        self.focused = focused;
        if !focused {
            self.all_selected = false;
        }
    }

    pub fn set_rect(&mut self, rect: Rect) {
        self.rect = rect;
    }

    fn text_x(&self) -> f32 {
        self.rect.x + 4.0
    }

    /// The first character shown, so the cursor stays inside the box.
    fn first_shown(&self) -> usize {
        let room = (self.rect.w - 8.0).max(8.0);
        let chars = self.shown();
        let mut first = 0;
        while first < self.cursor
            && text_width(&chars[first..self.cursor].iter().collect::<String>()) > room
        {
            first += 1;
        }
        first
    }

    /// A press at `(x, y)`: focuses the box when it is hit and moves the cursor there.
    pub fn click(&mut self, x: f32, y: f32) -> bool {
        let hit = self.rect.contains(x, y);
        self.set_focused(hit);
        if hit {
            let first = self.first_shown();
            let mut pen = self.text_x();
            let mut cursor = first;
            for c in &self.shown()[first..] {
                let width = text_width(&c.to_string());
                if pen + width / 2.0 > x {
                    break;
                }
                pen += width;
                cursor += 1;
            }
            self.cursor = cursor;
            self.all_selected = false;
            self.moved = Instant::now();
        }
        hit
    }

    fn clear_if_selected(&mut self) -> bool {
        if !self.all_selected {
            return false;
        }
        self.all_selected = false;
        let had = !self.chars.is_empty();
        self.chars.clear();
        self.cursor = 0;
        had
    }

    fn insert(&mut self, text: &str) -> bool {
        let mut changed = self.clear_if_selected();
        for c in text.chars().filter(|c| !c.is_control()) {
            if self.chars.len() >= self.max_len {
                break;
            }
            self.chars.insert(self.cursor, c);
            self.cursor += 1;
            changed = true;
        }
        changed
    }

    /// One key. True when the text changed.
    pub fn handle(&mut self, key: &Key) -> bool {
        self.moved = Instant::now();
        match key {
            Key::Char(c) => self.insert(&c.to_string()),
            Key::Paste(text) => self.insert(text),
            Key::Backspace => {
                if self.clear_if_selected() {
                    return true;
                }
                if self.cursor == 0 {
                    return false;
                }
                self.cursor -= 1;
                self.chars.remove(self.cursor);
                true
            }
            Key::Delete => {
                if self.clear_if_selected() {
                    return true;
                }
                if self.cursor >= self.chars.len() {
                    return false;
                }
                self.chars.remove(self.cursor);
                true
            }
            Key::Left | Key::Home | Key::Right | Key::End => {
                self.all_selected = false;
                self.cursor = match key {
                    Key::Left => self.cursor.saturating_sub(1),
                    Key::Right => (self.cursor + 1).min(self.chars.len()),
                    Key::Home => 0,
                    _ => self.chars.len(),
                };
                false
            }
            Key::SelectAll => {
                self.all_selected = !self.chars.is_empty();
                false
            }
            Key::Copy => {
                if !self.password {
                    melon_base::platform::copy_to_clipboard(&self.value());
                }
                false
            }
            Key::Cut => {
                if !self.password {
                    melon_base::platform::copy_to_clipboard(&self.value());
                }
                self.all_selected = true;
                self.clear_if_selected()
            }
            Key::Enter => false,
        }
    }

    pub fn draw(&self, ui: &mut Painter) {
        let y = self.rect.y + (self.rect.h - 8.0) / 2.0;
        if self.chars.is_empty() {
            if !self.focused {
                let hint = ui.ellipsize(&self.hint, self.rect.w - 8.0);
                ui.text(&hint, self.text_x(), y, theme::FAINT);
            }
        } else {
            let first = self.first_shown();
            let shown: String = self.shown()[first..].iter().collect();
            let shown = if text_width(&shown) > self.rect.w - 6.0 {
                let mut cut = String::new();
                for c in shown.chars() {
                    if text_width(&cut) + text_width(&c.to_string()) > self.rect.w - 6.0 {
                        break;
                    }
                    cut.push(c);
                }
                cut
            } else {
                shown
            };
            if self.all_selected && self.focused {
                let width = text_width(&shown);
                ui.fill(
                    Rect::new(self.text_x() - 1.0, y - 1.0, width + 1.0, 10.0),
                    theme::ACCENT_FILL,
                );
            }
            ui.text(&shown, self.text_x(), y, theme::TEXT);
        }
        if self.focused && self.moved.elapsed().as_millis() % 1000 < 500 {
            let first = self.first_shown();
            let before: String = self.shown()[first..self.cursor].iter().collect();
            let x = self.text_x() + text_width(&before);
            ui.fill(Rect::new(x, y - 1.0, 1.0, 10.0), theme::TEXT);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(field: &mut TextField, text: &str) {
        for c in text.chars() {
            field.handle(&Key::Char(c));
        }
    }

    #[test]
    fn typing_moving_and_deleting() {
        let mut field = TextField::new("");
        typed(&mut field, "helo");
        field.handle(&Key::Left);
        typed(&mut field, "l");
        assert_eq!(field.value(), "hello");
        field.handle(&Key::Home);
        field.handle(&Key::Delete);
        field.handle(&Key::End);
        field.handle(&Key::Backspace);
        assert_eq!(field.value(), "ell");
    }

    #[test]
    fn select_all_is_replaced_by_the_next_text() {
        let mut field = TextField::new("");
        field.set_value("old name");
        field.handle(&Key::SelectAll);
        assert!(field.handle(&Key::Paste("new".into())));
        assert_eq!(field.value(), "new");
    }

    #[test]
    fn a_password_keeps_its_text_but_shows_stars() {
        let mut field = TextField::new("");
        field.set_password(true);
        typed(&mut field, "secret:123");
        assert_eq!(field.value(), "secret:123");
        assert_eq!(field.shown().iter().collect::<String>(), "**********");
    }

    #[test]
    fn the_length_is_capped() {
        let mut field = TextField::new("");
        field.set_max_length(3);
        typed(&mut field, "abcdef");
        assert_eq!(field.value(), "abc");
    }
}
