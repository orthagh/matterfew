//! Autour de l'envoi des messages : envoi en attente et réessai, transfert,
//! confirmation avant @channel / @all, avertissements en message direct,
//! emojis récents, et toasts de la liste des messages.

use gpui_kit::component::input::InputState;

use super::dialogs::Dialog;
use super::*;
use crate::api::Preference;
use crate::backend::Command;

/// Au-delà de ce nombre de membres, @channel / @all / @here demandent confirmation
/// (comme le client web).
const NOTIFY_CONFIRM_MIN: i64 = 5;
/// Emojis récents gardés.
const RECENT_EMOJIS: usize = 24;

/// Message parti vers le serveur, en attente de sa confirmation ; `failed` :
/// l'envoi a échoué (réessayer, modifier ou abandonner).
pub(crate) struct OutgoingPost {
    pub pending_id: String,
    pub pane: Pane,
    pub channel_id: String,
    pub root_id: String,
    pub message: String,
    pub file_ids: Vec<String>,
    pub failed: Option<String>,
}

/// « Transférer » : message choisi, destination, ligne choisie au clavier.
pub(crate) struct ForwardDraft {
    pub post_id: String,
    pub target: Option<String>,
    pub highlight: usize,
}

/// Mention de tout le canal dans un message : « @channel », « @all », « @here ».
pub(crate) fn channel_wide_mention(message: &str) -> Option<&'static str> {
    let lower = message.to_lowercase();
    ["@channel", "@all", "@here"].into_iter().find(|m| {
        lower.match_indices(m).any(|(i, _)| {
            let before = lower[..i].chars().next_back();
            let after = lower[i + m.len()..].chars().next();
            let word = |c: char| c.is_alphanumeric() || c == '_' || c == '-' || c == '.';
            !before.is_some_and(word) && !after.is_some_and(word)
        })
    })
}

/// Emojis récents d'après la préférence du serveur (deux formats existent :
/// liste de noms, ou liste d'objets `{name, usageCount}`).
pub(crate) fn parse_recent_emojis(value: &str) -> Vec<String> {
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(value) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            other => other.get("name").and_then(|n| n.as_str()).map(str::to_string),
        })
        .collect()
}

impl ChatApp {
    // ------------------------------------------------------------------
    // Envoi, échec, réessai
    // ------------------------------------------------------------------

    /// Envoie un message en gardant de quoi le réessayer s'il échoue.
    pub(crate) fn post_message(
        &mut self,
        pane: Pane,
        channel_id: String,
        root_id: String,
        message: String,
        file_ids: Vec<String>,
    ) {
        let pending_id = format!("{}:{}", self.me_id(), chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default());
        self.outgoing.push(OutgoingPost {
            pending_id: pending_id.clone(),
            pane,
            channel_id: channel_id.clone(),
            root_id: root_id.clone(),
            message: message.clone(),
            file_ids: file_ids.clone(),
            failed: None,
        });
        self.send(Command::Send { channel_id, root_id, message, file_ids, pending_id });
    }

    pub(crate) fn on_send_result(&mut self, pending_id: &str, error: Option<String>) {
        match error {
            None => self.outgoing.retain(|o| o.pending_id != pending_id),
            Some(error) => {
                if let Some(o) = self.outgoing.iter_mut().find(|o| o.pending_id == pending_id) {
                    o.failed = Some(error);
                }
            }
        }
    }

    pub(crate) fn retry_send(&mut self, pending_id: &str, cx: &mut Context<Self>) {
        let Some(o) = self.outgoing.iter_mut().find(|o| o.pending_id == pending_id) else { return };
        o.failed = None;
        let cmd = Command::Send {
            channel_id: o.channel_id.clone(),
            root_id: o.root_id.clone(),
            message: o.message.clone(),
            file_ids: o.file_ids.clone(),
            pending_id: o.pending_id.clone(),
        };
        self.send(cmd);
        cx.notify();
    }

