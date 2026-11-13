//! The launcher window: instances on the left, mods/settings in the

use super::accounts::{self, AccountPanel};
use super::heads::Heads;
use super::instance_detail::{self, InstanceDetail};
use super::instance_page::{self, InstancePage};
use super::login::Outcome;
use super::mods_page::ModsPage;
use super::name_entry::{self, NameEntry, Purpose};
use super::settings_page::{self, SettingsPage};
use super::welcome::{self, Welcome};
use melon::auth::accounts::Account;
use crate::cdn;
use melon::gui::background;
use melon::gui::screen::{CursorKind, TextEvent};
use melon::gui::widgets::button::Button;
use melon::gui::widgets::tab_bar::{self, TabBar};
use melon::gui::widgets::{Rect, Ui};
use melon::i18n::languages::Language;
use melon::instances::{self, Engine, LauncherSettings};
use std::path::{Path, PathBuf};

/// The width of the lists and the rows of buttons.
pub const ROW_W: f32 = 300.0;
pub const BUTTON_H: f32 = 20.0;
pub const GAP: f32 = 6.0;
const CLOSE_W: f32 = 70.0;
const HEADER_H: f32 = 36.0;
const SIDE_W: f32 = 210.0;
const ACCOUNT_W: f32 = 230.0;
const PAD: f32 = 8.0;

const TABS: [&str; 3] = [
    "melon.launcher.tab.instances",
    "melon.launcher.tab.mods",
    "melon.launcher.tab.settings",
];

/// What starting the game needs to know.
pub struct PlayRequest {
    /// The instance's name; none for the main game folder.
    pub instance: Option<String>,
    pub validation: bool,
    /// A server to join straight away.
    pub join: Option<String>,
    /// The account playing, to count its play time.
    pub account_key: Option<String>,
}

pub enum Action {
    None,
    Play(PlayRequest),
    /// Start the Microsoft device-code login.
    SignIn,
    /// Unzip a local 26.3 client into Melon's assets folder.
    PrepareLocal,
    /// The language setting changed.
    LanguageChanged(Language),
    /// Fetch the published Melon client into the melon folder.
    DownloadClient,
    /// Close the launcher.
    Close,
}

pub struct LauncherUi {
    main_dir: PathBuf,
    settings: LauncherSettings,
    tabs: TabBar,
    accounts: AccountPanel,
    instances: InstancePage,
    detail: InstanceDetail,
    mods: ModsPage,
    options: SettingsPage,
    names: NameEntry,
    play: Button,
    close: Button,
    join: Button,
    /// The server the selected instance joined last.
    last_server: Option<String>,
    welcome: Option<Welcome>,
    /// Mojang assets are on disk; Play may start a Melon instance.
    prepared: bool,
    busy: bool,
    size: (f32, f32),
    mouse: (f32, f32),
    instance_frame: Rect,
    center_frame: Rect,
    account_frame: Rect,
}

impl LauncherUi {
    pub fn new(main_dir: &Path) -> Self {
        let settings = LauncherSettings::load(main_dir);
        let instances = InstancePage::new(main_dir.to_path_buf(), &settings);
        let detail = InstanceDetail::new(main_dir.to_path_buf());
        let game_dir = game_dir_of(main_dir, &settings);
        let extract_dir = settings.melon_extract(main_dir);
        let play_assets = settings.melon_play_assets(main_dir);
        let prepared = melon::home::is_prepared_at(&play_assets);
        let welcome = melon::prepare::needs_extract(main_dir, &play_assets)
            .then(|| Welcome::new(main_dir, extract_dir));
        let mut window = Self {
            main_dir: main_dir.to_path_buf(),
            options: SettingsPage::new(main_dir, instances.selected()),
            mods: ModsPage::new(&game_dir),
            last_server: instances::find(
                main_dir,
                &settings
                    .instance_in(main_dir)
                    .unwrap_or_else(|| instances::MELON.into()),
            )
            .and_then(|i| instances::quick_join(&i)),
            settings,
            tabs: TabBar::new(&TABS),
            accounts: AccountPanel::new(main_dir.to_path_buf()),
            instances,
            detail,
            names: NameEntry::new(),
            play: Button::sized("melon.launcher.play", ROW_W - GAP - CLOSE_W, BUTTON_H),
            close: Button::sized("melon.launcher.close", CLOSE_W, BUTTON_H),
            join: Button::sized("melon.launcher.join", ROW_W, BUTTON_H),
            welcome,
            prepared,
            busy: false,
            size: (0.0, 0.0),
            mouse: (-1.0, -1.0),
            instance_frame: Rect::default(),
            center_frame: Rect::default(),
            account_frame: Rect::default(),
        };
        window.refresh_instance();
        window
    }

