//! « Ne pas déranger » avec une durée : 30 min, 1 h, 2 h, jusqu'à demain
//! matin, ou une date et une heure choisies (comme le client web).

use chrono::{Datelike as _, Duration, Local, TimeZone as _, Timelike as _};
use gpui_kit::component::input::InputState;

use super::dialogs::Dialog;
use super::*;
use crate::backend::Command;

/// Durées proposées dans le menu du compte.
pub(crate) const DND_CHOICES: [&str; 5] = ["dont_clear", "thirty_minutes", "one_hour", "two_hours", "tomorrow"];

/// Fin de « Ne pas déranger » (secondes Unix) pour un choix du menu ;
/// 0 = sans échéance. « Demain » : demain à 9 h, comme le client web.
pub(crate) fn dnd_end(choice: &str, now: chrono::DateTime<Local>) -> i64 {
    let at = match choice {
        "thirty_minutes" => Some(now + Duration::minutes(30)),
        "one_hour" => Some(now + Duration::hours(1)),
        "two_hours" => Some(now + Duration::hours(2)),
        "tomorrow" => (now.date_naive() + Duration::days(1))
            .and_hms_opt(9, 0, 0)
            .and_then(|t| Local.from_local_datetime(&t).earliest()),
        _ => None,
    };
    at.map(|t| t.timestamp()).unwrap_or(0)
}

/// Libellé d'un choix du menu.
pub(crate) fn dnd_choice_label(choice: &str) -> SharedString {
    match choice {
        "thirty_minutes" => t!("dnd.thirty_minutes"),
        "one_hour" => t!("dnd.one_hour"),
        "two_hours" => t!("dnd.two_hours"),
        "tomorrow" => t!("dnd.tomorrow"),
        _ => t!("dnd.dont_clear"),
    }
    .into()
}

/// « jusqu'à 14:30 », « jusqu'à demain 09:00 », « jusqu'au 24/11 09:00 ».
pub(crate) fn dnd_until(end: i64, now: chrono::DateTime<Local>) -> Option<String> {
    let at = Local.timestamp_opt(end, 0).single()?;
    if end <= now.timestamp() {
        return None;
    }
    let time = at.format(super::settings::time_format()).to_string();
    let days = (at.date_naive() - now.date_naive()).num_days();
    Some(match days {
        0 => t!("dnd.until_today", time = time).to_string(),
        1 => t!("dnd.until_tomorrow", time = time).to_string(),
        _ => t!("dnd.until_date", date = at.format("%d/%m").to_string(), time = time).to_string(),
    })
}

/// Fin courte, sous son nom : « 14:30 », « demain 09:00 », « 24/11 09:00 ».
fn dnd_until_short(end: i64) -> String {
    let Some(at) = Local.timestamp_opt(end, 0).single() else { return String::new() };
    let time = at.format(super::settings::time_format()).to_string();
    match (at.date_naive() - Local::now().date_naive()).num_days() {
        0 => time,
        1 => t!("dnd.short_tomorrow", time = time).to_string(),
        _ => format!("{} {time}", at.format("%d/%m")),
    }
}

/// Heure saisie : « 14:30 », « 14h30 », « 14 h », « 9 ».
fn parse_time(text: &str) -> Option<(u32, u32)> {
    let text = text.trim().to_lowercase().replace(' ', "");
    let (h, m) = match text.split_once([':', 'h']) {
        Some((h, m)) => (h, if m.is_empty() { "0" } else { m }),
        None => (text.as_str(), "0"),
    };
    let (h, m) = (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?);
    (h < 24 && m < 60).then_some((h, m))
}

impl ChatApp {
    /// Change son statut ; `dnd_end_time` : fin de « Ne pas déranger ».
    pub(crate) fn set_status_until(&mut self, status: &str, dnd_end_time: i64, cx: &mut Context<Self>) {
        let me = self.me_id().to_string();
        self.statuses.insert(me, status.to_string());
        self.dnd_end = if status == "dnd" { dnd_end_time } else { 0 };
        self.send(Command::SetStatus { status: status.to_string(), dnd_end_time });
        cx.notify();
    }

    /// Statut affiché sous son nom, avec la fin de « Ne pas déranger ».
    pub(crate) fn my_status_line(&self) -> SharedString {
        let label = self.status_label(self.me_id());
        if self.status_of(self.me_id()) == "dnd" && dnd_until(self.dnd_end, Local::now()).is_some() {
            return format!("{label} · {}", dnd_until_short(self.dnd_end)).into();
        }
        label
    }

