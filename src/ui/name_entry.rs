//! The one-line name entry the launcher's pages share: a profile name to

use melon::auth::accounts::{self, valid_offline_name};
use melon::gui::screen::{CursorKind, TextEvent};
use melon::gui::widgets::button::Button;
use melon::gui::widgets::edit_box::EditBox;
use melon::gui::widgets::{Ui, Widget};
use melon::instances;

const BUTTON_H: f32 = 20.0;
const OK_W: f32 = 60.0;
const GAP: f32 = 6.0;
const WARNING: [f32; 4] = [0.95, 0.78, 0.4, 1.0];

/// What the name is for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    AddOffline,
    RenameOffline,
    NewInstance,
    CopyInstance,
    RenameInstance,
}

impl Purpose {
    fn accepts(self, c: char) -> bool {
        match self {
            Purpose::NewInstance | Purpose::CopyInstance | Purpose::RenameInstance => {
                c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ' ')
            }
            _ => c.is_ascii_alphanumeric() || c == '_',
        }
    }

    fn max_len(self) -> usize {
        match self {
            Purpose::NewInstance | Purpose::CopyInstance | Purpose::RenameInstance => {
                instances::MAX_NAME_LEN
            }
            _ => accounts::MAX_NAME_LEN,
        }
    }

    fn valid(self, name: &str) -> bool {
        match self {
            Purpose::NewInstance | Purpose::CopyInstance | Purpose::RenameInstance => {
                instances::valid_name(name)
            }
            _ => valid_offline_name(name),
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Purpose::NewInstance | Purpose::CopyInstance | Purpose::RenameInstance => {
                "melon.launcher.instance_hint"
            }
            _ => "melon.launcher.name_hint",
        }
    }
}

/// How a click went.
pub enum Click {
    /// The OK button, with a valid name.
    Submitted(String),
    /// On the box or the disabled button.
    Inside,
    /// Elsewhere: the entry closes.
    Outside,
}

pub struct NameEntry {
    purpose: Purpose,
    edit: EditBox,
    ok: Button,
    open: bool,
}

impl NameEntry {
    pub fn new() -> Self {
        Self {
            purpose: Purpose::AddOffline,
            edit: EditBox::new("melon.launcher.name_hint", 0.0, BUTTON_H),
            ok: Button::sized("melon.launcher.add", OK_W, BUTTON_H),
            open: false,
        }
    }

    pub fn purpose(&self) -> Purpose {
        self.purpose
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self, purpose: Purpose, initial: &str) {
        self.purpose = purpose;
        self.open = true;
        self.edit.hint = purpose.hint().into();
        self.edit.set_value(initial);
        self.edit.set_focused(true);
        self.ok.label = match purpose {
            Purpose::AddOffline | Purpose::NewInstance | Purpose::CopyInstance => {
                "melon.launcher.add"
            }
            Purpose::RenameOffline | Purpose::RenameInstance => "melon.launcher.rename_ok",
        }
        .into();
        self.refresh();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.edit.set_focused(false);
        self.edit.set_value("");
    }

    /// Places the entry in a row `width` wide whose left edge is `x`.
    pub fn layout(&mut self, x: f32, y: f32, width: f32) {
        self.edit.x = x;
        self.edit.y = y;
        self.edit.w = width - GAP - OK_W;
        self.ok.x = x + width - OK_W;
        self.ok.y = y;
    }

    pub fn hover(&mut self, x: f32, y: f32) {
        self.ok.update_hover(x, y);
        Widget::update_hover(&mut self.edit, x, y);
    }

    pub fn cursor(&self) -> Option<CursorKind> {
        if self.ok.enabled && self.ok.hovered {
            Some(CursorKind::Pointer)
        } else {
            Widget::cursor(&self.edit)
        }
    }

    pub fn click(&mut self, x: f32, y: f32) -> Click {
        if self.ok.clicked(x, y) {
            return Click::Submitted(self.edit.value());
        }
        if self.edit.click(x, y) || self.ok.contains(x, y) {
            return Click::Inside;
        }
        self.close();
        Click::Outside
    }

    /// A key; the name on Enter if it is valid.
    pub fn key(&mut self, event: TextEvent) -> Option<String> {
        if matches!(event, TextEvent::Enter) {
            return self.ok.enabled.then(|| self.edit.value());
        }
        let len = self.edit.value().chars().count();
        if let Some(event) = self.filter(event, len) {
            self.edit.handle(&event);
            self.refresh();
        }
        None
    }

    /// What the box takes of a key: the characters a name can have, up to
    fn filter(&self, event: TextEvent, len: usize) -> Option<TextEvent> {
        let room = self.purpose.max_len().saturating_sub(len);
        let accepts = |c: &char| self.purpose.accepts(*c);
        match event {
            TextEvent::Char(c) => (accepts(&c) && room > 0).then_some(TextEvent::Char(c)),
            TextEvent::Paste(text) => {
                let kept: String = text.chars().filter(accepts).take(room).collect();
                (!kept.is_empty()).then_some(TextEvent::Paste(kept))
            }
            other => Some(other),
        }
    }

    fn refresh(&mut self) {
        self.ok.enabled = self.purpose.valid(&self.edit.value());
    }

    /// The entry, with a warning line above it when the purpose has one.
    pub fn draw(&self, ui: &mut Ui) {
        if self.purpose == Purpose::RenameOffline {
            let warning = ui.tr("melon.launcher.rename_warning");
            let lines = ui.font().wrap(warning, self.edit.w + GAP + OK_W);
            let top = self.edit.y - 2.0 - lines.len() as f32 * 9.0;
            for (i, line) in lines.iter().enumerate() {
                ui.text(line, self.edit.x, top + i as f32 * 9.0, WARNING);
            }
        }
        self.edit.draw(ui);
        self.ok.draw(ui);
    }
}
