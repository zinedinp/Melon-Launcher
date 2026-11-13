//! The launcher's accounts page: the list with its buttons, and under it

use super::heads::Heads;
use super::login::Outcome;
use super::row::{self, Look, Session};
use super::stats;
use super::window::{BUTTON_H, GAP, ROW_W};
use melon::auth::accounts::{Account, Accounts, Meta, now_secs, offline_uuid};
use melon::gui::screen::{CursorKind, TextEvent};
use melon::gui::text::font::LINE_HEIGHT;
use melon::gui::widgets::button::Button;
use melon::gui::widgets::row_icon::{Moves, Zone};
use melon::gui::widgets::scroll_list::{ListClick, ScrollList};
use melon::gui::widgets::{Rect, Ui};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const DIM: [f32; 4] = [0.6, 0.6, 0.6, 1.0];
const COPIED: Duration = Duration::from_millis(1500);
/// The refresh button on an expired / logged-out row.
const REFRESH_W: f32 = 50.0;
const REFRESH_H: f32 = 14.0;
const REFRESH_INSET: f32 = 4.0;
/// UUID and play-time under the list (scaled glyphs).
const DETAIL_SCALE: f32 = 0.7;
const DETAIL_H: f32 = 20.0;

/// What the window should do after a click or key.
pub enum Action {
    None,
    Play,
    /// Start the Microsoft device-code login.
    SignIn,
    /// Ask for an offline profile's name.
    NameNewProfile,
    /// Ask for the selected offline profile's new name.
    NameRename(String),
}

/// What a click on a row's icon or its refresh button does.
#[derive(Clone, Copy)]
enum RowTarget {
    /// Swap with the neighbour this many places away.
    Move(isize),
    Refresh,
    /// The play arrow over the face: starts with this account.
    Play,
}

pub struct AccountPanel {
    game_dir: PathBuf,
    accounts: Accounts,
    /// What the launcher learned of each Microsoft account's session, by key.
    sessions: HashMap<String, Session>,
    list: ScrollList,
    microsoft: Button,
    offline: Button,
    rename: Button,
    lock: Button,
    remove: Button,
    /// The UUID line, which copies on a click.
    uuid_line: Rect,
    copied_at: Option<Instant>,
    busy: bool,
    size: (f32, f32),
    mouse: (f32, f32),
}

impl AccountPanel {
    pub fn new(game_dir: PathBuf) -> Self {
        let accounts = Accounts::load(&game_dir);
        let mut list = ScrollList::new(ROW_W, row::HEIGHT);
        list.set_len(accounts.list().len());
        list.select(accounts.selected_index());
        let mut panel = Self {
            game_dir,
            accounts,
            sessions: HashMap::new(),
            list,
            microsoft: Button::sized("melon.launcher.add_microsoft", 0.0, BUTTON_H),
            offline: Button::sized("melon.launcher.add_offline", 0.0, BUTTON_H),
            rename: Button::sized("melon.launcher.rename", 0.0, BUTTON_H),
            lock: Button::sized("melon.launcher.lock", 0.0, BUTTON_H),
            remove: Button::sized("melon.launcher.remove", 0.0, BUTTON_H),
            uuid_line: Rect::default(),
            copied_at: None,
            busy: false,
            size: (0.0, 0.0),
            mouse: (-1.0, -1.0),
        };
        panel.refresh();
        panel
    }

    pub fn accounts(&self) -> &[Account] {
        self.accounts.list()
    }

    pub fn selected(&self) -> Option<&Account> {
        self.accounts.selected()
    }

    pub fn column_x(&self) -> f32 {
        self.microsoft.x
    }

    pub fn add_row_y(&self) -> f32 {
        self.microsoft.y
    }

    /// The selected account may not be played.
    pub fn play_blocked(&self) -> bool {
        self.selected()
            .is_some_and(|account| self.accounts.meta(account).locked)
    }

    /// The selected Microsoft account's session has not been looked at.
    pub fn needs_check(&self) -> bool {
        self.accounts
            .selected()
            .is_some_and(|a| a.is_microsoft() && !self.sessions.contains_key(&a.key()))
    }

