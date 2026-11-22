//! Signing in with a service other than Microsoft: Ely.by, LittleSkin, Drasl or any server
//! that speaks the authlib-injector API.

use super::kit::theme;
use super::kit::{BUTTON_H, Btn, Cx, GAP, Rect, TextField};
use melon_base::auth::accounts::AltAccount;
use melon_base::auth::alt_sessions::{self, PRESETS, Provider};
use melon_base::auth::{self, LoginEvent, LoginHandle, PendingLogin};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

const PANEL_W: f32 = 290.0;
const PAD: f32 = 10.0;
const FIELD_H: f32 = 16.0;
/// The dropdown entry after the presets.
const CUSTOM: usize = PRESETS.len();

/// What the dialog ended with this frame.
pub enum Outcome {
    Open,
    Cancelled,
    /// The account was written to `accounts.json` and selected.
    SignedIn,
}

pub struct ProviderDialog {
    preset: usize,
    address: TextField,
    login: TextField,
    password: TextField,
    code: TextField,
    /// Ely.by asked for the 2FA code.
    needs_code: bool,
    provider: Option<Provider>,
    /// The address being looked up, and its answer.
    resolving: Option<(String, Receiver<Result<Provider, String>>)>,
    resolve_error: Option<String>,
    /// Sign in as soon as the service is known.
    submit_when_known: bool,
    handle: Option<LoginHandle>,
    /// Several profiles: the user picks one.
    choosing: Option<PendingLogin>,
    error: Option<String>,
    game_dir: PathBuf,
    /// The panel's height in the last frame, to centre this one.
    height: f32,
}

fn field(hint: &str, max: usize) -> TextField {
    let mut field = TextField::new(hint.to_owned());
    field.set_max_length(max);
    field
}

impl ProviderDialog {
    pub fn new(cx: &Cx, game_dir: PathBuf) -> Self {
        let mut password = field(cx.t("provider.password"), 256);
        password.set_password(true);
        let mut dialog = Self {
            preset: 0,
            address: field(cx.t("provider.address_hint"), 256),
            login: field(cx.t("provider.login_hint"), 256),
            password,
            code: field(cx.t("provider.code_hint"), 12),
            needs_code: false,
            provider: None,
            resolving: None,
            resolve_error: None,
            submit_when_known: false,
            handle: None,
            choosing: None,
            error: None,
            game_dir,
            height: 220.0,
        };
        dialog.resolve(PRESETS[0].address.to_owned());
        dialog.login.set_focused(true);
        dialog
    }

    /// Signing in again to an account whose session ran out.
    pub fn again(cx: &Cx, game_dir: PathBuf, account: &AltAccount) -> Self {
        let mut dialog = Self::new(cx, game_dir);
        dialog.resolving = None;
        let root = account.provider.root.as_str();
        dialog.preset = PRESETS.iter().position(|p| p.address == root).unwrap_or(CUSTOM);
        if dialog.preset == CUSTOM {
            dialog.address.set_value(root);
        }
        dialog.provider = Some(account.provider.clone());
        dialog.login.set_value(&account.login);
        dialog.login.set_focused(false);
        dialog.password.set_focused(true);
        dialog
    }

    fn busy(&self) -> bool {
        self.handle.is_some() || (self.resolving.is_some() && self.submit_when_known)
    }

    /// Looks up the service at `address` in the background.
    fn resolve(&mut self, address: String) {
        self.provider = None;
        self.resolve_error = None;
        let (tx, rx) = mpsc::channel();
        let asked = address.clone();
        std::thread::spawn(move || {
            let _ = tx.send(alt_sessions::resolve(&asked).map_err(|e| format!("{e:#}")));
        });
        self.resolving = Some((address, rx));
    }

    fn address(&self) -> String {
        match PRESETS.get(self.preset) {
            Some(preset) => preset.address.to_owned(),
            None => self.address.value().trim().to_owned(),
        }
    }

    fn submit(&mut self) {
        self.error = None;
        if self.login.value().trim().is_empty() || self.password.value().is_empty() {
            self.error = Some("provider.missing".into());
            return;
        }
        let Some(provider) = self.provider.clone() else {
            self.submit_when_known = true;
            if self.resolving.is_none() {
                let address = self.address();
                if address.is_empty() {
                    self.error = Some("provider.no_address".into());
                    self.submit_when_known = false;
                    return;
                }
                self.resolve(address);
            }
            return;
        };
        let mut password = self.password.value();
        let code = self.code.value();
        if self.needs_code && !code.trim().is_empty() {
            password = format!("{password}:{}", code.trim());
        }
        let login = self.login.value().trim().to_owned();
        self.handle = Some(auth::start_alt_login(provider, login, password, self.game_dir.clone()));
    }

