//! The Microsoft login and session check behind the account list, and the popup that shows
//! the device code.

use super::kit::theme;
use super::kit::{BUTTON_H, Btn, Cx, GAP, LINE, Rect};
use melon_base::auth::{self, LoginEvent, LoginHandle, oauth::DeviceCodeResponse};
use std::path::PathBuf;
use std::time::{Duration, Instant};

enum LoginState {
    Idle,
    /// The device code is out; the user is entering it in the browser.
    WaitingForCode(DeviceCodeResponse),
    /// Between the click and the code, or after it: talking to Microsoft.
    LoggingIn,
    /// Refreshing the selected account's saved session.
    Restoring,
    Error(String),
}

/// How a finished login or session check ended.
pub enum Outcome {
    /// The selected account is signed in; `accounts.json` has its tokens.
    SignedIn,
    /// The saved session can't be refreshed: sign in again.
    Expired,
}

/// How long the code box says "Copied".
const COPY_FEEDBACK: Duration = Duration::from_millis(1500);
const PANEL_W: f32 = 250.0;
const PAD: f32 = 10.0;
const CODE_BOX_H: f32 = 22.0;

pub struct LoginFlow {
    state: LoginState,
    handle: Option<LoginHandle>,
    client_id: Option<String>,
    game_dir: PathBuf,
    copied_at: Option<Instant>,
}

impl LoginFlow {
    pub fn new(client_id: Option<String>, game_dir: PathBuf) -> Self {
        Self {
            state: LoginState::Idle,
            handle: None,
            client_id,
            game_dir,
            copied_at: None,
        }
    }

    /// A login or session check is in flight: Play waits, so a rotating refresh token is not
    /// used twice.
    pub fn is_busy(&self) -> bool {
        self.handle.is_some()
    }

    /// Checks the selected account's saved session in the background.
    pub fn start_restore(&mut self) {
        self.state = LoginState::Restoring;
        self.handle = Some(auth::start_restore(
            self.client_id.clone(),
            self.game_dir.clone(),
        ));
    }

    /// Starts the device-code login for a new (or expired) account.
    pub fn start_login(&mut self) {
        let Some(client_id) = self.client_id.clone() else {
            self.state = LoginState::Error(auth::SIGN_IN_NOT_CONFIGURED.to_string());
            return;
        };
        self.state = LoginState::LoggingIn;
        self.handle = Some(auth::start_login(client_id, self.game_dir.clone()));
    }

    /// Takes in what the background thread reported.
    pub fn poll(&mut self) -> Option<Outcome> {
        loop {
            let handle = self.handle.as_ref()?;
            let Ok(event) = handle.events.try_recv() else {
                return None;
            };
            match event {
                LoginEvent::DeviceCode(device) => {
                    melon_base::platform::open_url(&device.verification_uri);
                    self.state = LoginState::WaitingForCode(device);
                    self.copied_at = None;
                }
                LoginEvent::Success { .. } => {
                    self.state = LoginState::Idle;
                    self.handle = None;
                    return Some(Outcome::SignedIn);
                }
                LoginEvent::Error(message) => {
                    self.state = LoginState::Error(message);
                    self.handle = None;
                }
                LoginEvent::SessionExpired => {
                    self.state = LoginState::Idle;
                    self.handle = None;
                    return Some(Outcome::Expired);
                }
                // Only the sign-in with another service asks these; it has its own dialog.
                LoginEvent::ChooseProfile(_) | LoginEvent::NeedsCode => {
                    self.state = LoginState::Idle;
                    self.handle = None;
                }
            }
        }
    }

    /// The popup covers the window.
    pub fn is_modal_open(&self) -> bool {
        matches!(
            self.state,
            LoginState::WaitingForCode(_) | LoginState::Error(_)
        )
    }

    /// The selected account's saved session is being checked.
    pub fn is_restoring(&self) -> bool {
        matches!(self.state, LoginState::Restoring)
    }

    /// The popup, over everything else.
    pub fn frame(&mut self, cx: &mut Cx) {
        let (title, body, code) = match &self.state {
            LoginState::WaitingForCode(device) => (
                "login.title",
                device.message.clone(),
                Some(device.user_code.clone()),
            ),
            LoginState::Error(message) => ("login.error_title", message.clone(), None),
            _ => return,
        };
        cx.shade();
        let width = PANEL_W.min(cx.width() - 16.0);
        let inner = width - PAD * 2.0;
        let lines = cx.ui.wrap(&body, inner);
        let body_h = lines.len().max(1) as f32 * LINE;
        let code_h = if code.is_some() {
            CODE_BOX_H + GAP + LINE + GAP
        } else {
            0.0
        };
        let height = PAD * 2.0 + LINE + GAP + body_h + GAP + code_h + BUTTON_H;
        let rect = Rect::new(
            ((cx.width() - width) / 2.0).round(),
            ((cx.height() - height) / 2.0).round(),
            width,
            height,
        );
        cx.frame(rect, theme::PANEL, theme::FAINT);
        let x = rect.x + PAD;
        let mut y = rect.y + PAD;
        cx.text(cx.t(title), x, y, theme::TEXT);
        y += LINE + GAP;
        for (i, line) in lines.iter().enumerate() {
            cx.text(line, x, y + i as f32 * LINE, theme::DIM);
        }
        y += body_h + GAP;
        if let Some(code) = &code {
            let code_box = Rect::new(x, y, inner, CODE_BOX_H);
            let copied = self.copied_at.is_some_and(|t| t.elapsed() < COPY_FEEDBACK);
            let hot = cx.hot(code_box);
            if hot {
                cx.pointer();
            }
            let edge = match (copied, hot) {
                (true, _) => theme::ACCENT,
                (false, true) => theme::FAINT,
                (false, false) => theme::EDGE,
            };
            cx.frame(code_box, theme::FIELD, edge);
            let spaced: String = code
                .chars()
                .map(String::from)
                .collect::<Vec<_>>()
                .join("  ");
            cx.centered(&spaced, code_box, theme::TEXT);
            let (hint, color) = match copied {
                true => ("common.copied", theme::ACCENT),
                false => ("login.copy_hint", theme::DIM),
            };
            let hint = cx.t(hint);
            let hint_w = cx.text_width(hint) * super::kit::SMALL;
            cx.small(hint, rect.x + (rect.w - hint_w) / 2.0, y + CODE_BOX_H + GAP, color);
            if cx.clicked(code_box) {
                melon_base::platform::copy_to_clipboard(code);
                self.copied_at = Some(Instant::now());
            }
        }
        let button = Rect::new(
            rect.right() - PAD - 90.0,
            rect.bottom() - PAD - BUTTON_H,
            90.0,
            BUTTON_H,
        );
        let label = if code.is_some() { "common.cancel" } else { "common.ok" };
        if cx.button(button, cx.t(label), Btn::NORMAL) {
            self.handle = None;
            self.state = LoginState::Idle;
            self.copied_at = None;
        }
    }
}
