//! The account menu: the accounts on the left, the selected one on the right.

use super::kit::theme;
use super::kit::{BUTTON_H, Btn, Cx, GAP};
use super::shell::{Action, Purpose, Session, Shell, Status};
use melon_base::auth::accounts::Account;
use super::kit::Rect;
use std::time::{Duration, Instant};

const ROW_H: f32 = 30.0;
const LIST_W: f32 = 190.0;
const COPIED: Duration = Duration::from_millis(1500);

/// The account's UUID in the usual 8-4-4-4-12 form.
fn uuid_of(account: &Account) -> String {
    let raw = account.uuid();
    uuid::Uuid::parse_str(&raw).map_or(raw, |uuid| uuid.to_string())
}

impl Shell {
    pub(super) fn account_menu(&mut self, cx: &mut Cx, sheet: Rect, status: &Status, out: &mut Vec<Action>) {
        let body = self.sheet(cx, sheet, cx.t("account.title"));
        let list = Rect::new(body.x + 8.0, body.y + 8.0, LIST_W.min(body.w * 0.45), body.h - 16.0);
        self.account_list(cx, list, status, out);
        let detail_x = list.right() + 12.0;
        let detail = Rect::new(detail_x, body.y + 8.0, body.right() - 8.0 - detail_x, body.h - 16.0);
        match self.selected_account().cloned() {
            Some(account) => self.account_detail(cx, detail, &account, status, out),
            None => {
                cx.paragraph(cx.t("account.empty"), detail.x, detail.y + 6.0, detail.w, theme::DIM);
            }
        }
    }

    fn account_list(&mut self, cx: &mut Cx, area: Rect, status: &Status, out: &mut Vec<Action>) {
        let half = ((area.w - GAP) / 2.0).floor();
        let buttons_y = area.bottom() - 2.0 * BUTTON_H - 4.0;
        let rows = Rect::new(area.x, area.y, area.w, area.h - 2.0 * BUTTON_H - 4.0 - GAP);
        cx.frame(rows, theme::FIELD, theme::EDGE);
        let inner = super::kit::inset(rows, 1.0);
        let accounts: Vec<Account> = self.accounts.list().to_vec();
        let selected = self.accounts.selected_index();
        let content_h = accounts.len() as f32 * ROW_H;
        let mut scroll = std::mem::take(&mut self.account_scroll);
        let top = scroll.begin(cx, inner, content_h);
        for (i, account) in accounts.iter().enumerate() {
            let row = Rect::new(inner.x, top + i as f32 * ROW_H, inner.w - 3.0, ROW_H);
            let hot = cx.hot(row);
            if selected == Some(i) {
                cx.fill(row, theme::ACCENT_SOFT);
                cx.fill(Rect::new(row.x, row.y, 2.0, row.h), theme::ACCENT);
            } else if hot {
                cx.fill(row, theme::HOVER);
                cx.pointer();
            }
            let face = Rect::new(row.x + 6.0, row.y + 4.0, 22.0, 22.0);
            match status.heads.get(&account.key()) {
                Some(head) => cx.image(head, face),
                None => cx.initial(face, &account.key(), account.username()),
            }
            let text_x = face.right() + 6.0;
            let text_w = row.right() - text_x - 4.0;
            cx.label(account.username(), text_x, row.y + 6.0, text_w, theme::TEXT);
            let (text, color) = self.account_status(cx, account, status);
            cx.small_label(&text, text_x, row.y + 17.0, text_w, color);
            if cx.clicked(row) && selected != Some(i) {
                if status.busy {
                    self.notify(cx.t("play.blocked.busy"));
                } else {
                    self.accounts.select(i);
                    self.save_accounts();
                }
            }
        }
        scroll.end(cx, inner, content_h);
        self.account_scroll = scroll;
        let microsoft = Rect::new(area.x, buttons_y, half, BUTTON_H);
        if cx.button(microsoft, cx.t("account.add_microsoft"), Btn::PRIMARY.enabled(!status.busy)) {
            out.push(Action::SignIn);
        }
        let offline = Rect::new(area.x + half + GAP, buttons_y, half, BUTTON_H);
        if cx.button(offline, cx.t("account.add_offline"), Btn::NORMAL.enabled(!status.busy)) {
            self.open_prompt(cx, Purpose::AddOffline, "");
        }
        let other = Rect::new(area.x, buttons_y + BUTTON_H + 4.0, area.w, BUTTON_H);
        if cx.button(other, cx.t("account.add_other"), Btn::NORMAL.enabled(!status.busy)) {
            self.open_provider_dialog(cx, None);
        }
        cx.tooltip(other, cx.t("account.add_other_tip"));
    }