    pub fn main_dir(&self) -> &Path {
        &self.main_dir
    }

    pub fn accounts(&self) -> &[Account] {
        self.accounts.accounts()
    }

    pub fn needs_check(&self) -> bool {
        self.accounts.needs_check()
    }

    pub fn language(&self) -> Language {
        self.options.language()
    }

    pub fn finished(&mut self, outcome: Outcome) {
        self.accounts.finished(outcome);
        self.refresh();
    }

    /// The game ended after `secs` with the account `key`.
    pub fn record_play(&mut self, key: &str, secs: u64) {
        self.accounts.record_play(key, secs);
    }

    pub fn set_busy(&mut self, busy: bool) {
        self.busy = busy;
        self.accounts.set_busy(busy);
        self.refresh();
    }

    /// Catalog changed: Download may appear or hide.
    pub fn note_discover(&mut self) {
        self.refresh();
    }

    pub fn resize(&mut self, size: (f32, f32)) {
        self.size = size;
        let (width, height) = size;
        let account_x = (width - ACCOUNT_W - PAD).max(PAD);
        let account_h = (height - PAD * 2.0).max(1.0);
        self.account_frame = Rect::new(account_x, PAD, ACCOUNT_W, account_h);
        self.accounts
            .resize_column(account_x, PAD, ACCOUNT_W, account_h);

        self.play.x = PAD;
        self.play.y = PAD + (HEADER_H - BUTTON_H) / 2.0;
        self.play.w = 180.0;
        self.join.x = PAD + 180.0 + GAP;
        self.join.y = self.play.y;
        self.join.w = 140.0;
        self.close.x = account_x - CLOSE_W - GAP;
        self.close.y = self.play.y;
        self.close.w = CLOSE_W;

        let body_y = PAD + HEADER_H + GAP;
        let body_h = (height - body_y - PAD).max(1.0);
        let side_x = PAD;
        self.instance_frame = Rect::new(side_x, body_y, SIDE_W, body_h);
        self.instances.resize_column(side_x, body_y, SIDE_W, body_h);

        let center_x = side_x + SIDE_W + GAP;
        let center_w = (account_x - GAP - center_x).max(1.0);
        self.center_frame = Rect::new(center_x, body_y, center_w, body_h);
        self.tabs.resize_at(center_x, body_y, center_w);
        let page_y = body_y + tab_bar::HEIGHT + 8.0;
        let page_h = (height - PAD - page_y).max(1.0);
        self.detail.resize_in(center_x, page_y, center_w, page_h);
        self.mods.resize_in(center_x, page_y, center_w, page_h);
        self.options.resize_in(center_x, page_y, center_w, page_h);
        if let Some(welcome) = &mut self.welcome {
            let welcome_w = (account_x - PAD * 2.0).max(1.0);
            welcome.resize(PAD, body_y, welcome_w, body_h);
        }
        self.layout_names();
    }

    fn layout_names(&mut self) {
        match self.names.purpose() {
            Purpose::AddOffline | Purpose::RenameOffline => {
                self.names.layout(
                    self.accounts.column_x(),
                    self.accounts.add_row_y(),
                    ACCOUNT_W,
                );
            }
            Purpose::NewInstance => {
                self.names.layout(
                    self.instances.column_x(),
                    self.instances.add_row_y(),
                    SIDE_W,
                );
            }
            Purpose::CopyInstance | Purpose::RenameInstance => {
                let (x, y, w) = self.detail.name_row();
                self.names.layout(x, y, w);
            }
        }
    }

    pub fn welcome_mut(&mut self) -> Option<&mut Welcome> {
        self.welcome.as_mut()
    }

    pub fn dismiss_welcome(&mut self) {
        self.welcome = None;
        self.resize(self.size);
        self.refresh();
    }

    pub fn set_prepared(&mut self, prepared: bool) {
        self.prepared = prepared;
        self.refresh();
    }

    fn set_assets_base(&mut self, path: PathBuf) {
        self.settings.set_assets_path(&self.main_dir, path);
        let extract = self.settings.melon_extract(&self.main_dir);
        if let Some(welcome) = &mut self.welcome {
            welcome.set_assets_dir(extract);
        }
        self.set_prepared(melon::home::is_prepared_at(
            &self.settings.melon_play_assets(&self.main_dir),
        ));
    }

