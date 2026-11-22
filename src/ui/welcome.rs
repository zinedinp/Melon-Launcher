//! The first run in a fresh Melon folder: the language, the Minecraft jar the assets come
//! from, and a first account. The jar and the account can be skipped; the launcher asks for
//! them again before the game starts.

use super::jar::JarSource;
use super::kit::theme;
use super::kit::{BUTTON_H, Btn, Cx, GAP, Rect};
use super::shell::{Action, Purpose, Shell, Status};
use crate::lang::LANGUAGES;
use crate::play::Install;
use melon_base::prepare;

const CARD_W: f32 = 360.0;
const PAD: f32 = 14.0;
const NEXT_W: f32 = 170.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Client,
    Account,
}

pub struct Welcome {
    step: Step,
    /// Assets are being extracted; the account step follows once they are ready.
    extracting: bool,
    /// How many accounts there were when the account step opened.
    accounts_before: usize,
    /// The card's height in the last frame, to centre this one.
    height: f32,
}

impl Welcome {
    pub fn new() -> Self {
        Self {
            step: Step::Client,
            extracting: false,
            accounts_before: 0,
            height: 240.0,
        }
    }
}

/// Skip's button: as wide as its text, so it stays quieter than the next step.
fn skip_width(cx: &Cx) -> f32 {
    (cx.text_width(cx.t("welcome.skip")) * super::kit::SMALL + 14.0).ceil()
}

impl Shell {
    /// The first-run setup, over the whole window.
    pub(super) fn welcome_page(&mut self, cx: &mut Cx, status: &Status, out: &mut Vec<Action>) {
        let Some(mut welcome) = self.welcome.take() else {
            return;
        };
        self.settings_menu.jar.poll();
        if welcome.extracting && !status.busy {
            welcome.extracting = false;
            if self.disk.assets_ready {
                self.to_account_step(&mut welcome);
            }
        }
        if welcome.step == Step::Account && self.accounts.list().len() > welcome.accounts_before {
            self.finish_welcome();
            return;
        }

        let logo = cx.height() >= welcome.height + 64.0;
        let block = welcome.height + if logo { 30.0 } else { 0.0 };
        let top = ((cx.height() - block) / 2.0).max(4.0).round();
        let x = ((cx.width() - CARD_W) / 2.0).round();
        if logo {
            let scale = 2.0;
            let w = cx.text_width("Melon") * scale;
            cx.ui.text_scaled("Melon", (x + (CARD_W - w) / 2.0).round(), top, theme::ACCENT, scale);
        }
        let card = Rect::new(x, if logo { top + 30.0 } else { top }, CARD_W, welcome.height);
        cx.frame(card, theme::PANEL, theme::EDGE);
        let (inner_x, inner_w) = (card.x + PAD, card.w - 2.0 * PAD);

        let title = match welcome.step {
            Step::Client => "welcome.title",
            Step::Account => "welcome.account_title",
        };
        cx.text(cx.t(title), inner_x, card.y + PAD, theme::TEXT);
        for (i, step) in [Step::Client, Step::Account].iter().enumerate() {
            let reached = *step == Step::Client || welcome.step == Step::Account;
            let bar = Rect::new(card.right() - PAD - 26.0 + i as f32 * 14.0, card.y + PAD + 3.0, 12.0, 2.0);
            cx.fill(bar, if reached { theme::ACCENT } else { theme::EDGE });
        }
        let y = card.y + PAD + 14.0;
        let (bottom, finished) = match welcome.step {
            Step::Client => self.welcome_client(cx, &mut welcome, inner_x, y, inner_w, status, out),
            Step::Account => self.welcome_account(cx, &mut welcome, inner_x, y, inner_w, status, out),
        };
        welcome.height = (bottom + PAD - card.y).round();
        if finished {
            self.finish_welcome();
        } else {
            self.welcome = Some(welcome);
        }
    }

    fn to_account_step(&self, welcome: &mut Welcome) {
        welcome.step = Step::Account;
        welcome.accounts_before = self.accounts.list().len();
    }

    /// The setup is over: from now on the launcher opens on the instances.
    fn finish_welcome(&mut self) {
        self.welcome = None;
        self.settings.welcome = false;
        self.settings.save(&self.home);
    }

    /// The row with the less prominent Skip on the left of the next step's button.
    fn welcome_buttons(cx: &mut Cx, x: f32, y: f32, w: f32, next: &str, next_btn: Btn, skip: bool) -> (bool, bool) {
        let next_rect = Rect::new(x + w - NEXT_W, y, NEXT_W, BUTTON_H);
        let skipped = skip && {
            let w = skip_width(cx);
            cx.button(Rect::new(next_rect.x - GAP - w, y, w, BUTTON_H), cx.t("welcome.skip"), Btn::GHOST.small())
        };
        (cx.button(next_rect, next, next_btn), skipped)
    }

