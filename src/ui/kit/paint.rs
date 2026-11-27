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

/// `text` with its middle cut to `...`: the end keeps about 60 % of the room.
pub fn ellipsize_middle(text: &str, max_width: f32) -> String {
    if text_width(text) <= max_width {
        return text.to_owned();
    }
    let dots = "...";
    let room = max_width - text_width(dots);
    if room <= 0.0 {
        return dots.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut head, mut tail) = (0, chars.len());
    let mut tail_w = 0.0;
    while tail > 0 && tail_w + advance(chars[tail - 1]) <= room * 0.6 {
        tail -= 1;
        tail_w += advance(chars[tail]);
    }
    let mut head_w = 0.0;
    while head < tail && head_w + advance(chars[head]) + tail_w <= room {
        head_w += advance(chars[head]);
        head += 1;
    }
    let mut out: String = chars[..head].iter().collect();
    out.push_str(dots);
    out.extend(&chars[tail..]);
    out
}

/// `text` in lines at most `max_width` wide, for URLs and paths: each line as full as it
/// gets, broken after a `/`, `\`, space, `?`, `&` or `=` when one is in its back half.
pub fn wrap_path(text: &str, max_width: f32) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut lines = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let (mut end, mut width) = (start, 0.0);
        while end < chars.len() && (end == start || width + advance(chars[end]) <= max_width) {
            width += advance(chars[end]);
            end += 1;
        }
        if end < chars.len()
            && let Some(cut) = (start + (end - start) / 2..end)
                .rev()
                .find(|&i| matches!(chars[i], '/' | '\\' | ' ' | '?' | '&' | '='))
        {
            end = cut + 1;
        }
        lines.push(chars[start..end].iter().collect::<String>().trim_end().to_owned());
        start = end;
        while chars.get(start) == Some(&' ') {
            start += 1;
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
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

    /// `text` with its middle cut to `...`, so both ends of a URL or path stay.
    pub fn ellipsize_middle(&self, text: &str, max_width: f32) -> String {
        ellipsize_middle(text, max_width)
    }

    pub fn wrap(&self, text: &str, max_width: f32) -> Vec<String> {
        wrap(text, max_width)
    }

    pub fn wrap_path(&self, text: &str, max_width: f32) -> Vec<String> {
        wrap_path(text, max_width)
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
    fn a_long_url_keeps_both_ends() {
        let url = "GET https://cdn.example.org/versions/0.10.4/windows-x86_64/melon.exe";
        let cut = ellipsize_middle(url, 120.0);
        assert!(text_width(&cut) <= 120.0, "{cut}");
        assert!(cut.starts_with("GET ") && cut.contains("...") && cut.ends_with("melon.exe"), "{cut}");
        assert_eq!(ellipsize_middle("short", 120.0), "short");
        assert_eq!(ellipsize_middle(url, 4.0), "...");
    }

    #[test]
    fn a_url_spills_onto_new_lines_at_its_slashes() {
        let url = "GET https://cdn.example.org/versions/0.10.4/windows-x86_64/melon.exe";
        let width = text_width("GET https://cdn.example.org/vers");
        let lines = wrap_path(url, width);
        assert!(lines.len() >= 2, "{lines:?}");
        assert!(lines[0].starts_with("GET https://"), "GET stays with its URL: {lines:?}");
        assert!(lines.iter().all(|line| text_width(line) <= width), "{lines:?}");
        assert!(lines[..lines.len() - 1].iter().all(|line| line.ends_with('/')), "{lines:?}");
        assert_eq!(lines.concat(), url);
        assert_eq!(wrap_path("short", 100.0), ["short"]);
        assert_eq!(wrap_path("", 100.0), [""]);
        let no_breaks = "a".repeat(40);
        assert!(wrap_path(&no_breaks, text_width("aaaa")).iter().all(|l| l.len() <= 4));
    }

    #[test]
    fn letters_have_widths_and_unknown_ones_a_stand_in() {
        assert_eq!(text_width(" "), 4.0);
        assert!(text_width("A") >= 5.0);
        assert_eq!(text_width("\u{4e2d}"), text_width("?"));
    }
}