    /// Play waits for a login or check (a rotating refresh token must not
    fn refresh(&mut self) {
        let can_play = !self.busy
            && !self.accounts.play_blocked()
            && self.welcome.is_none()
            && self.instance_playable();
        self.play.enabled = can_play;
        if self.wants_download() {
            self.join.enabled = !self.busy && self.welcome.is_none();
        } else {
            self.join.enabled = can_play && self.last_server.is_some();
        }
    }

    fn is_melon_latest(&self) -> bool {
        self.selected_instance()
            .is_some_and(|i| i.id == instances::MELON)
    }

    /// Melon latest, and the published client is not in the melon folder yet.
    fn wants_download(&self) -> bool {
        self.is_melon_latest() && !cdn::discover::client_is_latest(&self.main_dir)
    }

    fn selected_instance(&self) -> Option<instances::Instance> {
        let id = self
            .settings
            .instance_in(&self.main_dir)
            .unwrap_or_else(|| instances::MELON.into());
        instances::find(&self.main_dir, &id)
    }

    fn instance_playable(&self) -> bool {
        match self.selected_instance() {
            Some(instance) if instance.engine == Engine::Java => {
                crate::java_play::ready(&self.main_dir, &instance)
            }
            _ => self.prepared,
        }
    }

    fn refresh_instance(&mut self) {
        let game_dir = game_dir_of(&self.main_dir, &self.settings);
        self.last_server = self
            .selected_instance()
            .and_then(|i| instances::quick_join(&i));
        self.options
            .set_instance(&self.main_dir, self.instances.selected());
        self.mods.set_instance(&game_dir);
        self.detail.set_instance(self.instances.selected().cloned());
        self.refresh();
    }

    fn request(&self, join: Option<String>) -> PlayRequest {
        PlayRequest {
            instance: self.settings.instance_in(&self.main_dir),
            validation: self.settings.validation,
            join,
            account_key: self.accounts.selected().map(Account::key),
        }
    }

    fn play_action(&self) -> Action {
        if self.play.enabled {
            Action::Play(self.request(None))
        } else {
            Action::None
        }
    }

    /// The pointer moved. `active` is false under a modal.
    pub fn hover(&mut self, x: f32, y: f32, active: bool) {
        let (px, py) = if active {
            (x, y)
        } else {
            (f32::NEG_INFINITY, f32::NEG_INFINITY)
        };
        self.mouse = (px, py);
        self.play.update_hover(px, py);
        self.join.update_hover(px, py);
        self.close.update_hover(px, py);
        self.names.hover(px, py);
        self.accounts.hover(x, y, active);
        if let Some(welcome) = &mut self.welcome {
            welcome.hover(px, py);
            return;
        }
        self.tabs.on_mouse_move(px, py);
        self.instances.hover(x, y, active);
        self.detail.hover(x, y, active);
        self.mods.hover(x, y, active);
        self.options.hover(x, y, active);
    }

    pub fn cursor(&self) -> CursorKind {
        let footer = [&self.play, &self.join, &self.close]
            .iter()
            .any(|b| b.enabled && b.hovered);
        if footer || self.tabs.hovered() {
            return CursorKind::Pointer;
        }
        if self.names.is_open()
            && let Some(cursor) = self.names.cursor()
        {
            return cursor;
        }
        if let Some(welcome) = &self.welcome {
            let naming = self.names.is_open();
            let over_accounts = self.accounts.cursor(naming);
            if over_accounts != CursorKind::Default {
                return over_accounts;
            }
            if self.close.hovered {
                return CursorKind::Pointer;
            }
            return welcome.cursor();
        }
        let naming = self.names.is_open();
        let over_side = self.instances.cursor(naming);
        if over_side != CursorKind::Default {
            return over_side;
        }
        let over_accounts = self.accounts.cursor(naming);
        if over_accounts != CursorKind::Default {
            return over_accounts;
        }
        match self.tabs.selected() {
            0 => self.detail.cursor(),
            1 => self.mods.cursor(),
            _ => self.options.cursor(),
        }
    }

    pub fn release(&mut self) {
        self.accounts.release();
        self.instances.release();
        self.detail.release();
        self.mods.release();
    }

    pub fn scroll(&mut self, delta: f32) {
        let x = self.mouse.0;
        let account_x = (self.size.0 - ACCOUNT_W - PAD).max(PAD);
        if x >= account_x {
            self.accounts.scroll(delta);
            return;
        }
        if self.welcome.is_some() {
            return;
        }
        if x < PAD + SIDE_W {
            self.instances.scroll(delta);
        } else {
            match self.tabs.selected() {
                0 => self.detail.scroll(delta),
                1 => self.mods.scroll(delta),
                _ => {}
            }
        }
    }