    pub(crate) fn open_dnd_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Heure proposée : l'heure pleine suivante, dans au moins 30 minutes.
        let next = Local::now() + Duration::minutes(90);
        let suggested = format!("{:02}:00", next.hour());
        let day = if next.date_naive() > Local::now().date_naive() { 1 } else { 0 };
        self.dialog = Some(Dialog::DndCustom { day });
        self.dialog_input.update(cx, |s: &mut InputState, cx| {
            s.set_placeholder(t!("dnd.time_placeholder"), window, cx);
            s.set_value(suggested, window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    fn custom_dnd_end(&self, day: i64, cx: &App) -> Option<i64> {
        let (h, m) = parse_time(self.dialog_input.read(cx).value().as_ref())?;
        let date = Local::now().date_naive() + Duration::days(day);
        let at = Local.from_local_datetime(&date.and_hms_opt(h, m, 0)?).earliest()?;
        (at > Local::now()).then(|| at.timestamp())
    }

    pub(crate) fn save_dnd_custom(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::DndCustom { day }) = self.dialog else { return };
        match self.custom_dnd_end(day, cx) {
            Some(end) => {
                self.dialog = None;
                self.set_status_until("dnd", end, cx);
            }
            None => self.flash(t!("dnd.invalid").to_string(), cx),
        }
    }

    pub(crate) fn render_dnd_dialog(&self, day: i64, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let today = Local::now().date_naive();
        let day_name = |offset: i64| -> SharedString {
            match offset {
                0 => t!("dnd.today").into(),
                1 => t!("dnd.tomorrow_short").into(),
                _ => {
                    let date = today + Duration::days(offset);
                    format!("{} {}", weekday_short(date.weekday()), date.format("%d/%m")).into()
                }
            }
        };
        let chips = h_flex().flex_wrap().gap_1p5().children((0..7).map(|offset| {
            let active = day == offset;
            div()
                .id(SharedString::from(format!("dnd-day-{offset}")))
                .px_2p5()
                .py_1()
                .rounded_full()
                .border_1()
                .cursor_pointer()
                .secondary_size()
                .role(gpui_kit::Role::RadioButton)
                .aria_selected(active)
                .when(active, |el| el.bg(p.button_bg).border_color(p.button_bg).text_color(p.button_color))
                .when(!active, |el| el.border_color(p.border()).hover(|s| s.bg(p.hover())))
                .child(day_name(offset))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(Dialog::DndCustom { day }) = this.dialog.as_mut() {
                        *day = offset;
                    }
                    cx.notify();
                }))
        }));
        let preview = self
            .custom_dnd_end(day, cx)
            .and_then(|end| dnd_until(end, Local::now()))
            .map(|until| t!("dnd.preview", until = until).to_string())
            .unwrap_or_else(|| t!("dnd.invalid").to_string());
        let label = |text: SharedString| div().caption_size().font_semibold().text_color(p.muted()).child(text);
        let content = v_flex()
            .px_4()
            .pb_4()
            .gap_2()
            .capture_action(cx.listener(|this, _: &Enter, _, cx| {
                this.save_dnd_custom(cx);
                cx.stop_propagation();
            }))
            .child(label(t!("dnd.day").into()))
            .child(chips)
            .child(div().h(px(4.)))
            .child(label(t!("dnd.time").into()))
            .child(div().w(px(140.)).child(
                Input::new(&self.dialog_input).prefix(Icon::new(IconName::Clock).size(px(14.)).text_color(p.muted())),
            ))
            .child(div().secondary_size().text_color(p.muted()).child(preview))
            .into_any_element();
        let footer = h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(div().flex_1())
            .child(
                Button::new("dnd-cancel")
                    .label(t!("common.cancel"))
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.close_dialog(cx))),
            )
            .child(
                Button::new("dnd-save")
                    .label(t!("dnd.enable"))
                    .small()
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| this.save_dnd_custom(cx))),
            )
            .into_any_element();
        self.dialog_shell(IconName::CircleMinus, t!("dnd.custom_title").into(), 460., content, Some(footer), cx)
    }
}

fn weekday_short(day: chrono::Weekday) -> SharedString {
    match day {
        chrono::Weekday::Mon => t!("dnd.weekday.mon"),
        chrono::Weekday::Tue => t!("dnd.weekday.tue"),
        chrono::Weekday::Wed => t!("dnd.weekday.wed"),
        chrono::Weekday::Thu => t!("dnd.weekday.thu"),
        chrono::Weekday::Fri => t!("dnd.weekday.fri"),
        chrono::Weekday::Sat => t!("dnd.weekday.sat"),
        chrono::Weekday::Sun => t!("dnd.weekday.sun"),
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::{dnd_end, dnd_until, parse_time};
    use chrono::{Datelike as _, Local, TimeZone as _, Timelike as _};

    #[test]
    fn echeances_ne_pas_deranger() {
        let now = Local.with_ymd_and_hms(2026, 11, 22, 16, 10, 0).unwrap();
        assert_eq!(dnd_end("dont_clear", now), 0);
        assert_eq!(dnd_end("one_hour", now), now.timestamp() + 3600);
        let tomorrow = Local.timestamp_opt(dnd_end("tomorrow", now), 0).unwrap();
        assert_eq!((tomorrow.day(), tomorrow.hour(), tomorrow.minute()), (23, 9, 0));
        assert_eq!(parse_time("14h30"), Some((14, 30)));
        assert_eq!(parse_time(" 9 "), Some((9, 0)));
        assert_eq!(parse_time("14 h"), Some((14, 0)));
        assert_eq!(parse_time("25:00"), None);
        assert!(dnd_until(now.timestamp() - 1, now).is_none());
    }
}