    /// Step one: the language, and the Minecraft jar the assets are extracted from.
    #[allow(clippy::too_many_arguments)]
    fn welcome_client(
        &mut self,
        cx: &mut Cx,
        welcome: &mut Welcome,
        x: f32,
        mut y: f32,
        w: f32,
        status: &Status,
        out: &mut Vec<Action>,
    ) -> (f32, bool) {
        y += cx.paragraph(cx.t("welcome.sub"), x, y, w, theme::DIM) + 10.0;

        y = cx.section(cx.t("settings.language"), x, y, w);
        let current = LANGUAGES.iter().position(|l| *l == cx.tr.language());
        let label = current.map_or("", |i| LANGUAGES[i].name());
        let picked = cx.dropdown("welcome-language", Rect::new(x, y, w, 16.0), label, true, move || {
            (LANGUAGES.iter().map(|l| l.name().to_owned()).collect(), current)
        });
        if let Some(index) = picked.filter(|i| Some(*i) != current) {
            self.settings.language = Some(LANGUAGES[index].code().to_owned());
            self.settings.save(&self.home);
            out.push(Action::LanguageChanged);
        }
        y += 16.0 + 12.0;

        y = cx.section(cx.t("welcome.client"), x, y, w);
        let version = melon_base::home::VERSION;
        if self.disk.assets_ready {
            cx.label(&cx.tf("assets.ready", &[version]), x, y + 1.0, w, theme::ACCENT);
            y += 12.0 + 14.0;
            let label = cx.t("welcome.continue");
            let (next, _) = Self::welcome_buttons(cx, x, y, w, label, Btn::PRIMARY, false);
            if next {
                self.to_account_step(welcome);
            }
            return (y + BUTTON_H, false);
        }
        y += cx.paragraph(&cx.tf("welcome.client_note", &[version]), x, y, w, theme::DIM) + 6.0;
        let missing = cx.tf("welcome.no_jar", &[version, version]);
        y = self.settings_menu.jar.draw(cx, x, y, w, &missing);
        let half = ((w - GAP) / 2.0).floor();
        let free = !status.busy;
        if cx.button(Rect::new(x, y, half, BUTTON_H), cx.t("welcome.search"), Btn::NORMAL.enabled(free)) {
            self.settings_menu.jar = JarSource::scan(&self.home);
        }
        if cx.button(Rect::new(x + half + GAP, y, half, BUTTON_H), cx.t("assets.choose"), Btn::NORMAL.enabled(free))
            && let Some(jar) = melon_base::platform::pick_file(cx.t("assets.choose"), "jar")
        {
            self.settings_menu.jar.set(Some(prepare::from_jar(jar)));
        }
        y += BUTTON_H + 16.0;

        let jar = &self.settings_menu.jar;
        let can_extract = free && jar.found.is_some() && !jar.rejected();
        let next_btn = Btn::PRIMARY.enabled(can_extract);
        let label = cx.t("welcome.extract");
        let (next, skipped) = Self::welcome_buttons(cx, x, y, w, label, next_btn, true);
        if next {
            let install = Install {
                assets: true,
                version: None,
            };
            out.push(Action::Install(install, jar.found.clone()));
            welcome.extracting = true;
        }
        if skipped {
            self.to_account_step(welcome);
        }
        (y + BUTTON_H, false)
    }

    /// Step two: a Microsoft account or an offline profile.
    #[allow(clippy::too_many_arguments)]
    fn welcome_account(
        &mut self,
        cx: &mut Cx,
        welcome: &mut Welcome,
        x: f32,
        mut y: f32,
        w: f32,
        status: &Status,
        out: &mut Vec<Action>,
    ) -> (f32, bool) {
        y += cx.paragraph(cx.t("welcome.account_note"), x, y, w, theme::DIM) + 10.0;
        let has_account = !self.accounts.list().is_empty();
        if let Some(account) = self.selected_account().cloned() {
            let face = Rect::new(x, y, 18.0, 18.0);
            match status.heads.get(&account.key()) {
                Some(head) => cx.image(head, face),
                None => cx.initial(face, &account.key(), account.username()),
            }
            cx.label(account.username(), face.right() + 6.0, y + 1.0, w - 24.0, theme::TEXT);
            let (key, color) = self.account_status(&account, status);
            cx.small_label(cx.t(key), face.right() + 6.0, y + 11.0, w - 24.0, color);
            y += 18.0 + 10.0;
        }
        let half = ((w - GAP) / 2.0).floor();
        let free = !status.busy;
        let microsoft = if has_account { Btn::NORMAL } else { Btn::PRIMARY };
        if cx.button(Rect::new(x, y, half, BUTTON_H), cx.t("welcome.microsoft"), microsoft.enabled(free)) {
            out.push(Action::SignIn);
        }
        if cx.button(Rect::new(x + half + GAP, y, half, BUTTON_H), cx.t("welcome.offline"), Btn::NORMAL.enabled(free)) {
            self.open_prompt(cx, Purpose::AddOffline, "");
        }
        y += BUTTON_H + 16.0;

        if cx.button(Rect::new(x, y, 60.0, BUTTON_H), cx.t("welcome.back"), Btn::GHOST.small().enabled(free)) {
            welcome.step = Step::Client;
        }
        let (next, skipped) = match has_account {
            true => {
                let label = cx.t("welcome.continue");
                Self::welcome_buttons(cx, x, y, w, label, Btn::PRIMARY.enabled(free), false)
            }
            false => {
                let skip_w = skip_width(cx);
                let skip = Rect::new(x + w - skip_w, y, skip_w, BUTTON_H);
                (false, cx.button(skip, cx.t("welcome.skip"), Btn::GHOST.small().enabled(free)))
            }
        };
        (y + BUTTON_H, next || skipped)
    }
}