    /// Message non envoyé : retour dans la zone de saisie pour le modifier.
    fn edit_failed(&mut self, pending_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.outgoing.iter().position(|o| o.pending_id == pending_id) else { return };
        let o = self.outgoing.remove(i);
        self.composer_for(o.pane).update(cx, |s, cx| {
            s.set_value(o.message, window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    fn discard_failed(&mut self, pending_id: &str, cx: &mut Context<Self>) {
        self.outgoing.retain(|o| o.pending_id != pending_id);
        cx.notify();
    }

    /// Messages non envoyés de ce canal (ou de ce fil), au-dessus de la saisie.
    pub(crate) fn render_failed_sends(&self, pane: Pane, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.palette;
        let channel = self.channel_id.as_deref()?;
        let root = match pane {
            Pane::Channel => "",
            Pane::Thread => self.thread.as_ref().map(|t| t.root_id.as_str())?,
        };
        let failed: Vec<&OutgoingPost> = self
            .outgoing
            .iter()
            .filter(|o| o.failed.is_some() && o.channel_id == channel && (pane == Pane::Channel || o.root_id == root))
            .filter(|o| pane == Pane::Thread || o.pane == Pane::Channel)
            .collect();
        if failed.is_empty() {
            return None;
        }
        Some(
            v_flex()
                .flex_none()
                .mx_5()
                .mb_2()
                .gap_1p5()
                .children(failed.into_iter().map(|o| {
                    let id = o.pending_id.clone();
                    let (retry, edit, drop) = (id.clone(), id.clone(), id.clone());
                    let preview: String = markdown::plain_text(&o.message).chars().take(120).collect();
                    h_flex()
                        .id(SharedString::from(format!("failed-{id}")))
                        .gap_2()
                        .px_3()
                        .py_2()
                        .rounded(px(8.))
                        .border_1()
                        .border_color(p.error.opacity(0.4))
                        .bg(p.error.opacity(0.07))
                        .text_sm()
                        .child(Icon::new(IconName::TriangleAlert).size(px(16.)).text_color(p.error))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(div().font_semibold().text_color(p.error).child(t!("outgoing.failed")))
                                .child(div().truncate().text_color(p.muted()).child(preview)),
                        )
                        .child(
                            Button::new(SharedString::from(format!("retry-{id}")))
                                .icon(Icon::new(IconName::RotateCcw))
                                .label(t!("outgoing.retry"))
                                .small()
                                .primary()
                                .on_click(cx.listener(move |this, _, _, cx| this.retry_send(&retry, cx))),
                        )
                        .child(
                            Button::new(SharedString::from(format!("edit-failed-{id}")))
                                .icon(Icon::new(IconName::Pencil))
                                .small()
                                .ghost()
                                .accessibility_label(t!("outgoing.edit"))
                                .tooltip(t!("outgoing.edit"))
                                .on_click(cx.listener(move |this, _, window, cx| this.edit_failed(&edit, window, cx))),
                        )
                        .child(
                            Button::new(SharedString::from(format!("drop-failed-{id}")))
                                .icon(Icon::new(IconName::Trash))
                                .small()
                                .ghost()
                                .accessibility_label(t!("outgoing.discard"))
                                .tooltip(t!("outgoing.discard"))
                                .on_click(cx.listener(move |this, _, _, cx| this.discard_failed(&drop, cx))),
                        )
                }))
                .into_any_element(),
        )
    }

    // ------------------------------------------------------------------
    // Entrée, Ctrl+Entrée
    // ------------------------------------------------------------------

    /// Réglages du serveur : envoyer avec Ctrl+Entrée (Entrée va à la ligne),
    /// ou seulement dans un bloc de code.
    pub(crate) fn apply_enter_preferences(&mut self, cx: &mut Context<Self>) {
        let pref = |name: &str| {
            self.raw_prefs.iter().find(|p| p.category == "advanced_settings" && p.name == name).map(|p| p.value.clone())
        };
        self.send_on_ctrl_enter = pref("send_on_ctrl_enter").as_deref() == Some("true");
        self.code_block_ctrl_enter = pref("code_block_ctrl_enter").as_deref() != Some("false");
        let submit = !self.send_on_ctrl_enter;
        for pane in [Pane::Channel, Pane::Thread] {
            self.composer_for(pane).update(cx, |s, cx| s.set_submit_on_enter(submit, cx));
        }
    }

    /// Entrée dans une zone de saisie : envoyer, ou aller à la ligne.
    pub(crate) fn composer_enter(&mut self, pane: Pane, secondary: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.send_on_ctrl_enter {
            // Entrée a déjà inséré le saut de ligne ; seul Ctrl+Entrée envoie.
            if secondary {
                self.send_from(pane, window, cx);
            }
            return;
        }
        let text = self.composer_for(pane).read(cx).value().to_string();
        let open_code_block = text.trim_start().starts_with("```") && text.matches("```").count() % 2 == 1;
        if !secondary && self.code_block_ctrl_enter && open_code_block {
            self.composer_for(pane).update(cx, |s, cx| s.insert("\n", window, cx));
            return;
        }
        self.send_from(pane, window, cx);
    }

    // ------------------------------------------------------------------
    // @channel / @all / @here
    // ------------------------------------------------------------------

    /// Faut-il confirmer avant d'envoyer (`true` : la fenêtre est ouverte) ?
    pub(crate) fn confirm_channel_mention(&mut self, pane: Pane, channel_id: &str, message: &str) -> bool {
        if std::mem::take(&mut self.notify_confirmed) {
            return false;
        }
        if self.server_config.get("EnableConfirmNotificationsToChannel").map(String::as_str) == Some("false") {
            return false;
        }
        let Some(mention) = channel_wide_mention(message) else { return false };
        let direct = self.channels.get(channel_id).is_some_and(|c| c.channel.is_direct() || c.channel.is_group());
        let count = self.members.get(channel_id).map_or(0, |m| m.1);
        if direct || count <= NOTIFY_CONFIRM_MIN {
            return false;
        }
        self.dialog = Some(Dialog::ConfirmNotify { pane, count: count - 1, mention });
        true
    }

    pub(crate) fn confirm_notify_send(&mut self, pane: Pane, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = None;
        self.notify_confirmed = true;
        self.send_from(pane, window, cx);
        self.notify_confirmed = false;
    }

    pub(crate) fn render_confirm_notify(
        &self,
        pane: Pane,
        count: i64,
        mention: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let content = v_flex()
            .px_4()
            .pb_4()
            .gap_2()
            .text_sm()
            .child(t!("confirm_notify.body", mention = mention, count = count))
            .child(div().text_color(p.muted()).child(t!("confirm_notify.hint")))
            .into_any_element();
        let footer = h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(div().flex_1())
            .child(
                Button::new("notify-confirm-cancel")
                    .label(t!("common.cancel"))
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.close_dialog(cx))),
            )
            .child(
                Button::new("notify-confirm-send")
                    .label(t!("confirm_notify.send"))
                    .small()
                    .primary()
                    .on_click(cx.listener(move |this, _, window, cx| this.confirm_notify_send(pane, window, cx))),
            )
            .into_any_element();
        self.dialog_shell(
            IconName::TriangleAlert,
            t!("confirm_notify.title", count = count).into(),
            440.,
            content,
            Some(footer),
            cx,
        )
    }

    // ------------------------------------------------------------------
    // Transférer un message
    // ------------------------------------------------------------------

    pub(crate) fn open_forward(&mut self, post_id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = Some(Dialog::Forward(ForwardDraft { post_id, target: None, highlight: 0 }));
        self.dialog_comment.update(cx, |s: &mut InputState, cx| s.set_value("", window, cx));
        self.dialog_input.update(cx, |s: &mut InputState, cx| {
            s.set_placeholder(t!("forward.search_placeholder"), window, cx);
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    /// Canaux et conversations où transférer, les plus récents d'abord.
    fn forward_candidates(&self, cx: &App) -> Vec<String> {
        let needle = crate::fuzzy::fold(self.dialog_input.read(cx).value().trim());
        let mut list: Vec<&crate::sidebar::ChannelState> = self
            .channels
            .values()
            .filter(|c| c.channel.delete_at == 0)
            .filter(|c| needle.is_empty() || crate::fuzzy::fold(&self.channel_title(&c.channel)).contains(&needle))
            .collect();
        list.sort_by_key(|c| std::cmp::Reverse(c.channel.recency()));
        list.into_iter().take(30).map(|c| c.channel.id.clone()).collect()
    }

    fn forward_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let candidates = self.forward_candidates(cx);
        let Some(Dialog::Forward(d)) = self.dialog.as_mut() else { return };
        let n = candidates.len();
        match key {
            "up" if n > 0 => d.highlight = (d.highlight + n - 1) % n,
            "down" if n > 0 => d.highlight = (d.highlight + 1) % n,
            "enter" => {
                if let Some(id) = candidates.get(d.highlight) {
                    d.target = Some(id.clone());
                    self.dialog_comment.update(cx, |s, cx| s.focus(window, cx));
                }
            }
            _ => {}
        }
        cx.notify();
    }

    pub(crate) fn send_forward(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Forward(d)) = self.dialog.as_ref() else { return };
        let Some(target) = d.target.clone() else { return };
        let server = self.server_url.clone().unwrap_or_default();
        let link = format!("{server}/{}/pl/{}", self.team_name(), d.post_id);
        let comment = self.dialog_comment.read(cx).value().trim().to_string();
        let message = if comment.is_empty() { link } else { format!("{comment}\n{link}") };
        let title = self.channels.get(&target).map(|c| self.channel_title(&c.channel)).unwrap_or_default();
        self.dialog = None;
        self.post_message(Pane::Channel, target, String::new(), message, Vec::new());
        self.flash(t!("forward.done", channel = title).to_string(), cx);
        cx.notify();
    }

    pub(crate) fn render_forward(&self, d: &ForwardDraft, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let candidates = self.forward_candidates(cx);
        let rows = candidates.iter().enumerate().filter_map(|(i, id)| {
            let state = self.channels.get(id)?;
            let icon: AnyElement = match state.channel.direct_partner(self.me_id()) {
                Some(partner) => self.avatar(&partner, 22., p.center_bg).into_any_element(),
                None => self.channel_badge(&state.channel, 22., p.center_bg),
            };
            let chosen = d.target.as_deref() == Some(id.as_str());
            let id = id.clone();
            Some(
                h_flex()
                    .id(SharedString::from(format!("fw-{id}")))
                    .gap_3()
                    .px_4()
                    .py_1p5()
                    .cursor_pointer()
                    .when(i == d.highlight, |el| el.bg(p.button_bg.opacity(0.1)))
                    .hover(|s| s.bg(p.button_bg.opacity(0.07)))
                    .child(div().w(px(22.)).flex().justify_center().child(icon))
                    .child(div().flex_1().min_w_0().truncate().text_sm().child(self.channel_title(&state.channel)))
                    .when(chosen, |el| el.child(Icon::new(IconName::CircleCheck).size(px(17.)).text_color(p.button_bg)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(Dialog::Forward(d)) = this.dialog.as_mut() {
                            d.target = Some(id.clone());
                        }
                        this.dialog_comment.update(cx, |s, cx| s.focus(window, cx));
                        cx.notify();
                    })),
            )
        });
        let original = self.find_post(&d.post_id);
        let preview = original.map(|post| {
            v_flex()
                .mx_4()
                .px_3()
                .py_2()
                .gap_1()
                .border_l_4()
                .border_color(p.button_bg.opacity(0.5))
                .bg(p.center_color.opacity(0.04))
                .rounded_r(px(6.))
                .child(
                    h_flex()
                        .gap_2()
                        .child(self.avatar(&post.user_id, 20., p.center_bg))
                        .child(div().text_sm().font_semibold().child(self.user_name(&post.user_id))),
                )
                .child(div().text_sm().line_clamp(3).child(markdown::plain_text(&post.message)))
        });
        let content = v_flex()
            .capture_action(cx.listener(|this, _: &MoveUp, window, cx| {
                this.forward_key("up", window, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|this, _: &MoveDown, window, cx| {
                this.forward_key("down", window, cx);
                cx.stop_propagation();
            }))
            .child(
                div()
                    .px_4()
                    .pb_2()
                    .capture_action(cx.listener(|this, _: &Enter, window, cx| {
                        this.forward_key("enter", window, cx);
                        cx.stop_propagation();
                    }))
                    .child(
                        Input::new(&self.dialog_input)
                            .prefix(Icon::new(IconName::Search).size(px(14.)).text_color(p.muted()))
                            .cleanable(true),
                    ),
            )
            .child(
                v_flex()
                    .id("fw-list")
                    .max_h(px(220.))
                    .overflow_y_scroll()
                    .pb_1()
                    .border_b_1()
                    .border_color(p.border())
                    .children(rows),
            )
            .child(
                div()
                    .px_4()
                    .pt_3()
                    .pb_2()
                    .capture_action(cx.listener(|this, _: &Enter, _, cx| {
                        this.send_forward(cx);
                        cx.stop_propagation();
                    }))
                    .child(Input::new(&self.dialog_comment)),
            )
            .children(preview)
            .child(div().h(px(12.)))
            .into_any_element();
        let ready = d.target.is_some();
        let footer = h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(div().flex_1())
            .child(
                Button::new("fw-cancel")
                    .label(t!("common.cancel"))
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.close_dialog(cx))),
            )
            .child(
                Button::new("fw-send")
                    .icon(Icon::new(IconName::Forward))
                    .label(t!("forward.send"))
                    .small()
                    .primary()
                    .disabled(!ready)
                    .on_click(cx.listener(|this, _, _, cx| this.send_forward(cx))),
            )
            .into_any_element();
        self.dialog_shell(IconName::Forward, t!("forward.title").into(), 520., content, Some(footer), cx)
    }

    // ------------------------------------------------------------------
    // Avertissements en message direct
    // ------------------------------------------------------------------

    /// En message direct : la personne est en « Ne pas déranger », absente
    /// (réponse automatique), ou c'est la nuit chez elle.
    pub(crate) fn render_dm_warning(&self) -> Option<AnyElement> {
        let p = self.palette;
        let channel = &self.channels.get(self.channel_id.as_deref()?)?.channel;
        let partner = channel.direct_partner(self.me_id()).filter(|id| id != self.me_id())?;
        let user = self.users.get(&partner)?;
        if user.is_bot {
            return None;
        }
        let name = self.user_name(&partner);
        let (icon, text): (IconName, String) = if self.status_of(&partner) == "dnd" {
            (IconName::CircleMinus, t!("dm_warning.dnd", name = name).to_string())
        } else if user.notify_props.get("auto_responder_active").map(String::as_str) == Some("true") {
            (IconName::Mail, t!("dm_warning.out_of_office", name = name).to_string())
        } else {
            use chrono::Timelike as _;
            let tz: chrono_tz::Tz = user.time_zone()?.parse().ok()?;
            let there = chrono::Utc::now().with_timezone(&tz);
            let mine = chrono::Local::now();
            let late = there.hour() >= 22 || there.hour() < 7;
            // Même heure que chez soi : rien à signaler.
            if !late || there.format("%H").to_string() == mine.format("%H").to_string() {
                return None;
            }
            (
                IconName::Moon,
                t!("dm_warning.late", name = name, time = there.format(super::settings::time_format()).to_string())
                    .to_string(),
            )
        };
        Some(
            h_flex()
                .flex_none()
                .mx_5()
                .mb_1p5()
                .gap_2()
                .text_xs()
                .text_color(p.muted())
                .child(Icon::new(icon).size(px(14.)))
                .child(text)
                .into_any_element(),
        )
    }

    // ------------------------------------------------------------------
    // Emojis récents
    // ------------------------------------------------------------------

    pub(crate) fn load_recent_emojis(&mut self) {
        let me = self.me_id().to_string();
        self.recent_emojis = self
            .raw_prefs
            .iter()
            .find(|p| p.category == "recent_emojis" && (p.name == me || p.name.is_empty()))
            .map(|p| parse_recent_emojis(&p.value))
            .unwrap_or_default();
    }

    /// Emoji utilisé (réaction, saisie) : en tête des récents, gardé sur le serveur.
    pub(crate) fn note_emoji_used(&mut self, name: &str) {
        if name.is_empty() {
            return;
        }
        self.recent_emojis.retain(|n| n != name);
        self.recent_emojis.insert(0, name.to_string());
        self.recent_emojis.truncate(RECENT_EMOJIS);
        let me = self.me_id().to_string();
        let value: Vec<serde_json::Value> =
            self.recent_emojis.iter().map(|n| serde_json::json!({ "name": n, "usageCount": 1 })).collect();
        let pref = Preference {
            user_id: me.clone(),
            category: "recent_emojis".into(),
            name: me,
            value: serde_json::Value::Array(value).to_string(),
        };
        self.send(Command::SavePreferences(vec![pref]));
    }

    /// Les trois réactions rapides : les emojis récents, sinon 👍 ❤️ 😂.
    pub(crate) fn quick_reactions(&self) -> Vec<String> {
        let mut out: Vec<String> = self.recent_emojis.iter().take(3).cloned().collect();
        for d in QUICK_REACTIONS {
            if out.len() < 3 && !out.iter().any(|n| n == d) {
                out.push(d.to_string());
            }
        }
        out
    }

    // ------------------------------------------------------------------
    // Réactions, mentions hors canal
    // ------------------------------------------------------------------

    /// « Léa Bernard, vous et 2 autres ont réagi avec :+1: ».
    pub(crate) fn who_reacted(&self, post: &Post, emoji: &str) -> String {
        let me = self.me_id();
        let mut ids: Vec<&str> =
            post.metadata.reactions.iter().filter(|r| r.emoji_name == emoji).map(|r| r.user_id.as_str()).collect();
        // Soi d'abord, comme le client web.
        ids.sort_by_key(|id| *id != me);
        let names: Vec<String> =
            ids.iter().map(|id| if *id == me { t!("reactions.you").to_string() } else { self.user_name(id) }).collect();
        let list = match names.len() {
            0 => return String::new(),
            1 => names[0].clone(),
            n if n <= 6 => t!("reactions.and", first = names[..n - 1].join(", "), last = names[n - 1]).to_string(),
            n => t!("reactions.and_others", names = names[..5].join(", "), count = n - 5).to_string(),
        };
        t!("reactions.who", names = list, emoji = format!(":{emoji}:")).to_string()
    }

    /// Message du serveur « X n'a pas été notifié… » : bouton pour ajouter
    /// ces personnes au canal.
    pub(crate) fn out_of_channel_action(&self, post: &Post, entity: &Entity<Self>) -> Option<AnyElement> {
        let ids: Vec<String> = post
            .props
            .get("add_channel_member")?
            .get("not_in_channel_user_ids")?
            .as_array()?
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        if ids.is_empty() {
            return None;
        }
        let (entity, channel_id) = (entity.clone(), post.channel_id.clone());
        let label = if ids.len() == 1 {
            t!("out_of_channel.add_one", name = self.user_name(&ids[0]))
        } else {
            t!("out_of_channel.add_many", count = ids.len())
        };
        Some(
            div()
                .mt_1p5()
                .child(
                    Button::new(SharedString::from(format!("add-mentioned-{}", post.id)))
                        .icon(Icon::new(IconName::UserPlus))
                        .label(label)
                        .small()
                        .outline()
                        .on_click(move |_, _, cx| {
                            let (channel_id, ids) = (channel_id.clone(), ids.clone());
                            entity.update(cx, |this, cx| this.add_mentioned_members(channel_id, ids, cx));
                        }),
                )
                .into_any_element(),
        )
    }

    fn add_mentioned_members(&mut self, channel_id: String, user_ids: Vec<String>, cx: &mut Context<Self>) {
        let count = user_ids.len();
        for user_id in user_ids {
            self.send(Command::AddChannelMember { channel_id: channel_id.clone(), user_id });
        }
        self.flash(t!("out_of_channel.added", count = count).to_string(), cx);
    }

    // ------------------------------------------------------------------
    // Toasts de la liste des messages
    // ------------------------------------------------------------------

    /// « Nouveaux messages depuis 14:32 » quand la ligne des nouveaux messages
    /// est au-dessus de ce qui est affiché : un clic y remonte.
    pub(crate) fn render_unread_toast(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.palette;
        let since = self.new_since?;
        let channel = self.channel_id.clone()?;
        if self.unread_toast_closed.as_deref() == Some(channel.as_str()) {
            return None;
        }
        let first = self.posts.iter().position(|v| v.post.create_at > since && v.post.user_id != self.me_id())?;
        if self.scroller.read(cx).logical_scroll_top().item_ix <= first {
            return None;
        }
        let time = format_hour(self.posts[first].post.create_at);
        let closed = channel.clone();
        Some(
            div()
                .absolute()
                .top(px(10.))
                .left_0()
                .w_full()
                .flex()
                .justify_center()
                .child(
                    h_flex()
                        .id("unread-toast")
                        .occlude()
                        .gap_2()
                        .pl_3()
                        .pr_1()
                        .py_1()
                        .rounded_full()
                        .bg(p.button_bg)
                        .text_color(p.button_color)
                        .text_sm()
                        .font_semibold()
                        .shadow_md()
                        .child(
                            h_flex()
                                .id("unread-toast-jump")
                                .gap_1p5()
                                .cursor_pointer()
                                .child(Icon::new(IconName::ArrowUp).size(px(15.)))
                                .child(t!("toast.new_since", time = time))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.scroller.update(cx, |s, cx| {
                                        s.scroll_to_item(first.saturating_sub(1), cx);
                                    });
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("unread-toast-close")
                                .p_1()
                                .rounded_full()
                                .cursor_pointer()
                                .hover(|s| s.bg(white().opacity(0.18)))
                                .child(Icon::new(IconName::X).size(px(14.)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.unread_toast_closed = Some(closed.clone());
                                    cx.notify();
                                })),
                        ),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{channel_wide_mention, parse_recent_emojis};

    #[test]
    fn mentions_de_tout_le_canal() {
        assert_eq!(channel_wide_mention("Réunion à 14 h @channel !"), Some("@channel"));
        assert_eq!(channel_wide_mention("@ALL merci"), Some("@all"));
        assert_eq!(channel_wide_mention("écrire à @channelle"), None);
        assert_eq!(channel_wide_mention("courriel lea@here.fr"), None);
    }

    #[test]
    fn emojis_recents_deux_formats() {
        assert_eq!(parse_recent_emojis(r#"["+1","tada"]"#), ["+1", "tada"]);
        assert_eq!(parse_recent_emojis(r#"[{"name":"joy","usageCount":3}]"#), ["joy"]);
        assert!(parse_recent_emojis("n'importe quoi").is_empty());
    }
}