    fn account_detail(&mut self, cx: &mut Cx, area: Rect, account: &Account, status: &Status, out: &mut Vec<Action>) {
        // TODO: 3D player model: needs the client's entity renderer in the launcher.
        let model = Rect::new(area.right() - 92.0, area.y, 92.0, 132.0);
        cx.frame(model, theme::FIELD, theme::EDGE);
        let face = Rect::new(model.x + 22.0, model.y + 18.0, 48.0, 48.0);
        match status.heads.get(&account.key()) {
            Some(head) => cx.image(head, face),
            None => cx.initial(face, &account.key(), account.username()),
        }
        cx.paragraph(cx.t("account.model_soon"), model.x + 8.0, model.y + 78.0, model.w - 16.0, theme::FAINT);
        // TODO: skin changer: upload through the Minecraft services API.
        let skin = Rect::new(model.x, model.bottom() + GAP, model.w, BUTTON_H);
        cx.button(skin, cx.t("account.change_skin"), Btn::NORMAL.enabled(false));
        cx.tooltip(skin, cx.t("common.soon"));
        // TODO: cape changer: list and activate the account's capes.
        let cape = Rect::new(model.x, skin.bottom() + 4.0, model.w, BUTTON_H);
        cx.button(cape, cx.t("account.change_cape"), Btn::NORMAL.enabled(false));
        cx.tooltip(cape, cx.t("common.soon"));

        let w = model.x - 12.0 - area.x;
        let x = area.x;
        let mut y = area.y + 2.0;
        cx.label(account.username(), x, y, w, theme::TEXT);
        y += 12.0;
        let (text, color) = self.account_status(cx, account, status);
        cx.small_label(&text, x, y, w, color);
        y += 16.0;

        y = cx.section(cx.t("account.uuid"), x, y, w);
        let uuid = uuid_of(account);
        let line = Rect::new(x, y - 1.0, w, 11.0);
        let copied = self.copied_at.is_some_and(|t| t.elapsed() < COPIED);
        let hot = cx.hot(line);
        if hot {
            cx.pointer();
        }
        let (text, color) = match (copied, hot) {
            (true, _) => (cx.t("common.copied").to_owned(), theme::ACCENT),
            (false, true) => (uuid.clone(), theme::TEXT),
            (false, false) => (uuid.clone(), theme::DIM),
        };
        cx.small_label(&text, x, y, w, color);
        cx.tooltip(line, cx.t("account.copy_uuid"));
        if cx.clicked(line) {
            melon_base::platform::copy_to_clipboard(&uuid);
            self.copied_at = Some(Instant::now());
        }
        y += 16.0;

        y = cx.section(cx.t("panel.stats"), x, y, w);
        let meta = self.accounts.meta(account);
        let (played, last) = if meta.last_played == 0 {
            (cx.t("stats.never").to_owned(), cx.t("stats.never").to_owned())
        } else {
            (
                cx.tr.span(meta.play_secs),
                cx.tr.ago(crate::store::now_secs().saturating_sub(meta.last_played)),
            )
        };
        for (i, (key, value)) in [("stats.playtime", played), ("stats.last_played", last)].iter().enumerate() {
            let line_y = y + i as f32 * 12.0;
            let label_w = cx.small(cx.t(key), x, line_y, theme::DIM);
            cx.small_label(value, x + label_w + 6.0, line_y, w - label_w - 6.0, theme::TEXT);
        }
        y += 24.0 + 8.0;

        y = cx.section(cx.t("panel.manage"), x, y, w);
        let third = ((w - 2.0 * GAP) / 3.0).floor().min(90.0);
        let rename = Rect::new(x, y, third, BUTTON_H);
        let can_rename = !status.busy && !account.is_online();
        if cx.button(rename, cx.t("manage.rename"), Btn::NORMAL.enabled(can_rename)) {
            self.open_prompt(cx, Purpose::RenameOffline, account.username());
        }
        match account {
            Account::Microsoft(_) => cx.tooltip(rename, cx.t("account.rename_microsoft")),
            Account::Alternative(stored) => cx.tooltip(rename, &cx.tf("account.rename_service", &[&stored.provider.name])),
            Account::Offline { .. } => {}
        }
        let index = self.accounts.selected_index();
        let lock = Rect::new(x + third + GAP, y, third, BUTTON_H);
        let lock_key = if meta.locked { "account.unlock" } else { "account.lock" };
        if cx.button(lock, cx.t(lock_key), Btn::NORMAL)
            && let Some(index) = index
        {
            self.accounts.set_locked(index, !meta.locked);
            self.save_accounts();
        }
        cx.tooltip(lock, cx.t("account.lock_tip"));
        let what = format!("remove:{}", account.key());
        let remove_key = if self.confirming(&what) { "manage.delete_sure" } else { "account.remove" };
        let remove = Rect::new(x + 2.0 * (third + GAP), y, third, BUTTON_H);
        if cx.button(remove, cx.t(remove_key), Btn::DANGER.enabled(!status.busy))
            && self.confirmed(&what)
            && let Some(index) = index
        {
            self.accounts.remove(index);
            self.save_accounts();
        }
        y += BUTTON_H + GAP;
        if self.session_of(account, status) == Some(Session::Expired) {
            let again = Rect::new(x, y, (third * 2.0 + GAP).min(w), BUTTON_H);
            if cx.button(again, cx.t("account.sign_in_again"), Btn::PRIMARY.enabled(!status.busy)) {
                match account {
                    Account::Alternative(stored) => self.open_provider_dialog(cx, Some(stored)),
                    _ => out.push(Action::SignIn),
                }
            }
        }
    }
}
