//! Fenêtres : parcourir les canaux, nouveau message direct ou de groupe,
//! statut personnalisé, préférences de notification d'un canal ; et la vue
//! « Fils » de toute l'équipe (entrée de la barre latérale, onglets).

use std::collections::HashSet;

use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::InputState;
use gpui_kit::component::switch::Switch;

use super::*;
use crate::api::{ChannelMember, CustomStatus, UserThread};
use crate::backend::Command;
use crate::sidebar::ChannelState;

/// Personnes au plus dans un message de groupe (soi non compris).
pub(crate) const GROUP_MAX: usize = 7;

/// Fenêtre ouverte (une seule à la fois).
pub(crate) enum Dialog {
    Browse(Browse),
    Direct(DirectPicker),
    Status(StatusDraft),
    Notify(NotifyDraft),
    Forward(super::messaging::ForwardDraft),
    /// @channel / @all / @here dans un canal de `count` personnes.
    ConfirmNotify {
        pane: Pane,
        count: i64,
        mention: &'static str,
    },
}

/// « Parcourir les canaux » : canaux publics et archivés de l'équipe.
pub(crate) struct Browse {
    pub archived_tab: bool,
    pub hide_joined: bool,
    /// `None` pendant le chargement.
    pub public: Option<Vec<Channel>>,
    pub archived: Option<Vec<Channel>>,
    /// Canaux en cours d'adhésion.
    pub joining: HashSet<String>,
}

/// Nouveau message direct ou de groupe.
pub(crate) struct DirectPicker {
    /// Personnes proposées : récentes (recherche vide) ou trouvées ;
    /// `None` pendant la recherche.
    pub results: Option<Vec<String>>,
    pub selected: Vec<String>,
    /// Ligne choisie au clavier.
    pub highlight: usize,
}

/// Statut personnalisé en cours de saisie (le texte est dans `dialog_input`).
pub(crate) struct StatusDraft {
    pub emoji: String,
    pub duration: &'static str,
}

/// Préférences de notification d'un canal en cours de modification.
pub(crate) struct NotifyDraft {
    pub channel_id: String,
    /// "default", "all", "mention" ou "none".
    pub desktop: String,
    pub ignore_mentions: bool,
    pub auto_follow: bool,
    pub muted: bool,
}

/// Échéances proposées pour un statut personnalisé.
const DURATIONS: [&str; 6] = ["dont_clear", "thirty_minutes", "one_hour", "four_hours", "today", "this_week"];

/// Statuts suggérés : (emoji, clé du texte, échéance), comme le client web.
const PRESETS: [(&str, &str, &str); 5] = [
    ("calendar", "meeting", "one_hour"),
    ("hamburger", "lunch", "thirty_minutes"),
    ("sneezing_face", "sick", "today"),
    ("house", "wfh", "today"),
    ("palm_tree", "vacation", "this_week"),
];

fn duration_label(duration: &str) -> SharedString {
    match duration {
        "thirty_minutes" => t!("custom_status.duration.thirty_minutes"),
        "one_hour" => t!("custom_status.duration.one_hour"),
        "four_hours" => t!("custom_status.duration.four_hours"),
        "today" => t!("custom_status.duration.today"),
        "this_week" => t!("custom_status.duration.this_week"),
        "date_and_time" => t!("custom_status.duration.date_and_time"),
        _ => t!("custom_status.duration.dont_clear"),
    }
    .into()
}

fn preset_text(key: &str) -> String {
    match key {
        "meeting" => t!("custom_status.preset.meeting"),
        "lunch" => t!("custom_status.preset.lunch"),
        "sick" => t!("custom_status.preset.sick"),
        "wfh" => t!("custom_status.preset.wfh"),
        _ => t!("custom_status.preset.vacation"),
    }
    .to_string()
}

/// Échéance d'un statut (RFC 3339, UTC), vide pour « ne pas effacer ».
/// « Aujourd'hui » finit à minuit, « cette semaine » le dimanche soir.
pub(crate) fn expires_at(duration: &str, now: chrono::DateTime<chrono::Local>) -> String {
    use chrono::{Datelike as _, Duration, TimeZone as _};
    let end_of = |date: chrono::NaiveDate| {
        date.and_hms_opt(23, 59, 59).and_then(|t| chrono::Local.from_local_datetime(&t).earliest())
    };
    let at = match duration {
        "thirty_minutes" => Some(now + Duration::minutes(30)),
        "one_hour" => Some(now + Duration::hours(1)),
        "four_hours" => Some(now + Duration::hours(4)),
        "today" => end_of(now.date_naive()),
        "this_week" => {
            let to_sunday = 6 - i64::from(now.weekday().num_days_from_monday());
            end_of(now.date_naive() + Duration::days(to_sunday))
        }
        _ => None,
    };
    at.map(|t| t.with_timezone(&chrono::Utc).to_rfc3339()).unwrap_or_default()
}

/// Échéance connue, en `&'static str` (sinon « aujourd'hui »).
fn known_duration(duration: &str) -> &'static str {
    DURATIONS.iter().find(|d| **d == duration).copied().unwrap_or("today")
}

impl ChatApp {
    // ------------------------------------------------------------------
    // Commun
    // ------------------------------------------------------------------

    pub(crate) fn close_dialog(&mut self, cx: &mut Context<Self>) {
        if self.dialog.take().is_some() {
            cx.notify();
        }
    }

    fn reset_dialog_input(
        &mut self,
        placeholder: SharedString,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dialog_input.update(cx, |s: &mut InputState, cx| {
            s.set_placeholder(placeholder, window, cx);
            s.set_value(value, window, cx);
            s.focus(window, cx);
        });
    }

    fn dialog_term(&self, cx: &App) -> String {
        self.dialog_input.read(cx).value().trim().to_string()
    }

