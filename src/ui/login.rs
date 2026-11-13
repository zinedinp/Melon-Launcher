//! The Microsoft login and session check behind the launcher's account list:

use super::modal::{self, BODY_COLOR, PADDING, TITLE_COLOR};
use melon::auth::{self, LoginEvent, LoginHandle, oauth::DeviceCodeResponse};
use melon::gui::text::font::{self, Font};
use melon::gui::widgets::button::Button;
use melon::gui::widgets::{Rect, Ui};
use std::cell::Cell;
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

/// "Copied!" hold time on the code box.
const COPY_FEEDBACK: Duration = Duration::from_millis(1500);

const GAP: f32 = 6.0;
const CODE_BOX_H: f32 = 22.0;
const MODAL_BUTTON_W: f32 = 100.0;
const MODAL_BUTTON_H: f32 = 20.0;

/// Centered modal geometry, shared by `draw` and click handling.
struct ModalLayout {
    panel: Rect,
    body_start_y: f32,
    lines: Vec<String>,
    code_box: Option<Rect>,
    hint_y: f32,
    button: Rect,
}

fn modal_layout(
    width: f32,
    height: f32,
    font: &Font,
    body: &str,
    show_code_box: bool,
) -> ModalLayout {
    let panel_w = 240.0f32.min(width - 16.0).max(140.0);
    let inner_w = panel_w - PADDING * 2.0;
    let lines = font.wrap(body, inner_w);
    let body_h = lines.len().max(1) as f32 * font::LINE_HEIGHT;

    let mut content_h = font::LINE_HEIGHT + GAP + body_h + GAP;
    if show_code_box {
        content_h += CODE_BOX_H + GAP + font::LINE_HEIGHT + GAP;
    }
    content_h += MODAL_BUTTON_H;

    let panel_h = content_h + PADDING * 2.0;
    let panel_x = ((width - panel_w) / 2.0).round();
    let panel_y = ((height - panel_h) / 2.0).round();
    let panel = Rect::new(panel_x, panel_y, panel_w, panel_h);

    let body_start_y = panel_y + PADDING + font::LINE_HEIGHT + GAP;
    let mut y = body_start_y + body_h + GAP;

    let mut code_box = None;
    let mut hint_y = y;
    if show_code_box {
        code_box = Some(Rect::new(panel_x + PADDING, y, inner_w, CODE_BOX_H));
        y += CODE_BOX_H + GAP;
        hint_y = y;
        y += font::LINE_HEIGHT + GAP;
    }

    let button = Rect::new(
        panel_x + (panel_w - MODAL_BUTTON_W) / 2.0,
        y,
        MODAL_BUTTON_W,
        MODAL_BUTTON_H,
    );

    ModalLayout {
        panel,
        body_start_y,
        lines,
        code_box,
        hint_y,
        button,
    }
}

pub struct LoginFlow {
    state: LoginState,
    handle: Option<LoginHandle>,
    client_id: Option<String>,
    game_dir: PathBuf,
    last_mouse: (f32, f32),
    copied_at: Option<Instant>,
    /// The open modal's code box and button where they were last drawn,
    modal_targets: Cell<[Option<Rect>; 2]>,
}

impl LoginFlow {
    pub fn new(client_id: Option<String>, game_dir: PathBuf) -> Self {
        Self {
            state: LoginState::Idle,
            handle: None,
            client_id,
            game_dir,
            last_mouse: (0.0, 0.0),
            copied_at: None,
            modal_targets: Cell::new([None; 2]),
        }
    }

    pub fn update_hover(&mut self, mx: f32, my: f32) {
        self.last_mouse = (mx, my);
    }

    /// Whether the mouse is over something clickable in the open modal,
    pub fn modal_button_hovered(&self) -> bool {
        let (x, y) = self.last_mouse;
        self.modal_targets
            .get()
            .iter()
            .flatten()
            .any(|r| r.contains(x, y))
    }

    /// Login or session restore is in flight: Play must wait so it does not
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

