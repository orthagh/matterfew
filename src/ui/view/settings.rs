//! Fenêtre « Paramètres » (Ctrl+Maj+A) : notifications, affichage, barre
//! latérale, avancé. Les réglages vivent sur le serveur (préférences de
//! notification du compte, préférences utilisateur), sauf ceux propres à
//! cette application.

use std::collections::HashMap;

use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::component::switch::Switch;

use super::dialogs::Dialog;
use super::*;
use crate::api::Preference;
use crate::backend::Command;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SettingsTab {
    Notifications,
    Display,
    Sidebar,
    Advanced,
}

/// Réglages en cours de modification.
pub(crate) struct SettingsDraft {
    pub tab: SettingsTab,
    // Notifications (`notify_props` du compte).
    pub desktop: String,
    pub desktop_threads: String,
    pub sound_on: bool,
    pub sound: &'static str,
    pub email: bool,
    pub first_name: bool,
    pub channel_mentions: bool,
    pub comments: String,
    pub auto_reply: bool,
    pub mention_keys: Entity<InputState>,
    pub highlight_keys: Entity<InputState>,
    pub auto_reply_text: Entity<TextareaState>,
    // Affichage.
    pub crt: bool,
    pub clock24: bool,
    pub name_format: String,
    // Barre latérale.
    pub unread_section: bool,
    pub dm_limit: usize,
    // Avancé.
    pub ctrl_enter: bool,
    pub code_block_ctrl_enter: bool,
    pub show_formatting: bool,
    pub rich_editor: bool,
    pub check_updates: bool,
    pub mcp_enabled: bool,
}

/// Nombres de messages directs proposés (10000 = tous), comme le client web.
const DM_LIMITS: [usize; 5] = [10000, 10, 15, 20, 40];

/// Formats de nom : (valeur, clé de libellé).
const NAME_FORMATS: [&str; 3] = ["username", "nickname_full_name", "full_name"];

/// Heure au format choisi (24 h par défaut).
static CLOCK_24H: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Motif `chrono` des heures affichées (« 14:05 » ou « 2:05 PM »).
pub(crate) fn time_format() -> &'static str {
    if CLOCK_24H.load(std::sync::atomic::Ordering::Relaxed) { "%H:%M" } else { "%-I:%M %p" }
}

/// Mots d'une liste séparée par des virgules.
fn split_list(text: &str) -> Vec<String> {
    text.split(',').map(|k| k.trim().to_string()).filter(|k| !k.is_empty()).collect()
}

impl ChatApp {
    fn pref_value(&self, category: &str, name: &str) -> Option<&str> {
        self.raw_prefs.iter().find(|p| p.category == category && p.name == name).map(|p| p.value.as_str())
    }

    /// Format des noms : préférence, sinon réglage du serveur.
    fn name_format(&self) -> String {
        self.pref_value("display_settings", "name_format")
            .map(str::to_string)
            .or_else(|| self.server_config.get("TeammateNameDisplay").cloned())
            .unwrap_or_default()
    }

    /// Heure et noms selon les préférences (appelé à chaque changement).
    pub(crate) fn apply_display_preferences(&mut self) {
        let clock24 = self.pref_value("display_settings", "use_military_time").is_none_or(|v| v == "true");
        CLOCK_24H.store(clock24, std::sync::atomic::Ordering::Relaxed);
        crate::api::set_name_format(&self.name_format());
    }

    /// Mots-clés surlignés d'après les préférences du compte.
    pub(crate) fn refresh_keywords(&mut self, cx: &mut Context<Self>) {
        let Some(me) = &self.me else { return };
        let props = &me.notify_props;
        let get = |k: &str| props.get(k).map(String::as_str).unwrap_or("");
        let own = [me.username.to_lowercase(), format!("@{}", me.username.to_lowercase())];
        let mut mention: Vec<String> =
            split_list(get("mention_keys")).into_iter().filter(|k| !own.contains(&k.to_lowercase())).collect();
        if get("first_name") == "true" && !me.first_name.is_empty() {
            mention.push(me.first_name.clone());
        }
        let mut all = mention.clone();
        all.extend(split_list(get("highlight_keys")));
        self.mention_keywords = mention.iter().map(|k| k.to_lowercase()).collect();
        if crate::markdown::set_keywords(all) {
            self.refresh_markdown(cx);
        }
    }