    /// Le texte du champ a changé : nouvelle recherche de personnes.
    pub(crate) fn dialog_input_changed(&mut self, cx: &mut Context<Self>) {
        let term = self.dialog_term(cx);
        if let Some(Dialog::Forward(f)) = self.dialog.as_mut() {
            f.highlight = 0;
        }
        if matches!(self.dialog, Some(Dialog::Direct(_))) {
            let recents = term.is_empty().then(|| self.recent_partners());
            if let Some(Dialog::Direct(d)) = self.dialog.as_mut() {
                d.highlight = 0;
                d.results = recents.clone();
            }
            if recents.is_none() {
                self.send(Command::SearchUsers { term });
            }
        }
        cx.notify();
    }

    /// Fenêtre posée au-dessus de l'interface : en-tête, contenu, pied.
    pub(crate) fn dialog_shell(
        &self,
        icon: IconName,
        title: SharedString,
        width: f32,
        body: AnyElement,
        footer: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let width = width.min(self.viewport_w - 24.).max(280.);
        div()
            .id("dialog-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(black().opacity(0.35))
            .flex()
            .justify_center()
            .items_start()
            .pt(px(90.))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close_dialog(cx)))
            .child(
                v_flex()
                    .id("dialog")
                    .occlude()
                    .w(px(width))
                    .rounded(px(12.))
                    .overflow_hidden()
                    .border_1()
                    .border_color(p.border())
                    .bg(p.center_bg)
                    .text_color(p.center_color)
                    .shadow_xl()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .capture_action(cx.listener(|this, _: &Escape, _, cx| {
                        this.close_dialog(cx);
                        cx.stop_propagation();
                    }))
                    .child(
                        h_flex()
                            .px_4()
                            .pt_4()
                            .pb_3()
                            .gap_2()
                            .child(Icon::new(icon).size(px(18.)).text_color(p.button_bg))
                            .child(div().flex_1().min_w_0().truncate().text_size(px(16.)).font_bold().child(title))
                            .child(
                                Button::new("dialog-close")
                                    .icon(Icon::new(IconName::X))
                                    .ghost()
                                    .small()
                                    .on_click(cx.listener(|this, _, _, cx| this.close_dialog(cx))),
                            ),
                    )
                    .child(body)
                    .children(footer.map(|f| {
                        h_flex().px_4().py_3().gap_2().items_center().border_t_1().border_color(p.border()).child(f)
                    })),
            )
            .into_any_element()
    }

    pub(crate) fn render_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        Some(match self.dialog.as_ref()? {
            Dialog::Browse(b) => self.render_browse(b, cx),
            Dialog::Direct(d) => self.render_direct(d, cx),
            Dialog::Status(s) => self.render_status_dialog(s, cx),
            Dialog::Notify(n) => self.render_notify(n, cx),
            Dialog::Forward(f) => self.render_forward(f, cx),
            Dialog::ConfirmNotify { pane, count, mention } => self.render_confirm_notify(*pane, *count, mention, cx),
        })
    }

    /// Emoji par nom Mattermost (ou emoji personnalisé du serveur).
    fn named_emoji(&self, name: &str, size: f32) -> AnyElement {
        match self.custom_emojis.get(name) {
            Some(image) => img(image.clone()).size(px(size)).into_any_element(),
            None => emoji_face(&emoji::lookup(name), size),
        }
    }

    /// Onglet souligné, comme ceux du panneau des fichiers.
    fn dialog_tab(
        &self,
        id: &'static str,
        label: SharedString,
        count: Option<usize>,
        active: bool,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = self.palette;
        h_flex()
            .id(id)
            .gap_1p5()
            .px_3()
            .py_2()
            .cursor_pointer()
            .border_b_2()
            .border_color(if active { p.button_bg } else { transparent_black() })
            .text_sm()
            .when(active, |el| el.font_semibold())
            .when(!active, |el| el.text_color(p.muted()).hover(|s| s.text_color(p.center_color)))
            .child(label)
            .children(
                count.map(|n| {
                    div().px_1p5().rounded_full().text_xs().bg(p.center_color.opacity(0.08)).child(n.to_string())
                }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                on_click(this, cx);
                cx.notify();
            }))
    }

    // ------------------------------------------------------------------
    // Parcourir les canaux
    // ------------------------------------------------------------------

    pub(crate) fn open_browse_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(team_id) = self.team_id.clone() else { return };
        self.drawer = None;
        self.dialog = Some(Dialog::Browse(Browse {
            archived_tab: false,
            hide_joined: false,
            public: None,
            archived: None,
            joining: HashSet::new(),
        }));
        self.reset_dialog_input(t!("browse.search_placeholder").into(), String::new(), window, cx);
        self.send(Command::LoadBrowseChannels { team_id: team_id.clone(), archived: false });
        self.send(Command::LoadBrowseChannels { team_id, archived: true });
        cx.notify();
    }

    pub(crate) fn on_browse_channels(&mut self, team_id: String, archived: bool, mut channels: Vec<Channel>) {
        if self.team_id.as_deref() != Some(team_id.as_str()) {
            return;
        }
        if let Some(Dialog::Browse(b)) = self.dialog.as_mut() {
            channels.retain(|c| archived || (c.kind == "O" && c.delete_at == 0));
            channels.sort_by_cached_key(|c| crate::fuzzy::fold(&c.display_name));
            if archived {
                b.archived = Some(channels);
            } else {
                b.public = Some(channels);
            }
        }
    }

    pub(crate) fn is_archived(&self, channel_id: &str) -> bool {
        self.channels.get(channel_id).is_some_and(|c| c.channel.delete_at > 0)
    }

    /// Clic sur un canal : l'ouvrir si on en est membre (ou s'il est
    /// archivé, en lecture seule), sinon le rejoindre.
    pub(crate) fn browse_open(&mut self, channel: Channel, window: &mut Window, cx: &mut Context<Self>) {
        let id = channel.id.clone();
        if self.channels.contains_key(&id) || channel.delete_at > 0 {
            self.channels.entry(id.clone()).or_insert(ChannelState {
                channel,
                unread_messages: false,
                mentions: 0,
                muted: false,
                notify: Default::default(),
            });
            self.dialog = None;
            self.select_channel(id, cx);
            self.focus_composer(window, cx);
            return;
        }
        if let Some(Dialog::Browse(b)) = self.dialog.as_mut()
            && b.joining.insert(id.clone())
        {
            self.send(Command::JoinChannel { channel_id: id });
        }
        cx.notify();
    }

    fn render_browse(&self, b: &Browse, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let needle = crate::fuzzy::fold(&self.dialog_term(cx));
        let source = if b.archived_tab { &b.archived } else { &b.public };
        let matches = |c: &&Channel| {
            needle.is_empty()
                || crate::fuzzy::fold(&format!("{} {} {}", c.display_name, c.name, c.purpose)).contains(&needle)
        };
        let count = |list: &Option<Vec<Channel>>| list.as_ref().map(|l| l.iter().filter(matches).count());

        let tabs = h_flex()
            .pl_3()
            .pr_4()
            .gap_1()
            .border_b_1()
            .border_color(p.border())
            .child(self.dialog_tab(
                "browse-public",
                t!("browse.tab_public").into(),
                count(&b.public),
                !b.archived_tab,
                |this, _| {
                    if let Some(Dialog::Browse(b)) = this.dialog.as_mut() {
                        b.archived_tab = false;
                    }
                },
                cx,
            ))
            .child(self.dialog_tab(
                "browse-archived",
                t!("browse.tab_archived").into(),
                count(&b.archived),
                b.archived_tab,
                |this, _| {
                    if let Some(Dialog::Browse(b)) = this.dialog.as_mut() {
                        b.archived_tab = true;
                    }
                },
                cx,
            ))
            .child(div().flex_1())
            .when(!b.archived_tab, |el| {
                el.child(
                    Checkbox::new("browse-hide-joined")
                        .label(SharedString::from(t!("browse.hide_joined")))
                        .checked(b.hide_joined)
                        .small()
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            if let Some(Dialog::Browse(b)) = this.dialog.as_mut() {
                                b.hide_joined = *checked;
                            }
                            cx.notify();
                        })),
                )
            });

        let body: AnyElement = match source {
            None => div().p_4().text_color(p.muted()).child(t!("common.loading")).into_any_element(),
            Some(list) => {
                let rows: Vec<&Channel> = list
                    .iter()
                    .filter(matches)
                    .filter(|c| b.archived_tab || !b.hide_joined || !self.channels.contains_key(&c.id))
                    .collect();
                if rows.is_empty() {
                    div()
                        .p_4()
                        .text_color(p.muted())
                        .child(if b.archived_tab { t!("browse.no_archived") } else { t!("browse.no_match") })
                        .into_any_element()
                } else {
                    v_flex()
                        .id("browse-list")
                        .max_h(px(400.))
                        .overflow_y_scroll()
                        .py_1()
                        .children(rows.into_iter().map(|c| {
                            let member = self.channels.contains_key(&c.id);
                            let joining = b.joining.contains(&c.id);
                            let archived = c.delete_at > 0;
                            let channel = c.clone();
                            let action: AnyElement = if member || archived {
                                div()
                                    .flex_none()
                                    .text_sm()
                                    .text_color(p.muted())
                                    .child(if archived { t!("browse.view") } else { t!("browse.joined") })
                                    .into_any_element()
                            } else {
                                Button::new(SharedString::from(format!("join-{}", c.id)))
                                    .label(if joining { t!("browse.joining") } else { t!("browse.join") })
                                    .small()
                                    .primary()
                                    .disabled(joining)
                                    .into_any_element()
                            };
                            h_flex()
                                .id(SharedString::from(format!("br-{}", c.id)))
                                .gap_3()
                                .px_4()
                                .py_2()
                                .cursor_pointer()
                                .hover(|s| s.bg(p.hover()))
                                .child(div().when(archived, |el| el.opacity(0.6)).child(self.channel_badge(
                                    c,
                                    32.,
                                    p.center_bg,
                                )))
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .child(
                                            h_flex()
                                                .gap_1p5()
                                                .child(div().truncate().font_semibold().child(c.display_name.clone()))
                                                .when(archived, |el| {
                                                    el.child(
                                                        Icon::new(IconName::Archive)
                                                            .size(px(13.))
                                                            .text_color(p.muted()),
                                                    )
                                                }),
                                        )
                                        .when(!c.purpose.is_empty(), |el| {
                                            el.child(
                                                div()
                                                    .truncate()
                                                    .text_xs()
                                                    .text_color(p.muted())
                                                    .child(markdown::plain_text(&c.purpose)),
                                            )
                                        }),
                                )
                                .child(action)
                                .on_click(
                                    cx.listener(move |this, _, window, cx| {
                                        this.browse_open(channel.clone(), window, cx)
                                    }),
                                )
                        }))
                        .into_any_element()
                }
            }
        };

        let content = v_flex()
            .child(
                div().px_4().pb_3().child(
                    Input::new(&self.dialog_input)
                        .prefix(Icon::new(IconName::Search).size(px(14.)).text_color(p.muted()))
                        .cleanable(true),
                ),
            )
            .child(tabs)
            .child(body)
            .into_any_element();
        let footer = h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(div().flex_1().min_w_0().text_xs().text_color(p.muted()).child(t!("browse.hint")))
            .child(
                Button::new("browse-create")
                    .icon(Icon::new(IconName::Plus))
                    .label(t!("create.channel_title"))
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.dialog = None;
                        this.open_create(super::super::CreateKind::Channel, window, cx);
                    })),
            )
            .into_any_element();
        self.dialog_shell(IconName::Compass, t!("browse.title").into(), 560., content, Some(footer), cx)
    }

    // ------------------------------------------------------------------
    // Nouveau message direct ou de groupe
    // ------------------------------------------------------------------

    /// Personnes des messages directs récents, les plus récents d'abord.
    fn recent_partners(&self) -> Vec<String> {
        let me = self.me_id();
        let mut direct: Vec<&ChannelState> = self.channels.values().filter(|c| c.channel.is_direct()).collect();
        direct.sort_by_key(|c| std::cmp::Reverse(c.channel.recency()));
        direct
            .into_iter()
            .filter_map(|c| c.channel.direct_partner(me))
            .filter(|id| id != me && self.users.get(id).is_some_and(|u| u.delete_at == 0))
            .take(20)
            .collect()
    }

    pub(crate) fn open_direct_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.drawer = None;
        self.dialog = Some(Dialog::Direct(DirectPicker {
            results: Some(self.recent_partners()),
            selected: Vec::new(),
            highlight: 0,
        }));
        self.reset_dialog_input(t!("direct.search_placeholder").into(), String::new(), window, cx);
        cx.notify();
    }

    pub(crate) fn on_user_search(&mut self, term: String, users: Vec<User>, cx: &mut Context<Self>) {
        if self.dialog_term(cx) != term {
            return;
        }
        let ids: Vec<String> = users.iter().map(|u| u.id.clone()).collect();
        for u in users {
            self.users.entry(u.id.clone()).or_insert(u);
        }
        if let Some(Dialog::Direct(d)) = self.dialog.as_mut() {
            d.results = Some(ids);
            d.highlight = 0;
        }
    }

    pub(crate) fn toggle_direct_pick(&mut self, user_id: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Dialog::Direct(d)) = self.dialog.as_mut() {
            if let Some(i) = d.selected.iter().position(|id| *id == user_id) {
                d.selected.remove(i);
            } else if d.selected.len() < GROUP_MAX {
                d.selected.push(user_id);
            }
        }
        if !self.dialog_term(cx).is_empty() {
            self.dialog_input.update(cx, |s, cx| s.set_value("", window, cx));
            self.dialog_input_changed(cx);
        }
        self.dialog_input.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn start_direct(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Direct(d)) = self.dialog.as_ref() else { return };
        if d.selected.is_empty() {
            return;
        }
        let user_ids = d.selected.clone();
        self.dialog = None;
        self.send(Command::OpenGroup { user_ids });
        cx.notify();
    }

    /// Clavier : ↑/↓ choisissent, Entrée ajoute la personne choisie (ou
    /// ouvre la conversation), Retour arrière retire la dernière ajoutée.
    fn direct_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let term_empty = self.dialog_term(cx).is_empty();
        let Some(Dialog::Direct(d)) = self.dialog.as_mut() else { return };
        let n = d.results.as_ref().map_or(0, Vec::len);
        match key {
            "up" if n > 0 => d.highlight = (d.highlight + n - 1) % n,
            "down" if n > 0 => d.highlight = (d.highlight + 1) % n,
            "enter" => {
                let pick = d.results.as_ref().and_then(|r| r.get(d.highlight)).cloned();
                match pick {
                    Some(id) if !term_empty || d.selected.is_empty() => self.toggle_direct_pick(id, window, cx),
                    _ => self.start_direct(cx),
                }
                return;
            }
            "backspace" if term_empty => {
                d.selected.pop();
            }
            _ => {}
        }
        cx.notify();
    }

    fn render_direct(&self, d: &DirectPicker, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let searching = !self.dialog_term(cx).is_empty();
        let chips = d.selected.iter().map(|id| {
            let uid = id.clone();
            h_flex()
                .id(SharedString::from(format!("dm-chip-{id}")))
                .gap_1p5()
                .pl(px(3.))
                .pr_1p5()
                .py(px(3.))
                .rounded_full()
                .bg(p.button_bg.opacity(0.12))
                .text_sm()
                .child(self.avatar(id, 20., p.center_bg))
                .child(self.user_name(id))
                .child(
                    div()
                        .id(SharedString::from(format!("dm-chip-x-{id}")))
                        .rounded_full()
                        .cursor_pointer()
                        .hover(|s| s.bg(p.center_color.opacity(0.12)))
                        .child(Icon::new(IconName::X).size(px(13.)))
                        .on_click(
                            cx.listener(move |this, _, window, cx| this.toggle_direct_pick(uid.clone(), window, cx)),
                        ),
                )
        });
        let list: AnyElement = match &d.results {
            None => div().px_4().py_3().text_color(p.muted()).child(t!("members.searching")).into_any_element(),
            Some(ids) if ids.is_empty() => div()
                .px_4()
                .py_3()
                .text_color(p.muted())
                .child(if searching { t!("direct.nobody") } else { t!("direct.no_recent") })
                .into_any_element(),
            Some(ids) => v_flex()
                .id("dm-list")
                .max_h(px(340.))
                .overflow_y_scroll()
                .pb_1()
                .children(ids.iter().enumerate().map(|(i, id)| {
                    let picked = d.selected.contains(id);
                    let full = !picked && d.selected.len() >= GROUP_MAX;
                    let uid = id.clone();
                    let username = self.users.get(id).map(|u| format!("@{}", u.username)).unwrap_or_default();
                    h_flex()
                        .id(SharedString::from(format!("dm-row-{id}")))
                        .gap_3()
                        .px_4()
                        .py_1p5()
                        .cursor_pointer()
                        .when(i == d.highlight, |el| el.bg(p.button_bg.opacity(0.12)))
                        .hover(|s| s.bg(p.button_bg.opacity(0.07)))
                        .when(full, |el| el.opacity(0.5))
                        .child(self.avatar(id, 32., p.center_bg))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    h_flex()
                                        .gap_1p5()
                                        .child(div().truncate().text_sm().font_semibold().child(self.user_name(id)))
                                        .children(self.status_emoji(id, 15., cx)),
                                )
                                .child(div().truncate().text_xs().text_color(p.muted()).child(username)),
                        )
                        .child(if picked {
                            Icon::new(IconName::CircleCheck).size(px(18.)).text_color(p.button_bg).into_any_element()
                        } else {
                            Icon::new(IconName::Plus).size(px(16.)).text_color(p.muted()).into_any_element()
                        })
                        .on_click(
                            cx.listener(move |this, _, window, cx| this.toggle_direct_pick(uid.clone(), window, cx)),
                        )
                }))
                .into_any_element(),
        };
        let n = d.selected.len();
        let content = v_flex()
            .capture_action(cx.listener(|this, _: &MoveUp, window, cx| {
                this.direct_key("up", window, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|this, _: &MoveDown, window, cx| {
                this.direct_key("down", window, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|this, _: &Enter, window, cx| {
                this.direct_key("enter", window, cx);
                cx.stop_propagation();
            }))
            .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                if ev.keystroke.key == "backspace" {
                    this.direct_key("backspace", window, cx);
                }
            }))
            .child(
                v_flex()
                    .px_4()
                    .pb_2()
                    .gap_2()
                    .when(n > 0, |el| el.child(h_flex().flex_wrap().gap_1p5().children(chips)))
                    .child(
                        Input::new(&self.dialog_input)
                            .prefix(Icon::new(IconName::Search).size(px(14.)).text_color(p.muted()))
                            .cleanable(true),
                    ),
            )
            .child(div().px_4().pt_1().pb_1().text_xs().font_semibold().text_color(p.muted()).child(if searching {
                t!("direct.results")
            } else {
                t!("direct.recent")
            }))
            .child(list)
            .into_any_element();
        let footer = h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(div().flex_1().min_w_0().text_xs().text_color(p.muted()).child(if n >= GROUP_MAX {
                t!("direct.group_full", max = GROUP_MAX)
            } else {
                t!("direct.hint", max = GROUP_MAX)
            }))
            .child(
                Button::new("dm-cancel")
                    .label(t!("common.cancel"))
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.close_dialog(cx))),
            )
            .child(
                Button::new("dm-go")
                    .label(if n >= 2 { t!("direct.go_group", count = n) } else { t!("direct.go") })
                    .small()
                    .primary()
                    .disabled(n == 0)
                    .on_click(cx.listener(|this, _, _, cx| this.start_direct(cx))),
            )
            .into_any_element();
        self.dialog_shell(IconName::SquarePen, t!("direct.title").into(), 500., content, Some(footer), cx)
    }

    // ------------------------------------------------------------------
    // Statut personnalisé
    // ------------------------------------------------------------------

    pub(crate) fn my_custom_status(&self) -> Option<CustomStatus> {
        self.users.get(self.me_id()).or(self.me.as_ref()).and_then(User::custom_status)
    }

    pub(crate) fn open_status_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.my_custom_status();
        let (emoji, text, duration) = match current {
            Some(s) => (s.emoji, s.text, known_duration(&s.duration)),
            None => (String::new(), String::new(), "today"),
        };
        self.drawer = None;
        self.dialog = Some(Dialog::Status(StatusDraft { emoji, duration }));
        self.reset_dialog_input(t!("custom_status.placeholder").into(), text, window, cx);
        cx.notify();
    }

    pub(crate) fn set_status_emoji(&mut self, name: String, cx: &mut Context<Self>) {
        if let Some(Dialog::Status(d)) = self.dialog.as_mut() {
            d.emoji = name;
        }
        cx.notify();
    }

    pub(crate) fn apply_preset(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let (emoji, key, duration) = PRESETS[i];
        if let Some(Dialog::Status(d)) = self.dialog.as_mut() {
            d.emoji = emoji.into();
            d.duration = duration;
        }
        self.dialog_input.update(cx, |s, cx| {
            s.set_value(preset_text(key), window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    /// Enregistre le statut (ou l'efface : `None`), ici aussitôt et sur le serveur.
    fn apply_my_status(&mut self, status: Option<CustomStatus>) {
        let value = status
            .as_ref()
            .and_then(|s| serde_json::to_string(s).ok())
            .map(serde_json::Value::String)
            .unwrap_or_else(|| serde_json::Value::String(String::new()));
        let set = |u: &mut User| {
            if !u.props.is_object() {
                u.props = serde_json::json!({});
            }
            u.props["customStatus"] = value.clone();
        };
        let me = self.me_id().to_string();
        if let Some(u) = self.users.get_mut(&me) {
            set(u);
        }
        if let Some(u) = self.me.as_mut() {
            set(u);
        }
        self.send(Command::SetCustomStatus(status));
    }

    pub(crate) fn save_status(&mut self, cx: &mut Context<Self>) {
        let text: String = self.dialog_term(cx).chars().take(100).collect();
        let Some(Dialog::Status(d)) = self.dialog.as_ref() else { return };
        let status = if text.is_empty() && d.emoji.is_empty() {
            None
        } else {
            Some(CustomStatus {
                emoji: if d.emoji.is_empty() { "speech_balloon".into() } else { d.emoji.clone() },
                text,
                duration: d.duration.into(),
                expires_at: expires_at(d.duration, chrono::Local::now()),
            })
        };
        self.apply_my_status(status);
        self.dialog = None;
        cx.notify();
    }

    pub(crate) fn clear_status(&mut self, cx: &mut Context<Self>) {
        self.apply_my_status(None);
        self.dialog = None;
        cx.notify();
    }

    fn render_status_dialog(&self, d: &StatusDraft, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let empty_text = self.dialog_term(cx).is_empty();
        let emoji_button = div()
            .id("status-emoji")
            .flex_none()
            .size(px(36.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .cursor_pointer()
            .bg(p.center_color.opacity(0.06))
            .hover(|s| s.bg(p.center_color.opacity(0.12)))
            .child(if d.emoji.is_empty() {
                Icon::new(IconName::FaceSlightlySmiling).size(px(20.)).text_color(p.muted()).into_any_element()
            } else {
                self.named_emoji(&d.emoji, 22.)
            })
            .on_click(cx.listener(|this, _, window, cx| this.open_emoji_picker(PickerTarget::Status, window, cx)));
        let suggestions = v_flex().when(empty_text, |el| {
            el.child(
                div()
                    .px_4()
                    .pt_1()
                    .pb_1()
                    .text_xs()
                    .font_semibold()
                    .text_color(p.muted())
                    .child(t!("custom_status.suggestions")),
            )
            .children(PRESETS.iter().enumerate().map(|(i, (emoji, key, duration))| {
                h_flex()
                    .id(SharedString::from(format!("status-preset-{i}")))
                    .gap_3()
                    .px_4()
                    .py_1p5()
                    .cursor_pointer()
                    .hover(|s| s.bg(p.hover()))
                    .child(self.named_emoji(emoji, 18.))
                    .child(div().flex_1().text_sm().child(preset_text(key)))
                    .child(div().text_xs().text_color(p.muted()).child(duration_label(duration)))
                    .on_click(cx.listener(move |this, _, window, cx| this.apply_preset(i, window, cx)))
            }))
        });
        let durations = h_flex().flex_wrap().gap_1p5().children(DURATIONS.iter().map(|duration| {
            let active = d.duration == *duration;
            let value: &'static str = duration;
            div()
                .id(SharedString::from(format!("status-dur-{duration}")))
                .px_2p5()
                .py_1()
                .rounded_full()
                .cursor_pointer()
                .text_xs()
                .border_1()
                .when(active, |el| el.bg(p.button_bg).border_color(p.button_bg).text_color(p.button_color))
                .when(!active, |el| el.border_color(p.border()).hover(|s| s.bg(p.center_color.opacity(0.06))))
                .child(duration_label(duration))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(Dialog::Status(d)) = this.dialog.as_mut() {
                        d.duration = value;
                    }
                    cx.notify();
                }))
        }));
        let content = v_flex()
            .capture_action(cx.listener(|this, _: &Enter, _, cx| {
                this.save_status(cx);
                cx.stop_propagation();
            }))
            .child(
                h_flex()
                    .px_4()
                    .pb_3()
                    .gap_2()
                    .child(emoji_button)
                    .child(div().flex_1().child(Input::new(&self.dialog_input).cleanable(true))),
            )
            .child(suggestions)
            .child(
                v_flex()
                    .px_4()
                    .pt_3()
                    .pb_4()
                    .gap_2()
                    .child(div().text_xs().font_semibold().text_color(p.muted()).child(t!("custom_status.clear_after")))
                    .child(durations),
            )
            .into_any_element();
        let has_status = self.my_custom_status().is_some();
        let footer = h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .when(has_status, |el| {
                el.child(
                    Button::new("status-clear")
                        .label(t!("custom_status.clear"))
                        .small()
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| this.clear_status(cx))),
                )
            })
            .child(div().flex_1())
            .child(
                Button::new("status-cancel")
                    .label(t!("common.cancel"))
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.close_dialog(cx))),
            )
            .child(
                Button::new("status-save")
                    .label(t!("common.save"))
                    .small()
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| this.save_status(cx))),
            )
            .into_any_element();
        self.dialog_shell(
            IconName::FaceSlightlySmiling,
            t!("custom_status.title").into(),
            460.,
            content,
            Some(footer),
            cx,
        )
    }

    /// Emoji du statut personnalisé de quelqu'un, avec son texte en infobulle.
    pub(crate) fn status_emoji(&self, user_id: &str, size: f32, _cx: &App) -> Option<AnyElement> {
        let status = self.users.get(user_id)?.custom_status()?;
        let tip: SharedString = match (status.text.is_empty(), status.duration.as_str()) {
            (true, _) => status.emoji.clone().into(),
            (false, "" | "dont_clear") => status.text.clone().into(),
            (false, _) => format!("{} · {}", status.text, until_label(&status.expires_at)).into(),
        };
        Some(
            div()
                .id(SharedString::from(format!("cs-{user_id}-{size}")))
                .flex_none()
                .child(self.named_emoji(&status.emoji, size))
                .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx))
                .into_any_element(),
        )
    }

    /// Ligne « emoji texte » d'un statut personnalisé (carte de profil, pied
    /// de la barre latérale).
    pub(crate) fn status_line(&self, user_id: &str, size: f32) -> Option<Div> {
        let status = self.users.get(user_id)?.custom_status()?;
        Some(
            h_flex()
                .gap_1p5()
                .min_w_0()
                .child(div().flex_none().child(self.named_emoji(&status.emoji, size)))
                .when(!status.text.is_empty(), |el| el.child(div().truncate().child(status.text))),
        )
    }

    // ------------------------------------------------------------------
    // Préférences de notification d'un canal
    // ------------------------------------------------------------------

    pub(crate) fn open_notify_dialog(&mut self, channel_id: String, cx: &mut Context<Self>) {
        let Some(state) = self.channels.get(&channel_id) else { return };
        let n = &state.notify;
        self.dialog = Some(Dialog::Notify(NotifyDraft {
            desktop: if n.desktop.is_empty() { "default".into() } else { n.desktop.clone() },
            ignore_mentions: n.ignore_channel_mentions == "on",
            auto_follow: n.channel_auto_follow_threads == "on",
            muted: state.muted,
            channel_id,
        }));
        cx.notify();
    }

    pub(crate) fn save_notify(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Notify(d)) = self.dialog.take() else { return };
        let Some(state) = self.channels.get_mut(&d.channel_id) else { return };
        let on = |b: bool| if b { "on" } else { "off" }.to_string();
        let mut props: Vec<(&'static str, String)> = Vec::new();
        let n = &mut state.notify;
        if d.desktop != if n.desktop.is_empty() { "default" } else { n.desktop.as_str() } {
            n.desktop = d.desktop.clone();
            props.push(("desktop", d.desktop.clone()));
        }
        if d.ignore_mentions != (n.ignore_channel_mentions == "on") {
            n.ignore_channel_mentions = on(d.ignore_mentions);
            props.push(("ignore_channel_mentions", on(d.ignore_mentions)));
        }
        if d.auto_follow != (n.channel_auto_follow_threads == "on") {
            n.channel_auto_follow_threads = on(d.auto_follow);
            props.push(("channel_auto_follow_threads", on(d.auto_follow)));
        }
        if d.muted != state.muted {
            state.muted = d.muted;
            let value = if d.muted { "mention" } else { "all" };
            state.notify.mark_unread = value.into();
            props.push(("mark_unread", value.into()));
        }
        if !props.is_empty() {
            self.send(Command::SetChannelNotify { channel_id: d.channel_id, props });
        }
        cx.notify();
    }

    /// Préférences d'un canal modifiées (ici, ou depuis un autre appareil).
    pub(crate) fn on_member_updated(&mut self, member: ChannelMember) {
        if let Some(state) = self.channels.get_mut(&member.channel_id) {
            state.muted = member.is_muted();
            state.notify = member.notify_props;
        }
    }

    fn render_notify(&self, d: &NotifyDraft, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let Some(state) = self.channels.get(&d.channel_id) else { return div().into_any_element() };
        let direct = state.channel.is_direct() || state.channel.is_group();
        let account = self
            .me
            .as_ref()
            .and_then(|m| m.notify_props.get("desktop"))
            .map(String::as_str)
            .filter(|l| !l.is_empty())
            .unwrap_or("all");
        let level_label = |level: &str| -> SharedString {
            match level {
                "all" => t!("notify.level_all"),
                "mention" => t!("notify.level_mention"),
                _ => t!("notify.level_none"),
            }
            .into()
        };
        let choice = |value: &'static str, label: SharedString, cx: &mut Context<Self>| {
            let active = d.desktop == value;
            h_flex()
                .id(SharedString::from(format!("notify-{value}")))
                .gap_2p5()
                .px_4()
                .py_1p5()
                .cursor_pointer()
                .hover(|s| s.bg(p.hover()))
                .child(
                    div()
                        .flex_none()
                        .size(px(16.))
                        .rounded_full()
                        .border_2()
                        .border_color(if active { p.button_bg } else { p.center_color.opacity(0.35) })
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(active, |el| el.child(div().size(px(7.)).rounded_full().bg(p.button_bg))),
                )
                .child(div().text_sm().child(label))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(Dialog::Notify(d)) = this.dialog.as_mut() {
                        d.desktop = value.into();
                    }
                    cx.notify();
                }))
        };
        let toggle = |id: &'static str,
                      label: SharedString,
                      hint: SharedString,
                      checked: bool,
                      set: fn(&mut NotifyDraft, bool),
                      cx: &mut Context<Self>| {
            h_flex()
                .px_4()
                .py_2()
                .gap_3()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_sm().child(label))
                        .child(div().text_xs().text_color(p.muted()).child(hint)),
                )
                .child(Switch::new(id).checked(checked).on_click(cx.listener(move |this, checked: &bool, _, cx| {
                    if let Some(Dialog::Notify(d)) = this.dialog.as_mut() {
                        set(d, *checked);
                    }
                    cx.notify();
                })))
        };
        let section =
            |text: SharedString| div().px_4().pt_3().pb_1().text_xs().font_semibold().text_color(p.muted()).child(text);
        let content = v_flex()
            .pb_2()
            .child(section(t!("notify.desktop").into()))
            .child(choice("default", t!("notify.level_default", level = level_label(account)).into(), cx))
            .child(choice("all", level_label("all"), cx))
            .child(choice("mention", level_label("mention"), cx))
            .child(choice("none", level_label("none"), cx))
            .child(section(t!("notify.options").into()))
            .child(toggle(
                "notify-mute",
                t!("notify.mute").into(),
                t!("notify.mute_hint").into(),
                d.muted,
                |d, v| d.muted = v,
                cx,
            ))
            .when(!direct, |el| {
                el.child(toggle(
                    "notify-ignore",
                    t!("notify.ignore_mentions").into(),
                    t!("notify.ignore_mentions_hint").into(),
                    d.ignore_mentions,
                    |d, v| d.ignore_mentions = v,
                    cx,
                ))
                .child(toggle(
                    "notify-follow",
                    t!("notify.auto_follow").into(),
                    t!("notify.auto_follow_hint").into(),
                    d.auto_follow,
                    |d, v| d.auto_follow = v,
                    cx,
                ))
            })
            .into_any_element();
        let footer = h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(div().flex_1())
            .child(
                Button::new("notify-cancel")
                    .label(t!("common.cancel"))
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.close_dialog(cx))),
            )
            .child(
                Button::new("notify-save")
                    .label(t!("common.save"))
                    .small()
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| this.save_notify(cx))),
            )
            .into_any_element();
        let title = t!("notify.title", channel = self.channel_title(&state.channel));
        self.dialog_shell(IconName::BellRing, title.into(), 480., content, Some(footer), cx)
    }

    // ------------------------------------------------------------------
    // Fils de toute l'équipe
    // ------------------------------------------------------------------

    /// Fils avec des réponses non lues, et mentions qu'ils contiennent.
    pub(crate) fn unread_threads(&self) -> (usize, i64) {
        self.threads.as_ref().map_or((0, 0), |list| {
            let unread = list.iter().filter(|t| t.unread_replies > 0).count();
            (unread, list.iter().map(|t| t.unread_mentions).sum())
        })
    }

    pub(crate) fn mark_all_threads_read(&mut self, cx: &mut Context<Self>) {
        let Some(team_id) = self.team_id.clone() else { return };
        self.on_thread_read("", 0, 0);
        self.send(Command::MarkAllThreadsRead { team_id });
        cx.notify();
    }

    pub(crate) fn on_thread_updated(&mut self, team_id: String, thread: UserThread) {
        if !team_id.is_empty() && self.team_id.as_deref() != Some(team_id.as_str()) {
            return;
        }
        let Some(list) = self.threads.as_mut() else { return };
        match list.iter_mut().find(|t| t.id == thread.id) {
            Some(t) => *t = thread,
            None => list.push(thread),
        }
        list.sort_by_key(|t| std::cmp::Reverse(t.last_reply_at));
    }

    /// Lecture d'un fil (`thread_id` vide : tous les fils).
    pub(crate) fn on_thread_read(&mut self, thread_id: &str, unread_replies: i64, unread_mentions: i64) {
        for t in self.threads.iter_mut().flatten().filter(|t| thread_id.is_empty() || t.id == thread_id) {
            t.unread_replies = unread_replies;
            t.unread_mentions = unread_mentions;
        }
    }

    pub(crate) fn on_thread_follow(&mut self, thread_id: String, following: bool) {
        if following {
            if let Some(team_id) = self.team_id.clone() {
                self.send(Command::LoadThreads { team_id });
            }
        } else if let Some(list) = self.threads.as_mut() {
            list.retain(|t| t.id != thread_id);
        }
    }

    /// Entrée « Fils » en haut de la barre latérale, avec sa pastille.
    pub(crate) fn render_threads_entry(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.threads_available() {
            return None;
        }
        let p = self.palette;
        let (unread, mentions) = self.unread_threads();
        let selected = self.threads_all && self.side.as_ref().is_some_and(|s| s.kind == PanelKind::Threads);
        let color = if selected {
            p.sidebar_active_text
        } else if unread > 0 {
            p.sidebar_unread
        } else {
            p.sidebar_text.opacity(0.64)
        };
        Some(
            h_flex()
                .id("sidebar-threads")
                .flex_none()
                .h(px(33.))
                .mt_2()
                .gap(px(10.))
                .pl(px(18.))
                .pr_3()
                .cursor_pointer()
                .text_size(px(15.))
                .text_color(color)
                .border_l_4()
                .border_color(if selected { p.sidebar_active_border } else { transparent_black() })
                .when(selected, |el| el.bg(p.sidebar_text.opacity(0.16)))
                .when(!selected, |el| el.hover(|s| s.bg(p.sidebar_hover)))
                .child(
                    div().w(px(22.)).flex().justify_center().child(Icon::new(IconName::MessagesSquare).size(px(17.))),
                )
                .child(div().flex_1().when(unread > 0, |el| el.font_semibold()).child(t!("threads.sidebar")))
                .when(mentions > 0 || unread > 0, |el| {
                    let (bg, fg, n) = if mentions > 0 {
                        (p.mention_bg, p.mention_color, mentions)
                    } else {
                        (p.sidebar_text.opacity(0.16), p.sidebar_text, unread as i64)
                    };
                    el.child(
                        div()
                            .flex_none()
                            .min_w(px(20.))
                            .h(px(18.))
                            .px_1p5()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .text_size(px(11.))
                            .line_height(px(18.))
                            .pb(px(2.))
                            .font_bold()
                            .bg(bg)
                            .text_color(fg)
                            .child(centered_digits(n.to_string(), 11., cx)),
                    )
                })
                .on_click(cx.listener(|this, _, _, cx| this.open_threads_panel(true, cx)))
                .into_any_element(),
        )
    }

    /// Onglets « Tous / Non lus » et « Tout marquer comme lu » (vue de l'équipe).
    pub(crate) fn render_threads_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let total = self.threads.as_ref().map_or(0, Vec::len);
        let (unread, _) = self.unread_threads();
        h_flex()
            .px_3()
            .gap_1()
            .border_b_1()
            .border_color(p.border())
            .child(self.dialog_tab(
                "threads-tab-all",
                t!("threads.tab_all").into(),
                Some(total),
                !self.threads_unread,
                |this, _| this.threads_unread = false,
                cx,
            ))
            .child(self.dialog_tab(
                "threads-tab-unread",
                t!("threads.tab_unread").into(),
                Some(unread),
                self.threads_unread,
                |this, _| this.threads_unread = true,
                cx,
            ))
            .child(div().flex_1())
            .when(unread > 0, |el| {
                el.child(
                    Button::new("threads-read-all")
                        .icon(Icon::new(IconName::CheckCheck))
                        .label(t!("threads.mark_all_read"))
                        .xsmall()
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| this.mark_all_threads_read(cx))),
                )
            })
            .into_any_element()
    }
}