    /// This build has Melon's own Azure app (env, compile-time, or data-folder file).
    pub fn has_client_id(&self) -> bool {
        self.client_id.is_some()
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

    /// Takes in what the background thread reported. A finished login or
    pub fn poll(&mut self) -> Option<Outcome> {
        loop {
            let handle = self.handle.as_ref()?;
            let Ok(event) = handle.events.try_recv() else {
                return None;
            };
            match event {
                LoginEvent::DeviceCode(device) => {
                    melon::platform::open_url(&device.verification_uri);
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
            }
        }
    }

    /// Modal is covering the menu; route clicks here.
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

    /// Modal click. Always consumed (nothing behind the modal reacts).
    pub fn on_modal_click(&mut self, x: f32, y: f32, width: f32, height: f32, font: &Font) {
        match &self.state {
            LoginState::WaitingForCode(device) => {
                let layout = modal_layout(width, height, font, &device.message, true);
                if let Some(code_box) = layout.code_box
                    && code_box.contains(x, y)
                {
                    melon::platform::copy_to_clipboard(&device.user_code);
                    self.copied_at = Some(Instant::now());
                }
                if layout.button.contains(x, y) {
                    self.handle = None;
                    self.state = LoginState::Idle;
                    self.copied_at = None;
                }
            }
            LoginState::Error(message) => {
                let layout = modal_layout(width, height, font, message, false);
                if layout.button.contains(x, y) {
                    self.state = LoginState::Idle;
                }
            }
            _ => {}
        }
    }

    /// The modal, over everything else.
    pub fn draw(&self, ui: &mut Ui) {
        self.modal_targets.set([None; 2]);
        match &self.state {
            LoginState::WaitingForCode(device) => {
                let layout = modal_layout(ui.width, ui.height, ui.font(), &device.message, true);
                let hovered_code = layout
                    .code_box
                    .is_some_and(|r| r.contains(self.last_mouse.0, self.last_mouse.1));
                let copied = self.copied_at.is_some_and(|t| t.elapsed() < COPY_FEEDBACK);
                self.draw_frame(ui, &layout, "melon.login.modal_title");
                self.modal_targets
                    .set([layout.code_box, Some(layout.button)]);

                if let Some(code_box) = layout.code_box {
                    let box_color = if copied {
                        [0.25, 0.5, 0.28, 1.0]
                    } else if hovered_code {
                        [0.22, 0.22, 0.28, 1.0]
                    } else {
                        [0.16, 0.16, 0.2, 1.0]
                    };
                    ui.fill(code_box, box_color);
                    let spaced: String = device
                        .user_code
                        .chars()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join("  ");
                    let text_w = ui.text_width(&spaced);
                    let tx = (code_box.x + (code_box.w - text_w) / 2.0).round();
                    let ty = (code_box.y + (code_box.h - font::LINE_HEIGHT) / 2.0).round();
                    ui.text(&spaced, tx, ty, [1.0; 4]);

                    let (hint, hint_color) = if copied {
                        (ui.tr("melon.login.copied"), [0.5, 0.9, 0.5, 1.0])
                    } else {
                        (ui.tr("melon.login.copy_hint"), [0.65, 0.65, 0.65, 1.0])
                    };
                    let center = layout.panel.x + layout.panel.w / 2.0;
                    ui.centered(hint, center, layout.hint_y, hint_color);
                }

                self.draw_modal_button(ui, &layout, "melon.login.cancel");
            }
            LoginState::Error(message) => {
                let layout = modal_layout(ui.width, ui.height, ui.font(), message, false);
                self.draw_frame(ui, &layout, "melon.login.error_title");
                self.modal_targets.set([None, Some(layout.button)]);
                self.draw_modal_button(ui, &layout, "melon.login.ok");
            }
            _ => {}
        }
    }

    /// The dimmed backdrop, the bordered panel, its title and body lines.
    fn draw_frame(&self, ui: &mut Ui, layout: &ModalLayout, title_key: &'static str) {
        let p = layout.panel;
        modal::draw_panel(ui, p);

        let title = ui.tr(title_key);
        ui.centered(title, p.x + p.w / 2.0, p.y + PADDING, TITLE_COLOR);
        for (i, line) in layout.lines.iter().enumerate() {
            ui.text(
                line,
                p.x + PADDING,
                layout.body_start_y + i as f32 * font::LINE_HEIGHT,
                BODY_COLOR,
            );
        }
    }

    fn draw_modal_button(&self, ui: &mut Ui, layout: &ModalLayout, label: &'static str) {
        let r = layout.button;
        let mut button = Button::new(label, r.x, r.y, r.w, r.h);
        button.hovered = r.contains(self.last_mouse.0, self.last_mouse.1);
        button.draw(ui);
    }
}