    pub fn click(&mut self, x: f32, y: f32) -> Action {
        if self.names.is_open() {
            match self.names.click(x, y) {
                name_entry::Click::Submitted(name) => {
                    self.submit_name(&name);
                    return Action::None;
                }
                name_entry::Click::Inside => return Action::None,
                name_entry::Click::Outside => {}
            }
        }
        if self.welcome.is_some() {
            if self.close.clicked(x, y) {
                return Action::Close;
            }
            if let Some(welcome) = &mut self.welcome {
                match welcome.click(x, y) {
                    welcome::Action::Account => return Action::SignIn,
                    welcome::Action::Extract => return Action::PrepareLocal,
                    welcome::Action::SetAssetsDir(path) => {
                        self.set_assets_base(path);
                        return Action::None;
                    }
                    welcome::Action::None => {}
                }
            }
            let naming = self.names.is_open();
            let action = self.click_accounts(x, y, naming);
            self.refresh();
            return action;
        }
        if let Some(tab) = self.tabs.on_click(x, y) {
            self.tabs.select(tab);
            self.names.close();
            return Action::None;
        }
        if self.close.clicked(x, y) {
            return Action::Close;
        }
        if self.play.clicked(x, y) {
            return self.play_action();
        }
        if self.join.clicked(x, y) {
            if self.wants_download() {
                return Action::DownloadClient;
            }
            if self.is_melon_latest() {
                return Action::None;
            }
            return Action::Play(self.request(self.last_server.clone()));
        }
        let naming = self.names.is_open();
        let account_x = (self.size.0 - ACCOUNT_W - PAD).max(PAD);
        let action = if x >= account_x {
            self.click_accounts(x, y, naming)
        } else if x < PAD + SIDE_W {
            self.click_instances(x, y, naming)
        } else {
            match self.tabs.selected() {
                0 => self.click_detail(x, y, naming),
                1 => {
                    self.mods.click(x, y);
                    Action::None
                }
                _ => self.click_settings(x, y, naming),
            }
        };
        self.refresh();
        action
    }

    fn click_accounts(&mut self, x: f32, y: f32, naming: bool) -> Action {
        match self.accounts.click(x, y, naming) {
            accounts::Action::None => Action::None,
            accounts::Action::Play => {
                self.refresh();
                self.play_action()
            }
            accounts::Action::SignIn => Action::SignIn,
            accounts::Action::NameNewProfile => {
                self.names.open(Purpose::AddOffline, "");
                self.layout_names();
                Action::None
            }
            accounts::Action::NameRename(current) => {
                self.names.open(Purpose::RenameOffline, &current);
                self.layout_names();
                Action::None
            }
        }
    }

    fn click_instances(&mut self, x: f32, y: f32, naming: bool) -> Action {
        let action = self.instances.click(x, y, naming, &mut self.settings);
        self.refresh_instance();
        match action {
            instance_page::Action::None => Action::None,
            instance_page::Action::Play => self.play_action(),
            instance_page::Action::NameNew => {
                self.names.open(Purpose::NewInstance, "");
                self.layout_names();
                Action::None
            }
        }
    }

    fn click_detail(&mut self, x: f32, y: f32, naming: bool) -> Action {
        match self.detail.click(x, y, naming) {
            instance_detail::Action::None => Action::None,
            instance_detail::Action::NameRename => {
                let current = self.instances.selected_name().unwrap_or("");
                self.names.open(Purpose::RenameInstance, current);
                self.layout_names();
                Action::None
            }
            instance_detail::Action::NameCopy => {
                self.names.open(Purpose::CopyInstance, "");
                self.layout_names();
                Action::None
            }
            instance_detail::Action::Deleted => {
                self.settings.instance = Some(instances::MELON.into());
                self.settings.save(&self.main_dir);
                self.instances.reload(&self.settings);
                self.refresh_instance();
                Action::None
            }
            instance_detail::Action::Changed => {
                self.instances.reload(&self.settings);
                self.refresh_instance();
                Action::None
            }
            instance_detail::Action::DownloadClient => Action::DownloadClient,
        }
    }

    fn click_settings(&mut self, x: f32, y: f32, naming: bool) -> Action {
        let action = match self
            .options
            .click(x, y, &mut self.settings, &self.main_dir, naming)
        {
            settings_page::Action::None => Action::None,
            settings_page::Action::LanguageChanged => {
                Action::LanguageChanged(self.options.language())
            }
        };
        self.set_prepared(melon::home::is_prepared_at(
            &self.settings.melon_play_assets(&self.main_dir),
        ));
        action
    }