    /// Takes in what the background threads reported.
    fn poll(&mut self) -> Option<Outcome> {
        if let Some((_, rx)) = &self.resolving
            && let Ok(result) = rx.try_recv()
        {
            self.resolving = None;
            match result {
                Ok(provider) => {
                    self.provider = Some(provider);
                    if std::mem::take(&mut self.submit_when_known) {
                        self.submit();
                    }
                }
                Err(e) => {
                    self.resolve_error = Some(e);
                    self.submit_when_known = false;
                }
            }
        }
        let event = self.handle.as_ref()?.events.try_recv().ok()?;
        self.handle = None;
        match event {
            LoginEvent::Success { .. } => return Some(Outcome::SignedIn),
            LoginEvent::Error(message) => self.error = Some(message),
            LoginEvent::NeedsCode => {
                self.error = Some("provider.needs_code".into());
                self.needs_code = true;
                self.password.set_focused(false);
                self.code.set_focused(true);
            }
            LoginEvent::ChooseProfile(pending) => self.choosing = Some(pending),
            LoginEvent::DeviceCode(_) | LoginEvent::SessionExpired => {}
        }
        None
    }

    /// The dialog over everything else.
    pub fn frame(&mut self, cx: &mut Cx) -> Outcome {
        if let Some(outcome) = self.poll() {
            return outcome;
        }
        cx.shade();
        let width = PANEL_W.min(cx.width() - 16.0);
        let inner = width - 2.0 * PAD;
        let height = self.height;
        let rect = Rect::new(
            ((cx.width() - width) / 2.0).round(),
            ((cx.height() - height) / 2.0).round().max(4.0),
            width,
            height,
        );
        cx.frame(rect, theme::PANEL, theme::FAINT);
        let x = rect.x + PAD;
        let mut y = rect.y + PAD;
        cx.text(cx.t("provider.title"), x, y, theme::TEXT);
        y += 14.0;
        if self.choosing.is_some() {
            return self.profile_list(cx, rect, x, y, inner);
        }

        y = cx.section(cx.t("provider.service"), x, y, inner);
        let names: Vec<String> = PRESETS
            .iter()
            .map(|p| p.name.to_owned())
            .chain([cx.t("provider.custom").to_owned()])
            .collect();
        let label = names[self.preset].clone();
        let current = self.preset;
        let busy = self.busy();
        let picked = cx.dropdown("provider", Rect::new(x, y, inner, FIELD_H), &label, !busy, move || {
            (names, Some(current))
        });
        if let Some(index) = picked.filter(|i| *i != self.preset) {
            self.preset = index;
            self.needs_code = false;
            match PRESETS.get(index) {
                Some(preset) => self.resolve(preset.address.to_owned()),
                None => {
                    self.provider = None;
                    self.resolving = None;
                    self.resolve_error = None;
                    self.address.set_focused(true);
                }
            }
        }
        y += FIELD_H + 4.0;
        if self.preset == CUSTOM {
            let event = cx.field(&mut self.address, Rect::new(x + 1.0, y + 1.0, inner - 2.0, FIELD_H));
            if event.changed {
                self.provider = None;
                self.resolve_error = None;
            }
            if event.submitted && !self.address.value().trim().is_empty() {
                self.resolve(self.address.value().trim().to_owned());
            }
            y += FIELD_H + 4.0;
        }
        y = self.service_line(cx, x, y, inner);

        y = cx.section(cx.t("provider.account"), x, y + 4.0, inner);
        let mut submitted = false;
        let login_hint_key = match (PRESETS.get(self.preset), &self.provider) {
            (Some(preset), _) if preset.name_only => "provider.name_only",
            (_, Some(provider)) if !provider.name_login => "provider.email_only",
            _ => "",
        };
        submitted |= cx.field(&mut self.login, Rect::new(x + 1.0, y + 1.0, inner - 2.0, FIELD_H)).submitted;
        y += FIELD_H + 4.0;
        submitted |= cx.field(&mut self.password, Rect::new(x + 1.0, y + 1.0, inner - 2.0, FIELD_H)).submitted;
        y += FIELD_H + 4.0;
        if self.needs_code {
            submitted |= cx.field(&mut self.code, Rect::new(x + 1.0, y + 1.0, inner - 2.0, FIELD_H)).submitted;
            y += FIELD_H + 4.0;
        }
        if !login_hint_key.is_empty() {
            cx.small_label(cx.t(login_hint_key), x, y, inner, theme::DIM);
            y += 10.0;
        }
        let host = self.provider.as_ref().map_or_else(|| self.address(), |p| p.host().to_owned());
        y += cx.paragraph(&cx.tf("provider.password_note", &[&host]), x, y, inner, theme::WARN) + 4.0;
        if let Some(error) = &self.error {
            let text = if error.starts_with("provider.") {
                cx.tr.get(error).to_owned()
            } else {
                error.clone()
            };
            y += cx.paragraph(&text, x, y, inner, theme::DANGER);
        }
        self.height = (y - rect.y + 8.0 + BUTTON_H + PAD).round();

        let buttons_y = rect.bottom() - PAD - BUTTON_H;
        let ok = Rect::new(rect.right() - PAD - 100.0, buttons_y, 100.0, BUTTON_H);
        let cancel = Rect::new(ok.x - GAP - 70.0, buttons_y, 70.0, BUTTON_H);
        let label = if busy { "provider.signing_in" } else { "provider.sign_in" };
        if (cx.button(ok, cx.t(label), Btn::PRIMARY.enabled(!busy)) || submitted) && !busy {
            self.submit();
        }
        if cx.button(cancel, cx.t("common.cancel"), Btn::NORMAL) {
            return Outcome::Cancelled;
        }
        Outcome::Open
    }