    /// The accounts column on the right of the launcher.
    pub fn resize_column(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.size = (w, h);
        let bottom = y + h;
        let row_w = (w - 8.0).max(1.0);
        self.list.set_row_width(row_w);
        let half = ((w - GAP) / 2.0).floor();
        for (i, button) in [&mut self.microsoft, &mut self.offline]
            .into_iter()
            .enumerate()
        {
            button.w = half;
            button.x = x + i as f32 * (half + GAP);
            button.y = bottom - BUTTON_H;
        }
        let third = ((w - 2.0 * GAP) / 3.0).floor();
        for (i, button) in [&mut self.rename, &mut self.lock, &mut self.remove]
            .into_iter()
            .enumerate()
        {
            button.w = third;
            button.x = x + i as f32 * (third + GAP);
            button.y = bottom - BUTTON_H * 2.0 - GAP;
        }
        self.uuid_line = Rect::new(
            x + 4.0,
            bottom - BUTTON_H * 2.0 - GAP * 2.0 - DETAIL_H,
            (w - 8.0).max(1.0),
            DETAIL_H,
        );
        let end = (self.uuid_line.y - 4.0).max(y + row::HEIGHT);
        self.list
            .set_area(x + 1.0, y + 1.0, end, (w - 2.0).max(1.0));
    }

    /// A login or session check is in flight: nothing may change under it.
    pub fn set_busy(&mut self, busy: bool) {
        if self.busy != busy {
            self.busy = busy;
            self.refresh();
        }
    }

    /// Which buttons can be pressed.
    fn refresh(&mut self) {
        let selected = self.accounts.selected();
        self.microsoft.enabled = !self.busy;
        self.offline.enabled = !self.busy;
        self.rename.enabled = !self.busy && selected.is_some_and(|a| !a.is_microsoft());
        self.lock.enabled = selected.is_some();
        self.remove.enabled = !self.busy && selected.is_some();
    }

    /// The pointer moved. `active` is false under a modal.
    pub fn hover(&mut self, x: f32, y: f32, active: bool) {
        let (x, y) = if active {
            (x, y)
        } else {
            (f32::NEG_INFINITY, f32::NEG_INFINITY)
        };
        self.mouse = (x, y);
        self.list.on_mouse_move(x, y);
        for button in [
            &mut self.microsoft,
            &mut self.offline,
            &mut self.rename,
            &mut self.lock,
            &mut self.remove,
        ] {
            button.update_hover(x, y);
        }
    }

    pub fn cursor(&self, naming: bool) -> CursorKind {
        let buttons = [
            &self.microsoft,
            &self.offline,
            &self.rename,
            &self.lock,
            &self.remove,
        ];
        let over_buttons = !naming && buttons.iter().any(|b| b.enabled && b.hovered);
        let over_uuid =
            self.selected().is_some() && self.uuid_line.contains(self.mouse.0, self.mouse.1);
        if over_buttons || over_uuid || (!naming && self.row_target().is_some()) {
            return CursorKind::Pointer;
        }
        self.list.cursor().unwrap_or_default()
    }

    pub fn release(&mut self) {
        self.list.on_mouse_up();
    }

    pub fn scroll(&mut self, delta: f32) {
        self.list.on_scroll(delta);
    }