    pub fn key(&mut self, event: TextEvent) -> Action {
        if self.names.is_open() {
            if let Some(name) = self.names.key(event) {
                self.submit_name(&name);
            }
            return Action::None;
        }
        if self.tabs.selected() == 2 && self.options.key(&event, &mut self.settings, &self.main_dir)
        {
            self.set_prepared(melon::home::is_prepared_at(
                &self.settings.melon_play_assets(&self.main_dir),
            ));
            return Action::None;
        }
        if let Some(welcome) = &mut self.welcome
            && let welcome::Action::Extract = welcome.key(&event)
        {
            return Action::PrepareLocal;
        }
        let account_x = (self.size.0 - ACCOUNT_W - PAD).max(PAD);
        let action = if self.welcome.is_some() || self.mouse.0 >= account_x {
            match self.accounts.key(event) {
                accounts::Action::Play => self.play_action(),
                _ => Action::None,
            }
        } else {
            let action = self.instances.key(event, &mut self.settings);
            self.refresh_instance();
            match action {
                instance_page::Action::Play => self.play_action(),
                _ => Action::None,
            }
        };
        self.refresh();
        action
    }

    /// Escape: gives up typing a name.
    pub fn cancel(&mut self) {
        self.names.close();
    }

    fn submit_name(&mut self, name: &str) {
        match self.names.purpose() {
            Purpose::AddOffline => self.accounts.add_offline(name),
            Purpose::RenameOffline => self.accounts.rename_selected(name),
            Purpose::NewInstance => {
                self.instances.create(name, &mut self.settings);
                self.refresh_instance();
            }
            Purpose::CopyInstance => {
                self.instances.copy_selected(name, &mut self.settings);
                self.refresh_instance();
            }
            Purpose::RenameInstance => {
                self.instances.rename_selected(name, &mut self.settings);
                self.refresh_instance();
            }
        }
        self.names.close();
        self.refresh();
    }

    pub fn draw(&self, ui: &mut Ui, heads: &Heads, restoring: bool) {
        let naming = self.names.is_open();
        let naming_account = naming
            && matches!(
                self.names.purpose(),
                Purpose::AddOffline | Purpose::RenameOffline
            );
        ui.panel(
            self.account_frame,
            background::LAUNCHER_PANEL,
            background::LAUNCHER_EDGE,
        );
        self.accounts.draw(ui, heads, restoring, naming_account);
        if let Some(welcome) = &self.welcome {
            let welcome_frame = Rect::new(
                PAD,
                self.instance_frame.y,
                (self.account_frame.x - PAD - GAP).max(1.0),
                self.instance_frame.h,
            );
            ui.panel(
                welcome_frame,
                background::LAUNCHER_PANEL,
                background::LAUNCHER_EDGE,
            );
            welcome.draw(ui);
            self.close.draw(ui);
        } else {
            ui.panel(
                self.instance_frame,
                background::LAUNCHER_PANEL,
                background::LAUNCHER_EDGE,
            );
            ui.panel(
                self.center_frame,
                background::LAUNCHER_PANEL,
                background::LAUNCHER_EDGE,
            );
            self.instances.draw(ui, naming && !naming_account);
            self.tabs.draw(ui);
            match self.tabs.selected() {
                0 => self.detail.draw(ui),
                1 => self.mods.draw(ui),
                _ => self.options.draw(ui, &self.settings, &self.main_dir),
            }
            self.draw_header(ui);
        }
        if naming {
            self.names.draw(ui);
        }
    }

    fn draw_header(&self, ui: &mut Ui) {
        if self.wants_download() {
            self.join.draw_labeled(ui, ui.tr("melon.launcher.download"));
        } else if let Some(address) = &self.last_server {
            let label = ui
                .lang
                .format("melon.launcher.join_server", &[&shorten(address, 30)]);
            self.join.draw_labeled(ui, &label);
        }
        let label = match self.accounts.selected() {
            Some(account) => ui
                .lang
                .format("melon.launcher.play_as", &[account.username()]),
            None => ui.tr("melon.launcher.play").to_owned(),
        };
        self.play.draw_labeled(ui, &label);
        self.close.draw(ui);
    }
}

/// The folder of the selected instance.
fn game_dir_of(main_dir: &Path, settings: &LauncherSettings) -> PathBuf {
    instances::selected_dir(main_dir, settings)
}

/// `text` cut to `max` characters, with `…` for the cut part.
fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let head: String = text.chars().take(max - 1).collect();
    format!("{head}…")
}
