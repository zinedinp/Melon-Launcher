//! The launcher window: the quick bar on top, instances on the left, the selected instance on
//! the right, and the account and settings menus over them.

use super::content_list::ContentList;
use super::heads::Heads;
use super::kit::icons;
use super::kit::theme;
use super::kit::{self, BUTTON_H, Btn, Cx, GAP, Scroll};
use super::login::Outcome;
use super::settings_menu::SettingsMenu;
use super::textures::Textures;
use super::welcome::Welcome;
use crate::catalog;
use crate::cdn;
use crate::java_play;
use crate::play::{self, Facts, Install, PlayState};
use crate::store::instances::{self, Engine, Instance};
use crate::store::settings::Settings;
use crate::store::{java, shared};
use melon_base::auth::accounts::{Account, Accounts};
use super::kit::Rect;
use super::kit::TextField;
use melon_base::prepare::Found;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const TOPBAR_H: f32 = 36.0;
pub const PAD: f32 = 6.0;
const PLAY_W: f32 = 112.0;
const ACCOUNT_W: f32 = 172.0;
/// How long a second click confirms a delete.
const CONFIRM: Duration = Duration::from_secs(3);
/// How long a notice stays in the quick bar.
const NOTICE: Duration = Duration::from_secs(8);
/// How often what is on disk is looked at again.
const RECHECK: Duration = Duration::from_secs(2);
const RELEASES_URL: &str = "https://github.com/zinedinp/Melon-Launcher/releases";

/// What starting the game needs to know.
pub struct PlayRequest {
    pub instance: String,
    /// A server to join straight away.
    pub join: Option<String>,
    /// The account playing, to count its play time.
    pub account_key: Option<String>,
}

pub enum Action {
    Play(PlayRequest),
    /// End the running game.
    Stop,
    /// Fetch assets and/or a client; `Found` is a jar the user chose for the assets.
    Install(Install, Option<Found>),
    /// Start the Microsoft device-code login.
    SignIn,
    LanguageChanged,
}

/// What the launcher's loop tells the window each frame.
pub struct Status<'a> {
    /// A login, download or mod build is in flight.
    pub busy: bool,
    /// The instance whose game is running.
    pub running: Option<&'a str>,
    /// The selected account's session is being checked.
    pub restoring: bool,
    /// The CDN was asked for the release list in this run.
    pub catalog_checked: bool,
    pub heads: &'a Heads,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Main,
    Accounts,
    Settings,
}

/// What the launcher knows of a Microsoft account's saved session.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Session {
    Checking,
    Valid,
    Expired,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    NewInstance,
    RenameInstance,
    CopyInstance,
    GroupInstance,
    AddOffline,
    RenameOffline,
    NewShared,
    RenameShared,
}

/// The dialog that asks for a name.
pub struct Prompt {
    purpose: Purpose,
    edit: TextField,
    error: bool,
    /// A new instance for Java Edition.
    java: bool,
    /// The shared folder being renamed.
    subject: String,
}

/// What is on disk for the selected instance, looked at every [`RECHECK`].
#[derive(Default)]
pub struct Disk {
    pub installed: Vec<String>,
    pub local_binary: bool,
    pub assets_ready: bool,
    pub java_jar: bool,
    pub java_runtime: bool,
    pub last_server: Option<String>,
    /// The shared folders, `global` first.
    pub shared: Vec<String>,
}

pub struct Shell {
    pub home: PathBuf,
    pub settings: Settings,
    pub instances: Vec<Instance>,
    pub accounts: Accounts,
    pub sessions: HashMap<String, Session>,
    pub page: Page,
    pub disk: Disk,
    checked_at: Instant,
    pub prompt: Option<Prompt>,
    confirm: Option<(String, Instant)>,
    notice: Option<(String, Instant)>,
    pub copied_at: Option<Instant>,
    pub grid_scroll: Scroll,
    pub side_scroll: Scroll,
    pub content: ContentList,
    pub textures: Textures,
    pub settings_menu: SettingsMenu,
    pub account_scroll: Scroll,
    /// Java runtimes found on this machine; looked for when first needed.
    javas: Option<Vec<java::Runtime>>,
    /// The first-run setup, shown instead of the instances until it is done or skipped.
    pub welcome: Option<Welcome>,
}