    /// Les fils réduits peuvent-ils être activés ou désactivés ici ?
    fn crt_choice(&self) -> bool {
        matches!(
            self.server_config.get("CollapsedThreads").map(String::as_str).unwrap_or("default_on"),
            "default_on" | "default_off"
        )
    }

    pub(crate) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let props = self.me.as_ref().map(|m| m.notify_props.clone()).unwrap_or_default();
        let get = |k: &str| props.get(k).cloned().unwrap_or_default();
        let username = self.me.as_ref().map(|m| m.username.to_lowercase()).unwrap_or_default();
        let custom_keys: Vec<String> = split_list(&get("mention_keys"))
            .into_iter()
            .filter(|k| k.to_lowercase() != username && k.to_lowercase() != format!("@{username}"))
            .collect();
        let input = |value: String, placeholder: SharedString, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut s = InputState::new(window, cx).placeholder(placeholder.to_string());
                s.set_value(value, window, cx);
                s
            })
        };
        let mention_keys = input(custom_keys.join(", "), t!("settings.keys_placeholder").into(), window, cx);
        let highlight_keys = input(get("highlight_keys"), t!("settings.keys_placeholder").into(), window, cx);
        let auto_reply_text = cx.new(|cx| {
            let mut s = TextareaState::new(window, cx)
                .auto_grow(2, 5)
                .placeholder(t!("settings.auto_reply_placeholder").to_string());
            s.set_value(get("auto_responder_message"), window, cx);
            s
        });
        let or = |value: String, default: &str| if value.is_empty() { default.to_string() } else { value };
        self.dialog = Some(Dialog::Settings(Box::new(SettingsDraft {
            tab: SettingsTab::Notifications,
            desktop: or(get("desktop"), "mention"),
            desktop_threads: or(get("desktop_threads"), "all"),
            sound_on: get("desktop_sound") != "false",
            sound: crate::sound::normalize(&get("desktop_notification_sound")),
            email: get("email") != "false",
            first_name: get("first_name") == "true",
            channel_mentions: get("channel") != "false",
            comments: or(get("comments"), "never"),
            auto_reply: get("auto_responder_active") == "true",
            mention_keys,
            highlight_keys,
            auto_reply_text,
            crt: self.crt,
            clock24: self.pref_value("display_settings", "use_military_time").is_none_or(|v| v == "true"),
            name_format: or(self.name_format(), "full_name"),
            unread_section: self.prefs.show_unread_section,
            dm_limit: self.prefs.dm_limit,
            ctrl_enter: self.send_on_ctrl_enter,
            code_block_ctrl_enter: self.code_block_ctrl_enter,
            show_formatting: self.settings.show_formatting,
            rich_editor: self.settings.rich_editor,
            check_updates: self.settings.check_updates,
            mcp_enabled: self.settings.mcp_enabled,
        })));
        cx.notify();
    }

    fn settings_draft(&mut self) -> Option<&mut SettingsDraft> {
        match self.dialog.as_mut() {
            Some(Dialog::Settings(d)) => Some(d),
            _ => None,
        }
    }

    /// Enregistre ce qui a changé, sur le serveur et ici.
    pub(crate) fn save_settings(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Settings(d)) = self.dialog.take() else { return };
        let me = self.me_id().to_string();

        // Préférences de notification du compte : la carte complète.
        if let Some(user) = self.me.as_ref() {
            let mut props = user.notify_props.clone();
            let username = user.username.clone();
            let flag = |b: bool| if b { "true" } else { "false" }.to_string();
            let mut keys = split_list(d.mention_keys.read(cx).value().as_ref());
            // Le nom d'utilisateur, s'il figurait dans la liste, y reste.
            for own in [username.clone(), format!("@{username}")] {
                if split_list(props.get("mention_keys").map(String::as_str).unwrap_or("")).contains(&own) {
                    keys.push(own);
                }
            }
            let changes: [(&str, String); 11] = [
                ("desktop", d.desktop.clone()),
                ("desktop_threads", d.desktop_threads.clone()),
                ("desktop_sound", flag(d.sound_on)),
                ("desktop_notification_sound", d.sound.to_string()),
                ("email", flag(d.email)),
                ("first_name", flag(d.first_name)),
                ("channel", flag(d.channel_mentions)),
                ("comments", d.comments.clone()),
                ("mention_keys", keys.join(",")),
                ("highlight_keys", split_list(d.highlight_keys.read(cx).value().as_ref()).join(",")),
                ("auto_responder_active", flag(d.auto_reply)),
            ];
            let message = d.auto_reply_text.read(cx).value().trim().to_string();
            let mut changed = false;
            for (key, value) in changes.into_iter().chain([("auto_responder_message", message)]) {
                if props.get(key) != Some(&value) {
                    // Valeurs par défaut absentes : inutile de les écrire.
                    if !props.contains_key(key) && value.is_empty() {
                        continue;
                    }
                    props.insert(key.to_string(), value);
                    changed = true;
                }
            }
            if changed {
                if let Some(user) = self.me.as_mut() {
                    user.notify_props = props.clone();
                }
                self.send(Command::SaveNotifyProps(props));
                self.refresh_keywords(cx);
            }
        }

        // Préférences utilisateur.
        let mut wanted: Vec<(&str, &str, String)> = vec![
            ("display_settings", "use_military_time", d.clock24.to_string()),
            ("display_settings", "name_format", d.name_format.clone()),
            ("sidebar_settings", "show_unread_section", d.unread_section.to_string()),
            ("sidebar_settings", "limit_visible_dms_gms", d.dm_limit.to_string()),
            ("advanced_settings", "send_on_ctrl_enter", d.ctrl_enter.to_string()),
            ("advanced_settings", "code_block_ctrl_enter", d.code_block_ctrl_enter.to_string()),
        ];
        if self.crt_choice() {
            wanted.push(("display_settings", "collapsed_reply_threads", if d.crt { "on" } else { "off" }.into()));
        }
        let defaults: HashMap<(&str, &str), String> = HashMap::from([
            (("display_settings", "use_military_time"), "true".to_string()),
            (("display_settings", "name_format"), self.name_format()),
            (("sidebar_settings", "show_unread_section"), self.prefs.show_unread_section.to_string()),
            (("sidebar_settings", "limit_visible_dms_gms"), self.prefs.dm_limit.to_string()),
            (("advanced_settings", "send_on_ctrl_enter"), self.send_on_ctrl_enter.to_string()),
            (("advanced_settings", "code_block_ctrl_enter"), self.code_block_ctrl_enter.to_string()),
            (("display_settings", "collapsed_reply_threads"), if self.crt { "on" } else { "off" }.to_string()),
        ]);
        let mut prefs = Vec::new();
        for (category, name, value) in wanted {
            let current = self
                .pref_value(category, name)
                .map(str::to_string)
                .or_else(|| defaults.get(&(category, name)).cloned());
            if current.as_deref() == Some(value.as_str()) {
                continue;
            }
            let pref = Preference { user_id: me.clone(), category: category.into(), name: name.into(), value };
            match self.raw_prefs.iter_mut().find(|p| p.category == pref.category && p.name == pref.name) {
                Some(p) => p.value = pref.value.clone(),
                None => self.raw_prefs.push(pref.clone()),
            }
            prefs.push(pref);
        }
        if !prefs.is_empty() {
            self.send(Command::SavePreferences(prefs));
            self.refresh_preferences(cx);
            self.apply_display_preferences();
            self.refresh_markdown(cx);
        }

        // Réglages de cette application.
        if self.settings.check_updates != d.check_updates {
            self.settings.check_updates = d.check_updates;
            self.settings.save();
        }
        if self.settings.mcp_enabled != d.mcp_enabled {
            self.settings.mcp_enabled = d.mcp_enabled;
            self.settings.save();
            self.apply_mcp();
        }
        if self.settings.show_formatting != d.show_formatting || self.settings.rich_editor != d.rich_editor {
            let rich_changed = self.settings.rich_editor != d.rich_editor;
            self.settings.show_formatting = d.show_formatting;
            self.settings.rich_editor = d.rich_editor;
            self.settings.save();
            if rich_changed {
                self.check_spelling(Pane::Channel, cx);
                self.check_spelling(Pane::Thread, cx);
            }
        }
        self.flash(t!("settings.saved").to_string(), cx);
        cx.notify();
    }

    pub(crate) fn render_settings(&self, d: &SettingsDraft, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let tabs = [
            (SettingsTab::Notifications, IconName::Bell, t!("settings.tab.notifications")),
            (SettingsTab::Display, IconName::Monitor, t!("settings.tab.display")),
            (SettingsTab::Sidebar, IconName::PanelLeft, t!("settings.tab.sidebar")),
            (SettingsTab::Advanced, IconName::SlidersHorizontal, t!("settings.tab.advanced")),
        ];
        let nav = v_flex()
            .id("settings-nav")
            .flex_none()
            .w(px(190.))
            .h_full()
            .p_2()
            .gap_0p5()
            .border_r_1()
            .border_color(p.border())
            .role(gpui_kit::Role::TabList)
            .children(tabs.into_iter().map(|(tab, icon, label)| {
                let active = d.tab == tab;
                h_flex()
                    .id(SharedString::from(format!("settings-tab-{tab:?}")))
                    .gap_2()
                    .px_2p5()
                    .py_1p5()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_sm()
                    .role(gpui_kit::Role::Tab)
                    .aria_selected(active)
                    .aria_label(SharedString::from(label.to_string()))
                    .when(active, |el| el.bg(p.button_bg.opacity(0.12)).text_color(p.button_bg).font_semibold())
                    .when(!active, |el| el.hover(|s| s.bg(p.hover())))
                    .child(Icon::new(icon).size(px(16.)))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(d) = this.settings_draft() {
                            d.tab = tab;
                        }
                        cx.notify();
                    }))
            }));
        let content = match d.tab {
            SettingsTab::Notifications => self.render_settings_notifications(d, cx),
            SettingsTab::Display => self.render_settings_display(d, cx),
            SettingsTab::Sidebar => self.render_settings_sidebar(d, cx),
            SettingsTab::Advanced => self.render_settings_advanced(d, cx),
        };
        let height = (self.viewport_h - 230.).clamp(260., 560.);
        let body = h_flex()
            .h(px(height))
            .border_t_1()
            .border_color(p.border())
            .child(nav)
            .child(v_flex().id("settings-scroll").flex_1().min_w_0().h_full().overflow_y_scroll().pb_4().child(content))
            .into_any_element();
        let footer = h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(
                div().flex_1().min_w_0().truncate().text_xs().text_color(p.muted()).child(t!("settings.server_hint")),
            )
            .child(
                Button::new("settings-cancel")
                    .label(t!("common.cancel"))
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.close_dialog(cx))),
            )
            .child(
                Button::new("settings-save")
                    .label(t!("common.save"))
                    .small()
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| this.save_settings(cx))),
            )
            .into_any_element();
        self.dialog_shell(IconName::Settings, t!("settings.title").into(), 760., body, Some(footer), cx)
    }

    // ---------------------------------------------------------------
    // Éléments communs
    // ---------------------------------------------------------------

    fn settings_section(&self, title: SharedString, hint: Option<SharedString>) -> Div {
        let p = self.palette;
        v_flex()
            .px_5()
            .pt_4()
            .pb_1()
            .child(div().text_sm().font_semibold().child(title))
            .children(hint.map(|h| div().text_xs().text_color(p.muted()).child(h)))
    }

    /// Choix exclusif (boutons radio).
    fn settings_choices(
        &self,
        id: &'static str,
        current: &str,
        options: Vec<(String, SharedString)>,
        set: fn(&mut SettingsDraft, String),
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.palette;
        v_flex().px_3().children(options.into_iter().map(|(value, label)| {
            let active = current == value;
            h_flex()
                .id(SharedString::from(format!("{id}-{value}")))
                .gap_2p5()
                .px_2()
                .py_1()
                .rounded(px(6.))
                .cursor_pointer()
                .role(gpui_kit::Role::RadioButton)
                .aria_selected(active)
                .aria_label(label.clone())
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
                    if let Some(d) = this.settings_draft() {
                        set(d, value.clone());
                    }
                    cx.notify();
                }))
        }))
    }

    /// Interrupteur avec son explication.
    /// Serveur MCP actif : adresse, état, commande à copier, nouveau jeton.
    fn render_mcp_details(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.settings.mcp_enabled || self.demo {
            return None;
        }
        let p = self.palette;
        let port = self.settings.mcp_port;
        let (status, color): (SharedString, Hsla) = match &self.mcp_status {
            Some(Ok(Some(port))) => (t!("mcp.listening", url = crate::mcp::url(*port)).into(), p.muted()),
            Some(Err(err)) => (t!("mcp.failed", error = err).into(), p.error),
            _ => (t!("mcp.starting").into(), p.muted()),
        };
        Some(
            v_flex()
                .px_5()
                .pt_1()
                .gap_2()
                .child(div().text_xs().text_color(color).child(status))
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("settings-mcp-copy")
                                .label(t!("mcp.copy_command"))
                                .icon(Icon::new(IconName::Copy))
                                .small()
                                .outline()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let command = crate::mcp::claude_command(port, &this.settings.mcp_token);
                                    this.copy(command, &t!("mcp.command"), cx);
                                })),
                        )
                        .child(
                            Button::new("settings-mcp-token")
                                .label(t!("mcp.new_token"))
                                .icon(Icon::new(IconName::RotateCcw))
                                .small()
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.settings.mcp_token = crate::mcp::new_token();
                                    this.settings.save();
                                    this.apply_mcp();
                                    this.flash(t!("mcp.token_renewed").into(), cx);
                                })),
                        ),
                )
                .child(div().text_xs().text_color(p.muted()).child(t!("mcp.token_hint")))
                .into_any_element(),
        )
    }

    fn settings_toggle(
        &self,
        id: &'static str,
        label: SharedString,
        hint: Option<SharedString>,
        checked: bool,
        set: fn(&mut SettingsDraft, bool),
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.palette;
        h_flex()
            .px_5()
            .py_1p5()
            .gap_3()
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().child(label))
                    .children(hint.map(|h| div().text_xs().text_color(p.muted()).child(h))),
            )
            .child(Switch::new(id).checked(checked).on_click(cx.listener(move |this, checked: &bool, _, cx| {
                if let Some(d) = this.settings_draft() {
                    set(d, *checked);
                }
                cx.notify();
            })))
    }

    // ---------------------------------------------------------------
    // Onglets
    // ---------------------------------------------------------------

    fn render_settings_notifications(&self, d: &SettingsDraft, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let level = |key: &str| -> SharedString {
            match key {
                "all" => t!("notify.level_all"),
                "mention" => t!("notify.level_mention"),
                _ => t!("notify.level_none"),
            }
            .into()
        };
        let first_name = self.me.as_ref().map(|m| m.first_name.clone()).filter(|n| !n.is_empty());
        let username = self.me.as_ref().map(|m| m.username.clone()).unwrap_or_default();
        let sound_picker = Button::new("settings-sound")
            .label(SharedString::from(d.sound))
            .icon(Icon::new(IconName::ChevronDown))
            .small()
            .outline()
            .disabled(!d.sound_on)
            .dropdown_menu({
                let entity = cx.entity();
                let current = d.sound;
                move |menu, _, _| {
                    crate::sound::SOUNDS.iter().fold(menu.min_w(px(160.)), |menu, name| {
                        let entity = entity.clone();
                        menu.item(PopupMenuItem::new(*name).checked(current == *name).on_click(move |_, _, cx| {
                            entity.update(cx, |this, cx| {
                                if let Some(d) = this.settings_draft() {
                                    d.sound = name;
                                }
                                crate::sound::play(name);
                                cx.notify();
                            })
                        }))
                    })
                }
            });
        let sound = d.sound;
        let mut el = v_flex()
            .child(self.settings_section(t!("settings.desktop").into(), Some(t!("settings.desktop_hint").into())))
            .child(self.settings_choices(
                "settings-desktop",
                &d.desktop,
                ["all", "mention", "none"].iter().map(|k| (k.to_string(), level(k))).collect(),
                |d, v| d.desktop = v,
                cx,
            ));
        if self.crt {
            el = el.child(self.settings_toggle(
                "settings-threads",
                t!("settings.desktop_threads").into(),
                Some(t!("settings.desktop_threads_hint").into()),
                d.desktop_threads == "all",
                |d, v| d.desktop_threads = if v { "all" } else { "mention" }.into(),
                cx,
            ));
        } else {
            el = el.child(self.settings_section(t!("settings.comments").into(), None)).child(self.settings_choices(
                "settings-comments",
                &d.comments,
                vec![
                    ("any".into(), t!("settings.comments_any").into()),
                    ("root".into(), t!("settings.comments_root").into()),
                    ("never".into(), t!("settings.comments_never").into()),
                ],
                |d, v| d.comments = v,
                cx,
            ));
        }
        el.child(self.settings_toggle(
            "settings-sound-on",
            t!("settings.sound").into(),
            Some(t!("settings.sound_hint").into()),
            d.sound_on,
            |d, v| d.sound_on = v,
            cx,
        ))
        .child(
            h_flex().px_5().pb_2().gap_2().child(sound_picker).child(
                Button::new("settings-sound-test")
                    .icon(Icon::new(IconName::Volume2))
                    .label(t!("settings.sound_test"))
                    .small()
                    .ghost()
                    .disabled(!d.sound_on)
                    .on_click(move |_, _, _| crate::sound::play(sound)),
            ),
        )
        .child(self.settings_toggle(
            "settings-email",
            t!("settings.email").into(),
            Some(t!("settings.email_hint").into()),
            d.email,
            |d, v| d.email = v,
            cx,
        ))
        .child(self.settings_section(
            t!("settings.keywords").into(),
            Some(t!("settings.keywords_hint", username = username).into()),
        ))
        .children(first_name.map(|name| {
            self.settings_toggle(
                "settings-first-name",
                t!("settings.first_name", name = name).into(),
                None,
                d.first_name,
                |d, v| d.first_name = v,
                cx,
            )
        }))
        .child(self.settings_toggle(
            "settings-channel",
            t!("settings.channel_mentions").into(),
            None,
            d.channel_mentions,
            |d, v| d.channel_mentions = v,
            cx,
        ))
        .child(
            v_flex()
                .px_5()
                .pt_1()
                .gap_1()
                .child(div().text_sm().child(t!("settings.mention_keys")))
                .child(Input::new(&d.mention_keys).small()),
        )
        .child(
            v_flex()
                .px_5()
                .pt_3()
                .gap_1()
                .child(div().text_sm().child(t!("settings.highlight_keys")))
                .child(div().text_xs().text_color(p.muted()).child(t!("settings.highlight_keys_hint")))
                .child(Input::new(&d.highlight_keys).small()),
        )
        .child(self.settings_section(t!("settings.auto_reply").into(), Some(t!("settings.auto_reply_hint").into())))
        .child(self.settings_toggle(
            "settings-auto-reply",
            t!("settings.auto_reply_on").into(),
            None,
            d.auto_reply,
            |d, v| d.auto_reply = v,
            cx,
        ))
        .when(d.auto_reply, |el| {
            el.child(div().px_5().child(gpui_kit::component::input::Textarea::new(&d.auto_reply_text)))
        })
        .into_any_element()
    }

    fn render_settings_display(&self, d: &SettingsDraft, cx: &mut Context<Self>) -> AnyElement {
        let sample = self
            .me
            .as_ref()
            .map(|m| {
                let full = format!("{} {}", m.first_name, m.last_name).trim().to_string();
                (m.username.clone(), m.nickname.clone(), full)
            })
            .unwrap_or_default();
        let name_label = |format: &str| -> SharedString {
            let (user, nick, full) = &sample;
            let shown = match format {
                "username" => user.clone(),
                "nickname_full_name" if !nick.is_empty() => nick.clone(),
                _ if !full.is_empty() => full.clone(),
                _ => user.clone(),
            };
            let key = match format {
                "username" => t!("settings.name_username"),
                "nickname_full_name" => t!("settings.name_nickname"),
                _ => t!("settings.name_full"),
            };
            format!("{key} ({shown})").into()
        };
        let now = chrono::Local::now();
        v_flex()
            .when(self.crt_choice(), |el| {
                el.child(self.settings_toggle(
                    "settings-crt",
                    t!("settings.crt").into(),
                    Some(t!("settings.crt_hint").into()),
                    d.crt,
                    |d, v| d.crt = v,
                    cx,
                ))
            })
            .child(self.settings_section(t!("settings.clock").into(), None))
            .child(self.settings_choices(
                "settings-clock",
                if d.clock24 { "24" } else { "12" },
                vec![
                    ("24".into(), t!("settings.clock_24", example = now.format("%H:%M").to_string()).into()),
                    ("12".into(), t!("settings.clock_12", example = now.format("%-I:%M %p").to_string()).into()),
                ],
                |d, v| d.clock24 = v == "24",
                cx,
            ))
            .child(self.settings_section(t!("settings.names").into(), Some(t!("settings.names_hint").into())))
            .child(self.settings_choices(
                "settings-names",
                &d.name_format,
                NAME_FORMATS.iter().map(|f| (f.to_string(), name_label(f))).collect(),
                |d, v| d.name_format = v,
                cx,
            ))
            .into_any_element()
    }

    fn render_settings_sidebar(&self, d: &SettingsDraft, cx: &mut Context<Self>) -> AnyElement {
        let limit_label =
            |n: usize| -> SharedString { if n >= 10000 { t!("settings.dm_all").into() } else { n.to_string().into() } };
        v_flex()
            .child(self.settings_toggle(
                "settings-unread-section",
                t!("settings.unread_section").into(),
                Some(t!("settings.unread_section_hint").into()),
                d.unread_section,
                |d, v| d.unread_section = v,
                cx,
            ))
            .child(self.settings_section(t!("settings.dm_limit").into(), Some(t!("settings.dm_limit_hint").into())))
            .child(self.settings_choices(
                "settings-dm-limit",
                &d.dm_limit.to_string(),
                DM_LIMITS.iter().map(|n| (n.to_string(), limit_label(*n))).collect(),
                |d, v| d.dm_limit = v.parse().unwrap_or(40),
                cx,
            ))
            .into_any_element()
    }

    fn render_settings_advanced(&self, d: &SettingsDraft, cx: &mut Context<Self>) -> AnyElement {
        let ctrl = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" };
        v_flex()
            .child(self.settings_toggle(
                "settings-ctrl-enter",
                t!("settings.ctrl_enter", key = ctrl).into(),
                Some(t!("settings.ctrl_enter_hint", key = ctrl).into()),
                d.ctrl_enter,
                |d, v| d.ctrl_enter = v,
                cx,
            ))
            .when(!d.ctrl_enter, |el| {
                el.child(self.settings_toggle(
                    "settings-code-ctrl-enter",
                    t!("settings.code_ctrl_enter", key = ctrl).into(),
                    Some(t!("settings.code_ctrl_enter_hint").into()),
                    d.code_block_ctrl_enter,
                    |d, v| d.code_block_ctrl_enter = v,
                    cx,
                ))
            })
            .child(self.settings_toggle(
                "settings-formatting",
                t!("settings.formatting_bar").into(),
                Some(t!("settings.local_hint").into()),
                d.show_formatting,
                |d, v| d.show_formatting = v,
                cx,
            ))
            .child(self.settings_toggle(
                "settings-rich-editor",
                t!("settings.rich_editor").into(),
                Some(t!("settings.rich_editor_hint").into()),
                d.rich_editor,
                |d, v| d.rich_editor = v,
                cx,
            ))
            .child(self.settings_section(
                t!("update.section").into(),
                Some(t!("update.current", version = env!("CARGO_PKG_VERSION")).into()),
            ))
            .child(self.settings_toggle(
                "settings-check-updates",
                t!("update.auto_check").into(),
                Some(t!("update.auto_check_hint").into()),
                d.check_updates,
                |d, v| d.check_updates = v,
                cx,
            ))
            .child(
                h_flex().px_5().pt_1().child(
                    Button::new("settings-check-now")
                        .label(t!("update.check_now"))
                        .icon(Icon::new(IconName::RotateCcw))
                        .small()
                        .outline()
                        .on_click(cx.listener(|this, _, _, cx| this.check_for_update(true, cx))),
                ),
            )
            .child(self.settings_section(t!("mcp.section").into(), Some(t!("mcp.section_hint").into())))
            .child(self.settings_toggle(
                "settings-mcp",
                t!("mcp.enable").into(),
                Some(t!("mcp.enable_hint").into()),
                d.mcp_enabled,
                |d, v| d.mcp_enabled = v,
                cx,
            ))
            .children(self.render_mcp_details(cx))
            .into_any_element()
    }
}