    /// A click. `naming`: the name entry is open over the buttons.
    pub fn click(&mut self, x: f32, y: f32, naming: bool) -> Action {
        if !self.busy
            && let Some((index, target)) = self.row_target_at(x, y)
        {
            match target {
                RowTarget::Move(by) => self.move_row(index, by),
                RowTarget::Refresh => {
                    self.select(index);
                    return Action::SignIn;
                }
                RowTarget::Play => {
                    self.select(index);
                    return Action::Play;
                }
            }
            return Action::None;
        }
        match self.list.on_click(x, y) {
            ListClick::Row { index, double } => {
                if self.busy {
                    self.list.select(self.accounts.selected_index());
                    return Action::None;
                }
                if Some(index) != self.accounts.selected_index() {
                    self.select(index);
                }
                return if double { Action::Play } else { Action::None };
            }
            ListClick::Scrollbar => return Action::None,
            ListClick::None => {}
        }
        if self.uuid_line.contains(x, y)
            && let Some(account) = self.selected()
        {
            melon::platform::copy_to_clipboard(&uuid_of(account));
            self.copied_at = Some(Instant::now());
            return Action::None;
        }
        if naming {
            return Action::None;
        }
        if self.microsoft.clicked(x, y) {
            Action::SignIn
        } else if self.offline.clicked(x, y) {
            Action::NameNewProfile
        } else if self.rename.clicked(x, y) {
            let name = self.selected().map(|a| a.username().to_owned());
            name.map_or(Action::None, Action::NameRename)
        } else if self.lock.clicked(x, y) {
            self.toggle_lock();
            Action::None
        } else if self.remove.clicked(x, y) {
            self.remove_selected();
            Action::None
        } else {
            Action::None
        }
    }

    pub fn key(&mut self, event: TextEvent) -> Action {
        match event {
            TextEvent::Up => self.step(-1),
            TextEvent::Down => self.step(1),
            TextEvent::Delete { .. } if self.remove.enabled => self.remove_selected(),
            TextEvent::Enter => return Action::Play,
            _ => {}
        }
        Action::None
    }