impl Shell {
    pub fn new(home: &Path) -> Self {
        let fresh = !home.join("instances").exists() && !home.join("launcher.json").exists();
        instances::init(home);
        if shared::ensure(home) {
            instances::relink_all(home);
        }
        let mut settings = Settings::load(home);
        settings.welcome |= fresh;
        let welcome = settings.welcome.then(Welcome::new);
        let mut shell = Self {
            home: home.to_path_buf(),
            settings_menu: SettingsMenu::new(home, &settings),
            settings,
            instances: Vec::new(),
            accounts: Accounts::load(home),
            sessions: HashMap::new(),
            page: Page::Main,
            disk: Disk::default(),
            checked_at: Instant::now(),
            prompt: None,
            confirm: None,
            notice: None,
            copied_at: None,
            grid_scroll: Scroll::default(),
            side_scroll: Scroll::default(),
            content: ContentList::default(),
            textures: Textures::default(),
            account_scroll: Scroll::default(),
            javas: None,
            welcome,
        };
        shell.reload();
        shell
    }

    /// Reads the instances again and keeps a valid selection.
    pub fn reload(&mut self) {
        self.instances = instances::list(&self.home);
        let selected = self
            .settings
            .instance
            .as_deref()
            .and_then(|id| self.instances.iter().find(|i| i.id == id || i.name.eq_ignore_ascii_case(id)))
            .or(self.instances.first())
            .map(|i| i.id.clone());
        if selected != self.settings.instance {
            self.settings.instance = selected;
            self.settings.save(&self.home);
        }
        self.content.set_instance(self.selected().map(|i| i.path.clone()));
        self.recheck();
    }

    pub fn selected(&self) -> Option<&Instance> {
        let id = self.settings.instance.as_deref()?;
        self.instances.iter().find(|i| i.id == id)
    }

    pub fn select(&mut self, id: &str) {
        if self.settings.instance.as_deref() == Some(id) {
            return;
        }
        self.settings.instance = Some(id.to_owned());
        self.settings.save(&self.home);
        self.side_scroll.reset();
        self.content.set_instance(self.selected().map(|i| i.path.clone()));
        self.recheck();
    }

    /// Looks at what is on disk for the selected instance.
    pub fn recheck(&mut self) {
        self.checked_at = Instant::now();
        let selected = self.selected().cloned();
        let java = selected.as_ref().filter(|i| i.is_java());
        self.disk = Disk {
            installed: cdn::installed_versions(&self.home),
            local_binary: self.settings.local_binary(&self.home).is_some(),
            assets_ready: melon_base::home::is_prepared_at(&self.settings.melon_play_assets(&self.home)),
            java_jar: java.is_some_and(|i| java_play::locate(&self.home, i).is_some()),
            java_runtime: java.is_some_and(|i| java::resolve(&self.settings, i).is_some()),
            last_server: selected.as_ref().and_then(|i| instances::last_server(&i.path)),
            shared: shared::list(&self.home),
        };
    }

    pub fn javas(&mut self) -> &[java::Runtime] {
        self.javas.get_or_insert_with(java::detect)
    }

    pub fn rescan_javas(&mut self) {
        self.javas = Some(java::detect());
    }

    /// A line for the quick bar's info area, for a while.
    pub fn notify(&mut self, text: impl Into<String>) {
        self.notice = Some((text.into(), Instant::now()));
    }