    /// The service's name and its sign-up link, or how looking it up went. Returns the y below.
    fn service_line(&mut self, cx: &mut Cx, x: f32, y: f32, w: f32) -> f32 {
        let (text, color) = match (&self.provider, &self.resolving, &self.resolve_error) {
            (Some(provider), ..) => (cx.tf("provider.found", &[&provider.name]), theme::ACCENT),
            (None, Some(_), _) => (cx.t("provider.looking").to_owned(), theme::DIM),
            (None, None, Some(error)) => (error.clone(), theme::DANGER),
            (None, None, None) => (cx.t("provider.enter_address").to_owned(), theme::DIM),
        };
        let register = self.provider.as_ref().and_then(|p| p.register_url.clone());
        let link_w = match &register {
            Some(_) => cx.text_width(cx.t("provider.register")) * super::kit::SMALL + 4.0,
            None => 0.0,
        };
        let h = cx.paragraph(&text, x, y, w - link_w, color);
        if let Some(url) = register {
            let label = cx.t("provider.register");
            let line = Rect::new(x + w - link_w, y - 1.0, link_w, 10.0);
            let hot = cx.hot(line);
            if hot {
                cx.pointer();
            }
            cx.small(label, line.x + 4.0, y, if hot { theme::TEXT } else { theme::ACCENT });
            cx.tooltip(line, &url);
            if cx.clicked(line) {
                melon_base::platform::open_url(&url);
            }
        }
        y + h + 2.0
    }

    /// Picking one of several profiles.
    fn profile_list(&mut self, cx: &mut Cx, rect: Rect, x: f32, mut y: f32, w: f32) -> Outcome {
        let Some(pending) = self.choosing.clone() else {
            return Outcome::Open;
        };
        y += cx.paragraph(cx.t("provider.choose_profile"), x, y, w, theme::DIM) + 6.0;
        let busy = self.handle.is_some();
        for profile in &pending.session.available_profiles {
            if cx.button(Rect::new(x, y, w, BUTTON_H), &profile.name, Btn::NORMAL.enabled(!busy)) {
                self.handle = Some(auth::start_alt_select(
                    pending.clone(),
                    profile.clone(),
                    self.game_dir.clone(),
                ));
            }
            y += BUTTON_H + 4.0;
        }
        if let Some(error) = &self.error {
            y += cx.paragraph(error, x, y, w, theme::DANGER);
        }
        self.height = (y - rect.y + 8.0 + BUTTON_H + PAD).round();
        let cancel = Rect::new(rect.right() - PAD - 70.0, rect.bottom() - PAD - BUTTON_H, 70.0, BUTTON_H);
        if cx.button(cancel, cx.t("common.cancel"), Btn::NORMAL) {
            return Outcome::Cancelled;
        }
        Outcome::Open
    }
}