    /// A login or check ended.
    pub fn finished(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::SignedIn => {
                self.accounts = Accounts::load(&self.game_dir);
                self.mark_selected(Session::Valid);
                self.sync_list();
            }
            Outcome::Expired => self.mark_selected(Session::Expired),
        }
        self.refresh();
    }

    /// The game ran with the selected account: counts its play time.
    pub fn record_play(&mut self, key: &str, secs: u64) {
        melon::auth::accounts::record_play(&self.game_dir, key, secs);
        self.accounts = Accounts::load(&self.game_dir);
    }

    pub fn add_offline(&mut self, name: &str) {
        if self.accounts.add_offline(name) {
            self.save();
            self.sync_list();
        }
    }

    pub fn rename_selected(&mut self, name: &str) {
        if let Some(index) = self.accounts.selected_index()
            && self.accounts.rename_offline(index, name)
        {
            self.save();
            self.sync_list();
        }
    }

    /// A login or check ran for the selected account (nothing can change
    fn mark_selected(&mut self, session: Session) {
        if let Some(account) = self.accounts.selected() {
            self.sessions.insert(account.key(), session);
        }
    }

    /// The face tile, under the move / play arrows.
    fn icon_rect(&self, index: usize) -> Rect {
        let row = self.list.row_rect(index);
        Rect::new(
            row.x + 4.0,
            row.y + (row.h - row::FACE) / 2.0,
            row::FACE,
            row::FACE,
        )
    }

    fn moves(&self, index: usize) -> Moves {
        Moves::of(index, self.accounts.list().len())
    }

    /// What the session check found out about the account at `index`.
    fn session_at(&self, index: usize) -> Option<Session> {
        self.sessions
            .get(&self.accounts.list()[index].key())
            .copied()
    }

    /// Where the refresh button of the row at `index` sits.
    fn refresh_rect(&self, index: usize) -> Rect {
        let row = self.list.row_rect(index);
        Rect::new(
            row.right() - REFRESH_W - REFRESH_INSET,
            row.y + (row.h - REFRESH_H) / 2.0,
            REFRESH_W,
            REFRESH_H,
        )
    }

    fn shows_refresh(&self, index: usize) -> bool {
        self.accounts
            .list()
            .get(index)
            .is_some_and(|a| a.is_microsoft())
            && self.session_at(index) == Some(Session::Expired)
    }

    /// The arrow or button under `(x, y)` and the row it belongs to.
    fn row_target_at(&self, x: f32, y: f32) -> Option<(usize, RowTarget)> {
        let index = self.list.row_at(x, y)?;
        if let Some(zone) = icon_zone(self.icon_rect(index), (x, y), self.moves(index)) {
            return match zone {
                Zone::MoveUp => Some((index, RowTarget::Move(-1))),
                Zone::MoveDown => Some((index, RowTarget::Move(1))),
                Zone::Primary => Some((index, RowTarget::Play)),
            };
        }
        (self.shows_refresh(index) && self.refresh_rect(index).contains(x, y))
            .then_some((index, RowTarget::Refresh))
    }

    fn row_target(&self) -> Option<(usize, RowTarget)> {
        self.row_target_at(self.mouse.0, self.mouse.1)
    }

    /// The move arrows swap a row with its neighbour; the selection stays
    fn move_row(&mut self, index: usize, by: isize) {
        if self.accounts.move_account(index, by) {
            self.save();
            self.list.select(self.accounts.selected_index());
        }
    }

    /// Moves the selection `by` rows.
    fn step(&mut self, by: isize) {
        let len = self.accounts.list().len();
        let Some(current) = self.accounts.selected_index() else {
            return;
        };
        let next = current.saturating_add_signed(by).min(len.saturating_sub(1));
        if !self.busy && next != current {
            self.select(next);
            let height = self.list.viewport_height();
            self.list.center_on(next, height);
        }
    }

    fn select(&mut self, index: usize) {
        self.accounts.select(index);
        self.save();
        self.list.select(Some(index));
        self.refresh();
    }

    fn toggle_lock(&mut self) {
        if let Some(index) = self.accounts.selected_index() {
            let locked = self.accounts.meta(&self.accounts.list()[index]).locked;
            self.accounts.set_locked(index, !locked);
            self.save();
        }
    }

    fn remove_selected(&mut self) {
        let Some(index) = self.accounts.selected_index() else {
            return;
        };
        self.accounts.remove(index);
        self.save();
        self.sync_list();
    }

    /// The list follows `accounts` and scrolls to the selection.
    fn sync_list(&mut self) {
        self.list.set_len(self.accounts.list().len());
        let selected = self.accounts.selected_index();
        self.list.select(selected);
        if let Some(index) = selected {
            let height = self.list.viewport_height();
            self.list.center_on(index, height);
        }
        self.refresh();
    }

    fn save(&self) {
        if let Err(e) = self.accounts.save(&self.game_dir) {
            eprintln!("accounts.json konnte nicht gespeichert werden: {e}");
        }
    }

    /// The list, and under it the details and buttons unless the name
    pub fn draw(&self, ui: &mut Ui, heads: &Heads, restoring: bool, naming: bool) {
        let center = ui.width / 2.0;
        let selected = self.accounts.selected_index();
        self.list.draw(ui, |ui, index, rect| {
            let account = &self.accounts.list()[index];
            let key = account.key();
            let session = match self.sessions.get(&key) {
                Some(&known) => known,
                None if restoring && selected == Some(index) => Session::Checking,
                None => Session::Unknown,
            };
            let look = Look {
                session,
                locked: self.accounts.meta(account).locked,
                face: heads.get(&key),
            };
            let text_right = if self.shows_refresh(index) {
                self.refresh_rect(index).x - 4.0
            } else {
                rect.right() - 4.0
            };
            row::draw(ui, rect, account, &look, text_right);
            if !naming && !self.busy {
                self.draw_row_controls(ui, index, account);
            }
        });
        if self.accounts.is_empty() {
            let hint = ui.tr("melon.launcher.empty");
            let middle = self.list.row_rect(0).y + 8.0;
            ui.centered(hint, center, middle, DIM);
        }
        if naming {
            return;
        }
        if let Some(account) = self.selected() {
            self.draw_details(ui, account);
        }
        self.microsoft.draw(ui);
        self.offline.draw(ui);
        self.rename.draw(ui);
        let locked = self.play_blocked();
        let lock_label = ui.tr(if locked {
            "melon.launcher.unlock"
        } else {
            "melon.launcher.lock"
        });
        self.lock.draw_labeled(ui, lock_label);
        self.remove.draw(ui);
    }

    /// Play / move arrows over the hovered row's face, and Refresh on a
    fn draw_row_controls(&self, ui: &mut Ui, index: usize, _account: &Account) {
        let (x, y) = self.mouse;
        if self.list.row_at(x, y) == Some(index) {
            let icon = self.icon_rect(index);
            let zone = icon_zone(icon, self.mouse, self.moves(index));
            draw_launcher_arrows(ui, icon, zone, self.moves(index));
        }
        if self.shows_refresh(index) {
            let at = self.refresh_rect(index);
            let mut button = Button::new("melon.launcher.refresh", at.x, at.y, at.w, at.h);
            button.hovered = at.contains(x, y);
            let label = ui.tr("melon.launcher.refresh").to_owned();
            button.draw_labeled(ui, &label);
        }
    }

    /// The UUID, which a click copies, and when the account was last played.
    fn draw_details(&self, ui: &mut Ui, account: &Account) {
        let line = self.uuid_line;
        let copied = self.copied_at.is_some_and(|t| t.elapsed() < COPIED);
        let hovered = line.contains(self.mouse.0, self.mouse.1);
        let (text, color) = if copied {
            (ui.tr("melon.login.copied").to_owned(), [0.5, 0.9, 0.5, 1.0])
        } else {
            let color = if hovered { [1.0; 4] } else { DIM };
            (uuid_of(account), color)
        };
        let uuid = ui.ellipsize(&text, line.w);
        ui.text_scaled(&uuid, line.x, line.y, color, DETAIL_SCALE);

        let Meta {
            last_played,
            play_secs,
            ..
        } = self.accounts.meta(account);
        let stats = if last_played == 0 {
            ui.tr("melon.launcher.never").to_owned()
        } else {
            let ago = stats::ago(ui.lang, now_secs().saturating_sub(last_played));
            ui.lang.format(
                "melon.launcher.stats",
                &[&ago, &stats::span(ui.lang, play_secs)],
            )
        };
        let stats = ui.ellipsize(&stats, line.w);
        ui.text_scaled(
            &stats,
            line.x,
            line.y + LINE_HEIGHT * DETAIL_SCALE + 2.0,
            DIM,
            DETAIL_SCALE,
        );
    }
}

