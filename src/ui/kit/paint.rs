//! Drawing: rectangles, images and the 8x8 pixel font, into the renderer's list of quads.

use melon_base::gfx::TextureId;
use melon_base::gfx::ascii8::{self, ATLAS, CELL, COLS};
use melon_base::gfx::quad::DrawList;

/// A widget's area in GUI pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// The right and bottom edges are outside.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

/// The textures every frame draws with.
#[derive(Clone, Copy)]
pub struct Textures {
    /// 1x1 white, for plain rectangles.
    pub white: TextureId,
    /// The font's glyphs ([`ascii8::atlas`]).
    pub font: TextureId,
}

/// How far a character moves the pen.
fn advance(c: char) -> f32 {
    match glyph(c) {
        Some((_, width)) => width + 1.0,
        None => 4.0,
    }
}

/// A character's code point in the atlas and its width; `None` for a space.
fn glyph(c: char) -> Option<(u32, f32)> {
    if c == ' ' {
        return None;
    }
    let code = Some(c as u32)
        .filter(|code| ascii8::rows_for(*code).is_some())
        .unwrap_or('?' as u32);
    let rows = ascii8::rows_for(code)?;
    Some((code, ascii8::glyph_width(rows).max(1) as f32))
}

pub fn text_width(text: &str) -> f32 {
    text.chars().map(advance).sum()
}

/// `text` broken at spaces into lines at most `max_width` wide.
pub fn wrap(text: &str, max_width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if text_width(&candidate) <= max_width || line.is_empty() {
                line = candidate;
            } else {
                lines.push(std::mem::replace(&mut line, word.to_owned()));
            }
            while text_width(&line) > max_width && line.chars().count() > 1 {
                let mut head = String::new();
                for c in line.chars() {
                    if text_width(&head) + advance(c) > max_width && !head.is_empty() {
                        break;
                    }
                    head.push(c);
                }
                let rest = line[head.len()..].to_owned();
                lines.push(head);
                line = rest;
            }
        }
        lines.push(line);
    }
    lines
}

/// What a frame draws with.
pub struct Painter<'a> {
    pub list: &'a mut DrawList,
    textures: Textures,
    /// The window's size in GUI pixels.
    pub width: f32,
    pub height: f32,
}

impl<'a> Painter<'a> {
    pub fn new(list: &'a mut DrawList, textures: Textures, size: (f32, f32)) -> Self {
        Self {
            list,
            textures,
            width: size.0,
            height: size.1,
        }
    }

    /// A solid colour over a rectangle.
    pub fn fill(&mut self, rect: Rect, color: [f32; 4]) {
        self.list
            .push_fill(self.textures.white, rect.x, rect.y, rect.w, rect.h, color);
    }

    /// A filled box with a one pixel edge.
    pub fn panel(&mut self, rect: Rect, fill: [f32; 4], edge: [f32; 4]) {
        self.fill(rect, fill);
        self.fill(Rect::new(rect.x, rect.y, rect.w, 1.0), edge);
        self.fill(Rect::new(rect.x, rect.bottom() - 1.0, rect.w, 1.0), edge);
        self.fill(Rect::new(rect.x, rect.y, 1.0, rect.h), edge);
        self.fill(Rect::new(rect.right() - 1.0, rect.y, 1.0, rect.h), edge);
    }

    /// A whole texture over a rectangle.
    pub fn sprite(&mut self, texture: TextureId, rect: Rect) {
        self.list.push_sprite(texture, rect.x, rect.y, rect.w, rect.h);
    }

    pub fn text_width(&self, text: &str) -> f32 {
        text_width(text)
    }

    fn glyphs(&mut self, text: &str, x: f32, y: f32, color: [f32; 4]) -> f32 {
        let mut pen = x;
        for c in text.chars() {
            if let Some((code, width)) = glyph(c) {
                let (col, row) = (code % COLS, code / COLS);
                let u0 = (col * CELL) as f32 / ATLAS as f32;
                let v0 = (row * CELL) as f32 / ATLAS as f32;
                let uv = [u0, v0, u0 + width / ATLAS as f32, v0 + 8.0 / ATLAS as f32];
                self.list
                    .push_rect(self.textures.font, [pen, y, width, 8.0], uv, color);
            }
            pen += advance(c);
        }
        pen - x
    }

    /// Text with a drop shadow, top-left at `(x, y)`. Returns its width.
    pub fn text(&mut self, text: &str, x: f32, y: f32, color: [f32; 4]) -> f32 {
        let shadow = [color[0] * 0.25, color[1] * 0.25, color[2] * 0.25, color[3]];
        self.glyphs(text, x + 1.0, y + 1.0, shadow);
        self.glyphs(text, x, y, color)
    }

    /// [`Self::text`] scaled about its top-left.
    pub fn text_scaled(&mut self, text: &str, x: f32, y: f32, color: [f32; 4], scale: f32) -> f32 {
        if (scale - 1.0).abs() < 0.01 {
            return self.text(text, x, y, color);
        }
        let start = self.list.vertices_len();
        let width = self.text(text, x / scale, y / scale, color);
        self.list.scale_since(start, scale);
        width * scale
    }

    /// `text` cut with `...` so its width is at most `max_width`.
    pub fn ellipsize(&self, text: &str, max_width: f32) -> String {
        if text_width(text) <= max_width {
            return text.to_owned();
        }
        let dots = "...";
        let dots_width = text_width(dots);
        if max_width <= dots_width {
            return dots.to_owned();
        }
        let mut out = String::new();
        let mut width = 0.0;
        for c in text.chars() {
            if width + advance(c) + dots_width > max_width {
                break;
            }
            width += advance(c);
            out.push(c);
        }
        out.push_str(dots);
        out
    }

    pub fn wrap(&self, text: &str, max_width: f32) -> Vec<String> {
        wrap(text, max_width)
    }

    /// Clips what follows to `rect`, until [`Painter::unclip`].
    pub fn clip(&mut self, rect: Rect) {
        self.list
            .set_scissor(Some([rect.x, rect.y, rect.w, rect.h]));
    }

    pub fn unclip(&mut self) {
        self.list.set_scissor(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_wraps_at_spaces_and_breaks_long_words() {
        let lines = wrap("one two three", text_width("one two"));
        assert_eq!(lines, ["one two", "three"]);
        let lines = wrap("abcdefgh", text_width("abc"));
        assert!(lines.len() >= 3 && lines.concat() == "abcdefgh");
        assert_eq!(wrap("a\nb", 100.0), ["a", "b"]);
    }

    #[test]
    fn letters_have_widths_and_unknown_ones_a_stand_in() {
        assert_eq!(text_width(" "), 4.0);
        assert!(text_width("A") >= 5.0);
        assert_eq!(text_width("\u{4e2d}"), text_width("?"));
    }
}