    /// Reports a failed action in the quick bar.
    pub fn report<T>(&mut self, result: std::io::Result<T>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                self.notify(error.to_string());
                None
            }
        }
    }

    /// True on the second press within [`CONFIRM`]; the first press arms it.
    pub fn confirmed(&mut self, what: &str) -> bool {
        if self.confirming(what) {
            self.confirm = None;
            return true;
        }
        self.confirm = Some((what.to_owned(), Instant::now()));
        false
    }

    pub fn confirming(&self, what: &str) -> bool {
        self.confirm
            .as_ref()
            .is_some_and(|(armed, at)| armed == what && at.elapsed() < CONFIRM)
    }

    pub fn selected_account(&self) -> Option<&Account> {
        self.accounts.selected()
    }

    pub fn account_locked(&self) -> bool {
        self.selected_account()
            .is_some_and(|account| self.accounts.meta(account).locked)
    }

    /// The selected Microsoft account's session has not been looked at.
    pub fn needs_check(&self) -> bool {
        self.selected_account()
            .is_some_and(|a| a.is_microsoft() && !self.sessions.contains_key(&a.key()))
    }

    /// A login or session check ended.
    pub fn finished(&mut self, outcome: Outcome) {
        let session = match outcome {
            Outcome::SignedIn => {
                self.accounts = Accounts::load(&self.home);
                Session::Valid
            }
            Outcome::Expired => Session::Expired,
        };
        if let Some(account) = self.accounts.selected() {
            self.sessions.insert(account.key(), session);
        }
    }

    /// The game ended after `secs`.
    pub fn record_play(&mut self, account_key: Option<&str>, instance: &str, secs: u64) {
        if let Some(key) = account_key {
            melon_base::auth::accounts::record_play(&self.home, key, secs);
            self.accounts = Accounts::load(&self.home);
        }
        instances::record_play(&self.home, instance, secs);
        self.reload();
    }

    pub fn save_accounts(&mut self) {
        if let Err(e) = self.accounts.save(&self.home) {
            self.notify(format!("accounts.json: {e}"));
        }
    }

    pub fn session_of(&self, account: &Account, status: &Status) -> Option<Session> {
        let key = account.key();
        self.sessions.get(&key).copied().or_else(|| {
            let selected = self.selected_account().is_some_and(|a| a.key() == key);
            (status.restoring && selected).then_some(Session::Checking)
        })
    }

    /// The text key and colour of an account's status line.
    pub fn account_status(&self, account: &Account, status: &Status) -> (&'static str, theme::Color) {
        if self.accounts.meta(account).locked {
            return ("account.locked", theme::DANGER);
        }
        if !account.is_microsoft() {
            return ("account.offline", theme::DIM);
        }
        match self.session_of(account, status) {
            Some(Session::Checking) => ("account.checking", theme::DIM),
            Some(Session::Valid) => ("account.valid", theme::ACCENT),
            Some(Session::Expired) => ("account.expired", theme::DANGER),
            None => ("account.microsoft", theme::DIM),
        }
    }

    pub fn facts<'a>(&'a self, status: &Status) -> Facts<'a> {
        Facts {
            instance: self.selected(),
            busy: status.busy,
            running: status.running.is_some(),
            account_locked: self.account_locked(),
            assets_ready: self.disk.assets_ready,
            latest: None,
            installed: &self.disk.installed,
            local_binary: self.disk.local_binary,
            java_jar: self.disk.java_jar,
            java_runtime: self.disk.java_runtime,
        }
    }

    pub fn play_state(&self, status: &Status) -> PlayState {
        let latest = catalog::latest_game();
        let mut facts = self.facts(status);
        facts.latest = latest.as_deref();
        play::play_state(&facts)
    }

    /// The Play button, or a tile's double click, was pressed in `state`.
    pub fn press_play(&mut self, state: &PlayState, join: Option<String>, status: &Status, out: &mut Vec<Action>) {
        let Some(instance) = self.selected() else {
            return;
        };
        match state {
            PlayState::Start => out.push(Action::Play(PlayRequest {
                instance: instance.id.clone(),
                join,
                account_key: self.selected_account().map(Account::key),
            })),
            PlayState::Download | PlayState::Update => {
                let latest = catalog::latest_game();
                let mut facts = self.facts(status);
                facts.latest = latest.as_deref();
                out.push(Action::Install(play::install(&facts), None));
            }
            PlayState::Locked(_) => {}
        }
    }

    /// Escape: closes the innermost thing that is open.
    pub fn cancel(&mut self, popup: &mut kit::Popup) {
        if popup.is_open() {
            popup.close();
        } else if self.prompt.is_some() {
            self.prompt = None;
        } else {
            self.page = Page::Main;
        }
    }

    /// One frame: draws the window and returns what the launcher should do.
    pub fn frame(&mut self, cx: &mut Cx, status: &Status, modal: bool) -> Vec<Action> {
        if self.checked_at.elapsed() >= RECHECK {
            self.recheck();
        }
        let mut out = Vec::new();
        let free = !modal && !cx.popup.is_open();
        // What opens in this frame reacts from the next one on: the press that opened it is
        // not its own.
        let (page_was_open, prompt_was_open) = (self.page != Page::Main, self.prompt.is_some());
        let all = Rect::new(0.0, 0.0, cx.width(), cx.height());
        cx.fill(all, theme::BG);

        if self.welcome.is_some() {
            cx.active = free && self.prompt.is_none();
            self.welcome_page(cx, status, &mut out);
            if self.prompt.is_some() {
                cx.active = free && prompt_was_open;
                cx.shade();
                self.prompt_dialog(cx);
            }
            return out;
        }

        cx.active = free && self.page == Page::Main && self.prompt.is_none();
        self.topbar(cx, status, &mut out);
        let body_y = TOPBAR_H + PAD;
        let body_h = (cx.height() - body_y - PAD).max(40.0);
        let grid_w = (cx.width() * 0.36).clamp(190.0, 300.0).floor();
        let grid = Rect::new(PAD, body_y, grid_w, body_h);
        let panel_x = grid.right() + PAD;
        let panel = Rect::new(panel_x, body_y, (cx.width() - panel_x - PAD).max(40.0), body_h);
        self.instance_grid(cx, grid, status, &mut out);
        self.instance_panel(cx, panel, status, &mut out);

        if self.page != Page::Main {
            cx.active = free && page_was_open && self.prompt.is_none();
            cx.shade();
            let sheet = Rect::new(
                24.0,
                TOPBAR_H + 4.0,
                (cx.width() - 48.0).max(40.0),
                (cx.height() - TOPBAR_H - 16.0).max(40.0),
            );
            match self.page {
                Page::Accounts => self.account_menu(cx, sheet, status, &mut out),
                Page::Settings => self.settings_page(cx, sheet, status, &mut out),
                Page::Main => {}
            }
        }
        if self.prompt.is_some() {
            cx.active = free && prompt_was_open;
            cx.shade();
            self.prompt_dialog(cx);
        }
        out
    }

    /// A menu's frame: title bar with a close button. Returns the area under the title.
    pub fn sheet(&mut self, cx: &mut Cx, sheet: Rect, title: &str) -> Rect {
        cx.frame(sheet, theme::PANEL, theme::FAINT);
        cx.fill(Rect::new(sheet.x + 1.0, sheet.y + 1.0, sheet.w - 2.0, 22.0), theme::BAR);
        cx.fill(Rect::new(sheet.x + 1.0, sheet.y + 23.0, sheet.w - 2.0, 1.0), theme::EDGE);
        cx.text(title, sheet.x + 10.0, sheet.y + 8.0, theme::TEXT);
        let close = Rect::new(sheet.right() - 21.0, sheet.y + 3.0, 18.0, 18.0);
        if cx.button(close, "", Btn::GHOST.icon(icons::CROSS)) {
            self.page = Page::Main;
        }
        Rect::new(sheet.x + 1.0, sheet.y + 24.0, sheet.w - 2.0, sheet.h - 25.0)
    }

    fn topbar(&mut self, cx: &mut Cx, status: &Status, out: &mut Vec<Action>) {
        let width = cx.width();
        cx.fill(Rect::new(0.0, 0.0, width, TOPBAR_H), theme::BAR);
        cx.fill(Rect::new(0.0, TOPBAR_H - 1.0, width, 1.0), theme::EDGE);

        let play = Rect::new(width - PAD - PLAY_W, 6.0, PLAY_W, 24.0);
        let state = self.play_state(status);
        let (label, btn) = match &state {
            PlayState::Start => ("play.start", Btn::PRIMARY.icon(icons::PLAY)),
            PlayState::Download => ("play.download", Btn::PRIMARY.icon(icons::DOWNLOAD)),
            PlayState::Update => ("play.update", Btn::PRIMARY.icon(icons::UPDATE)),
            PlayState::Locked(_) => ("play.locked", Btn::NORMAL.icon(icons::LOCK).enabled(false)),
        };
        if cx.button(play, cx.t(label), btn) {
            self.press_play(&state, None, status, out);
        }
        if let PlayState::Locked(why) = &state {
            cx.tooltip(play, cx.t(why.key()));
        }

        let gear = Rect::new(play.x - GAP - 24.0, 6.0, 24.0, 24.0);
        if cx.button(gear, "", Btn::NORMAL.icon(icons::GEAR)) {
            self.page = Page::Settings;
        }
        cx.tooltip(gear, cx.t("settings.title"));

        let account_x = ((width - ACCOUNT_W) / 2.0).floor().min(gear.x - GAP - ACCOUNT_W).max(PAD);
        let account = Rect::new(account_x, 5.0, ACCOUNT_W, 26.0);
        self.account_switcher(cx, account, status);

        let info = Rect::new(PAD + 2.0, 0.0, (account.x - PAD * 2.0 - 2.0).max(0.0), TOPBAR_H);
        self.info(cx, info, status);
    }

    /// The selected account, with the list of accounts and the way into the account menu.
    fn account_switcher(&mut self, cx: &mut Cx, rect: Rect, status: &Status) {
        let hot = cx.hot(rect);
        let open = cx.dropdown_open("account");
        let edge = if open {
            theme::ACCENT
        } else if hot {
            theme::FAINT
        } else {
            theme::EDGE
        };
        cx.frame(rect, if hot { theme::HOVER } else { theme::RAISED }, edge);
        let face = Rect::new(rect.x + 4.0, rect.y + 4.0, 18.0, 18.0);
        let text_x = face.right() + 6.0;
        let text_w = rect.right() - 16.0 - text_x;
        match self.selected_account().cloned() {
            Some(account) => {
                match status.heads.get(&account.key()) {
                    Some(head) => cx.image(head, face),
                    None => cx.initial(face, &account.key(), account.username()),
                }
                cx.label(account.username(), text_x, rect.y + 4.0, text_w, theme::TEXT);
                let (key, color) = self.account_status(&account, status);
                cx.small_label(cx.t(key), text_x, rect.y + 15.0, text_w, color);
            }
            None => {
                cx.fill(face, theme::FIELD);
                cx.label(cx.t("account.none"), text_x, rect.y + 4.0, text_w, theme::DIM);
                cx.small_label(cx.t("account.add_hint"), text_x, rect.y + 15.0, text_w, theme::FAINT);
            }
        }
        cx.icon(icons::CHEVRON_DOWN, rect.right() - 12.0, rect.y + 11.0, theme::DIM);

        let names: Vec<String> = self.accounts.list().iter().map(|a| a.username().to_owned()).collect();
        let count = names.len();
        let current = self.accounts.selected_index();
        let manage = cx.t("account.manage").to_owned();
        let picked = cx.dropdown_over("account", rect, true, || {
            let mut items = names;
            items.push(manage);
            (items, current)
        });
        match picked {
            Some(index) if index < count => {
                if status.busy {
                    self.notify(cx.t("play.blocked.busy"));
                } else {
                    self.accounts.select(index);
                    self.save_accounts();
                }
            }
            Some(_) => self.page = Page::Accounts,
            None => {}
        }
    }

    /// The quick bar's left: what is new or wrong, most important first.
    fn info(&mut self, cx: &mut Cx, rect: Rect, status: &Status) {
        let mut lines: Vec<(String, theme::Color, Option<&'static str>)> = Vec::new();
        if let Some((text, at)) = &self.notice {
            if at.elapsed() < NOTICE {
                lines.push((text.clone(), theme::WARN, None));
            } else {
                self.notice = None;
            }
        }
        if let Some(id) = status.running {
            let name = self.instances.iter().find(|i| i.id == id).map_or(id, |i| i.name.as_str());
            lines.push((cx.tf("info.running", &[name]), theme::ACCENT, None));
        }
        if let Some(version) = catalog::launcher_update() {
            lines.push((cx.tf("info.launcher_update", &[&version]), theme::ACCENT, Some(RELEASES_URL)));
        }
        let latest = catalog::latest_game();
        if let Some(latest) = &latest
            && !self.disk.installed.is_empty()
            && !self.disk.installed.contains(latest)
        {
            lines.push((cx.tf("info.game_update", &[latest]), theme::ACCENT, None));
        }
        if status.catalog_checked && catalog::source() != catalog::Source::Online {
            let key = match catalog::source() {
                catalog::Source::None => "info.offline",
                _ => "info.offline_cached",
            };
            lines.push((cx.t(key).to_owned(), theme::WARN, None));
        }
        if !self.disk.assets_ready && self.selected().is_some_and(|i| !i.is_java()) {
            lines.push((cx.t("info.assets_missing").to_owned(), theme::WARN, None));
        }
        if lines.is_empty() {
            let version = cx.tf("info.version", &[env!("CARGO_PKG_VERSION")]);
            lines.push((version, theme::DIM, None));
            if let Some(latest) = &latest {
                lines.push((cx.tf("info.up_to_date", &[latest]), theme::FAINT, None));
            }
        }
        let shown = lines.len().min(2);
        let top = rect.y + (rect.h - shown as f32 * 11.0) / 2.0 + 1.0;
        for (i, (text, color, link)) in lines.iter().take(2).enumerate() {
            let y = top + i as f32 * 11.0;
            let line = Rect::new(rect.x, y - 1.0, rect.w, 11.0);
            let hot = link.is_some() && cx.hot(line);
            if hot {
                cx.pointer();
            }
            let color = if hot { theme::TEXT } else { *color };
            if i == 0 && cx.text_width(text) <= rect.w {
                cx.label(text, rect.x, y, rect.w, color);
            } else {
                cx.small_label(text, rect.x, y + 1.0, rect.w, color);
            }
            if let Some(url) = link
                && cx.clicked(line)
            {
                melon_base::platform::open_url(url);
            }
        }
    }

    pub fn open_prompt(&mut self, cx: &Cx, purpose: Purpose, value: &str) {
        let hint = match purpose {
            Purpose::AddOffline | Purpose::RenameOffline => "prompt.hint.profile",
            Purpose::GroupInstance => "prompt.hint.group",
            Purpose::NewShared | Purpose::RenameShared => "prompt.hint.shared",
            _ => "prompt.hint.instance",
        };
        let mut edit = TextField::new(cx.t(hint).to_owned());
        let max = match purpose {
            Purpose::AddOffline | Purpose::RenameOffline => melon_base::auth::accounts::MAX_NAME_LEN,
            _ => instances::MAX_NAME_LEN,
        };
        edit.set_max_length(max);
        edit.set_value(value);
        edit.set_focused(true);
        self.prompt = Some(Prompt {
            purpose,
            edit,
            error: false,
            java: false,
            subject: value.to_owned(),
        });
    }

    fn prompt_dialog(&mut self, cx: &mut Cx) {
        let Some(mut prompt) = self.prompt.take() else {
            return;
        };
        let new_instance = prompt.purpose == Purpose::NewInstance;
        let (w, h) = (236.0, if new_instance { 112.0 } else { 88.0 });
        let panel = Rect::new(
            ((cx.width() - w) / 2.0).round(),
            ((cx.height() - h) / 2.0).round(),
            w,
            h,
        );
        cx.frame(panel, theme::PANEL, theme::FAINT);
        let title = match prompt.purpose {
            Purpose::NewInstance => "prompt.new_instance",
            Purpose::RenameInstance => "prompt.rename_instance",
            Purpose::CopyInstance => "prompt.copy_instance",
            Purpose::GroupInstance => "prompt.group_instance",
            Purpose::AddOffline => "prompt.add_offline",
            Purpose::RenameOffline => "prompt.rename_offline",
            Purpose::NewShared => "prompt.new_shared",
            Purpose::RenameShared => "prompt.rename_shared",
        };
        let x = panel.x + 10.0;
        let inner = panel.w - 20.0;
        cx.text(cx.t(title), x, panel.y + 10.0, theme::TEXT);
        let mut y = panel.y + 26.0;
        let event = cx.field(&mut prompt.edit, Rect::new(x, y, inner, 16.0));
        if event.changed {
            prompt.error = false;
        }
        y += 22.0;
        if new_instance {
            let half = ((inner - GAP) / 2.0).floor();
            if cx.button(Rect::new(x, y, half, BUTTON_H), cx.t("engine.melon"), Btn::NORMAL.selected(!prompt.java)) {
                prompt.java = false;
            }
            if cx.button(
                Rect::new(x + half + GAP, y, half, BUTTON_H),
                cx.t("engine.java"),
                Btn::NORMAL.selected(prompt.java),
            ) {
                prompt.java = true;
            }
            y += BUTTON_H + 6.0;
        }
        if prompt.error {
            cx.small_label(cx.t("prompt.invalid"), x, y, inner, theme::DANGER);
        } else if prompt.purpose == Purpose::RenameOffline {
            cx.small_label(cx.t("prompt.rename_warning"), x, y, inner, theme::WARN);
        } else if prompt.purpose == Purpose::GroupInstance {
            cx.small_label(cx.t("prompt.group_note"), x, y, inner, theme::DIM);
        }
        let buttons_y = panel.bottom() - 10.0 - BUTTON_H;
        let ok = Rect::new(panel.right() - 10.0 - 70.0, buttons_y, 70.0, BUTTON_H);
        let cancel = Rect::new(ok.x - GAP - 70.0, buttons_y, 70.0, BUTTON_H);
        let submit = cx.button(ok, cx.t("common.ok"), Btn::PRIMARY) || event.submitted;
        if cx.button(cancel, cx.t("common.cancel"), Btn::NORMAL) {
            return;
        }
        if submit {
            if self.submit_prompt(&prompt) {
                return;
            }
            prompt.error = true;
        }
        self.prompt = Some(prompt);
    }

    /// A shared folder was made or renamed. False when that failed.
    fn shared_changed(&mut self, result: std::io::Result<()>) -> bool {
        let done = result.is_ok();
        if done {
            self.reload();
        }
        done
    }

    /// Does what the dialog asked for. False when the name cannot be used.
    fn submit_prompt(&mut self, prompt: &Prompt) -> bool {
        let value = prompt.edit.value();
        let name = value.trim();
        let home = self.home.clone();
        let selected = self.settings.instance.clone().unwrap_or_default();
        let made = match prompt.purpose {
            Purpose::NewInstance => {
                let engine = if prompt.java { Engine::Java } else { Engine::Melon };
                instances::create(&home, name, engine).ok()
            }
            Purpose::CopyInstance => instances::copy(&home, &selected, name).ok(),
            Purpose::RenameInstance => instances::rename(&home, &selected, name).ok(),
            Purpose::GroupInstance => instances::set_group(&home, &selected, name).ok(),
            Purpose::AddOffline => {
                if !self.accounts.add_offline(name) {
                    return false;
                }
                self.save_accounts();
                return true;
            }
            Purpose::RenameOffline => {
                let renamed = self
                    .accounts
                    .selected_index()
                    .is_some_and(|index| self.accounts.rename_offline(index, name));
                if renamed {
                    self.save_accounts();
                }
                return renamed;
            }
            Purpose::NewShared => return self.shared_changed(shared::create(&home, name)),
            Purpose::RenameShared => {
                let renamed = instances::rename_shared_folder(&home, &prompt.subject, name);
                return self.shared_changed(renamed);
            }
        };
        let Some(instance) = made else {
            return false;
        };
        self.settings.instance = Some(instance.id);
        self.settings.save(&home);
        self.reload();
        true
    }
}