/// Arrow zones on the face tile (the launcher chrome has no vanilla sprites).
fn icon_zone(icon: Rect, (x, y): (f32, f32), moves: Moves) -> Option<Zone> {
    let (rel_x, rel_y) = (x - icon.x, y - icon.y);
    if !(0.0..icon.w).contains(&rel_x) || !(0.0..icon.h).contains(&rel_y) {
        return None;
    }
    if rel_x >= icon.w / 2.0 {
        Some(Zone::Primary)
    } else if rel_y < icon.h / 2.0 && moves.up {
        Some(Zone::MoveUp)
    } else if rel_y >= icon.h / 2.0 && moves.down {
        Some(Zone::MoveDown)
    } else {
        None
    }
}

fn draw_launcher_arrows(ui: &mut Ui, icon: Rect, zone: Option<Zone>, moves: Moves) {
    ui.fill(icon, [0.0, 0.0, 0.0, 0.40]);
    let lit = [0.85, 1.0, 0.55, 1.0];
    let dim = [0.82, 0.82, 0.82, 1.0];
    let color = |z| if zone == Some(z) { lit } else { dim };
    ui.text(
        ">",
        icon.x + icon.w * 0.55,
        icon.y + (icon.h - 8.0) / 2.0,
        color(Zone::Primary),
    );
    if moves.up {
        ui.text("^", icon.x + 3.0, icon.y + 1.0, color(Zone::MoveUp));
    }
    if moves.down {
        ui.text(
            "v",
            icon.x + 3.0,
            icon.y + icon.h - 9.0,
            color(Zone::MoveDown),
        );
    }
}

/// The account's UUID in the usual 8-4-4-4-12 form (an offline profile's
fn uuid_of(account: &Account) -> String {
    match account {
        Account::Offline { username } => offline_uuid(username).to_string(),
        Account::Microsoft(stored) => uuid::Uuid::parse_str(&stored.uuid)
            .map_or_else(|_| stored.uuid.clone(), |uuid| uuid.to_string()),
    }
}