/// « jusqu'à 18:30 », « jusqu'à demain », « jusqu'au 24 nov. ».
fn until_label(expires_at: &str) -> String {
    let Ok(at) = chrono::DateTime::parse_from_rfc3339(expires_at) else { return String::new() };
    let local = at.with_timezone(&chrono::Local);
    let today = chrono::Local::now().date_naive();
    if local.date_naive() == today {
        t!("custom_status.until_time", time = local.format("%H:%M").to_string()).to_string()
    } else {
        t!("custom_status.until_day", day = day_label_inline(at.timestamp_millis())).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::expires_at;
    use chrono::{Datelike as _, TimeZone as _, Timelike as _};

    #[test]
    fn echeances_des_statuts() {
        // Mercredi 18 novembre 2026, 10 h, heure locale.
        let now = chrono::Local.with_ymd_and_hms(2026, 11, 18, 10, 0, 0).unwrap();
        let parse = |s: String| chrono::DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&chrono::Local);
        assert_eq!(expires_at("dont_clear", now), "");
        assert_eq!(parse(expires_at("one_hour", now)).hour(), 11);
        let today = parse(expires_at("today", now));
        assert_eq!((today.day(), today.hour(), today.minute()), (18, 23, 59));
        let week = parse(expires_at("this_week", now));
        assert_eq!((week.weekday(), week.day()), (chrono::Weekday::Sun, 22));
    }
}
