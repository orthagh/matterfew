//! Gestion : paramètres d'un canal (nom, adresse, privé, archivage), rôles
//! et retrait des membres, catégories (renommer, supprimer), aide des
//! raccourcis clavier et bandeau hors ligne.

use gpui_kit::component::input::InputState;

use super::dialogs::Dialog;
use super::*;
use crate::backend::Command;

/// Action à confirmer dans les paramètres d'un canal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ChannelDanger {
    MakePrivate,
    Archive,
}

/// Nom d'un canal dans les liens : minuscules, chiffres, tirets.
pub(crate) fn clean_channel_name(raw: &str) -> String {
    let folded = crate::fuzzy::fold(raw.trim());
    let mut out = String::new();
    for c in folded.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').chars().take(64).collect()
}

impl ChatApp {
    // ------------------------------------------------------------------
    // Paramètres du canal
    // ------------------------------------------------------------------

    pub(crate) fn open_channel_settings(&mut self, channel_id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.channels.get(&channel_id) else { return };
        let (display, name) = (state.channel.display_name.clone(), state.channel.name.clone());
        self.dialog = Some(Dialog::ChannelSettings { channel_id, confirm: None });
        self.dialog_comment.update(cx, |s: &mut InputState, cx| {
            s.set_placeholder(t!("channel_settings.url_placeholder"), window, cx);
            s.set_value(name, window, cx);
        });
        self.dialog_input.update(cx, |s: &mut InputState, cx| {
            s.set_placeholder(t!("channel_settings.name_placeholder"), window, cx);
            s.set_value(display, window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    pub(crate) fn save_channel_settings(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::ChannelSettings { channel_id, .. }) = self.dialog.as_ref() else { return };
        let channel_id = channel_id.clone();
        let Some(state) = self.channels.get(&channel_id) else { return };
        let display_name = self.dialog_input.read(cx).value().trim().to_string();
        let name = clean_channel_name(self.dialog_comment.read(cx).value().as_ref());
        if display_name.is_empty() || name.len() < 2 {
            self.flash(t!("channel_settings.invalid").to_string(), cx);
            return;
        }
        if display_name != state.channel.display_name || name != state.channel.name {
            self.send(Command::RenameChannel { channel_id, display_name, name });
        }
        self.dialog = None;
        cx.notify();
    }

    fn set_channel_danger(&mut self, danger: Option<ChannelDanger>, cx: &mut Context<Self>) {
        if let Some(Dialog::ChannelSettings { confirm, .. }) = self.dialog.as_mut() {
            *confirm = danger;
        }
        cx.notify();
    }

    pub(crate) fn run_channel_danger(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::ChannelSettings { channel_id, confirm: Some(danger) }) = self.dialog.as_ref() else { return };
        let channel_id = channel_id.clone();
        let archived = self.is_archived(&channel_id);
        match danger {
            ChannelDanger::MakePrivate => self.send(Command::MakeChannelPrivate { channel_id }),
            ChannelDanger::Archive => self.send(Command::ArchiveChannel { channel_id, archive: !archived }),
        }
        self.dialog = None;
        cx.notify();
    }

    pub(crate) fn render_channel_settings(
        &self,
        channel_id: &str,
        confirm: Option<ChannelDanger>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let Some(state) = self.channels.get(channel_id) else { return div().into_any_element() };
        let channel = &state.channel;
        let archived = channel.delete_at > 0;
        let public = channel.kind == "O";
        let server = self.server_url.clone().unwrap_or_default();
        let url = format!(
            "{server}/{}/channels/{}",
            self.team_name(),
            clean_channel_name(self.dialog_comment.read(cx).value().as_ref())
        );
        let label = |text: SharedString| div().text_xs().font_semibold().text_color(p.muted()).child(text);
        let danger_box = |title: SharedString, body: SharedString, button: AnyElement| {
            h_flex()
                .gap_3()
                .px_3()
                .py_2p5()
                .rounded(px(8.))
                .border_1()
                .border_color(p.border())
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_sm().font_semibold().child(title))
                        .child(div().text_xs().text_color(p.muted()).child(body)),
                )
                .child(button)
        };
        let confirm_row = confirm.map(|danger| {
            let text = match (danger, archived) {
                (ChannelDanger::MakePrivate, _) => t!("channel_settings.confirm_private"),
                (ChannelDanger::Archive, false) => t!("channel_settings.confirm_archive"),
                (ChannelDanger::Archive, true) => t!("channel_settings.confirm_unarchive"),
            };
            h_flex()
                .gap_2()
                .px_3()
                .py_2()
                .rounded(px(8.))
                .bg(p.error.opacity(0.08))
                .text_sm()
                .child(Icon::new(IconName::TriangleAlert).size(px(15.)).text_color(p.error))
                .child(div().flex_1().min_w_0().child(text))
                .child(
                    Button::new("danger-no")
                        .label(t!("common.cancel"))
                        .xsmall()
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| this.set_channel_danger(None, cx))),
                )
                .child(
                    Button::new("danger-yes")
                        .label(t!("common.confirm"))
                        .xsmall()
                        .danger()
                        .on_click(cx.listener(|this, _, _, cx| this.run_channel_danger(cx))),
                )
        });
        let content = v_flex()
            .px_4()
            .pb_4()
            .gap_2()
            .capture_action(cx.listener(|this, _: &Enter, _, cx| {
                this.save_channel_settings(cx);
                cx.stop_propagation();
            }))
            .child(label(t!("channel_settings.name").into()))
            .child(Input::new(&self.dialog_input))
            .child(div().h(px(4.)))
            .child(label(t!("channel_settings.url").into()))
            .child(Input::new(&self.dialog_comment).prefix(div().text_color(p.muted()).child("~")))
            .child(div().truncate().text_xs().text_color(p.muted()).child(url))
            .child(div().h(px(8.)))
            .when(public && !archived, |el| {
                el.child(danger_box(
                    t!("channel_settings.public").into(),
                    t!("channel_settings.public_hint").into(),
                    Button::new("make-private")
                        .icon(Icon::new(IconName::Lock))
                        .label(t!("channel_settings.make_private"))
                        .small()
                        .outline()
                        .on_click(
                            cx.listener(|this, _, _, cx| this.set_channel_danger(Some(ChannelDanger::MakePrivate), cx)),
                        )
                        .into_any_element(),
                ))
            })
            .child(danger_box(
                if archived { t!("channel_settings.archived") } else { t!("channel_settings.archive_title") }.into(),
                if archived { t!("channel_settings.unarchive_hint") } else { t!("channel_settings.archive_hint") }
                    .into(),
                Button::new("archive")
                    .icon(Icon::new(IconName::Archive))
                    .label(if archived { t!("channel_settings.unarchive") } else { t!("channel_settings.archive") })
                    .small()
                    .when(!archived, |b| b.danger())
                    .when(archived, |b| b.outline())
                    .on_click(cx.listener(|this, _, _, cx| this.set_channel_danger(Some(ChannelDanger::Archive), cx)))
                    .into_any_element(),
            ))
            .children(confirm_row)
            .into_any_element();
        let footer = h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(div().flex_1())
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
                    .on_click(cx.listener(|this, _, _, cx| this.save_channel_settings(cx))),
            )
            .into_any_element();
        let title = t!("channel_settings.title", channel = self.channel_title(channel));
        self.dialog_shell(IconName::Settings, title.into(), 520., content, Some(footer), cx)
    }

    // ------------------------------------------------------------------
    // Membres : rôles et retrait
    // ------------------------------------------------------------------

    /// On peut gérer les membres : administrateur du canal ou du système.
    pub(crate) fn can_manage_members(&self, channel_id: &str) -> bool {
        let me = self.me_id();
        let direct = self.channels.get(channel_id).is_none_or(|c| c.channel.is_direct() || c.channel.is_group());
        !direct
            && (self.me.as_ref().is_some_and(|u| u.is_system_admin())
                || self.channel_admins.get(channel_id).is_some_and(|a| a.iter().any(|id| id == me)))
    }

    /// Menu d'un membre (clic droit ou « ⋯ ») : rôle, retrait du canal.
    pub(crate) fn member_menu(
        &self,
        channel_id: &str,
        user_id: &str,
        cx: &mut Context<Self>,
    ) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
        let entity = cx.entity();
        let admin = self.channel_admins.get(channel_id).is_some_and(|a| a.iter().any(|id| id == user_id));
        let (channel_id, user_id) = (channel_id.to_string(), user_id.to_string());
        let name = self.user_name(&user_id);
        move |menu, _, _| {
            let (e1, c1, u1) = (entity.clone(), channel_id.clone(), user_id.clone());
            let (e2, c2, u2) = (entity.clone(), channel_id.clone(), user_id.clone());
            menu.min_w(px(240.))
                .label(name.clone())
                .item(
                    PopupMenuItem::new(if admin { t!("members.remove_admin") } else { t!("members.make_admin") })
                        .icon(Icon::new(IconName::Shield))
                        .on_click(move |_, _, cx| {
                            let (channel_id, user_id) = (c1.clone(), u1.clone());
                            e1.update(cx, |this, cx| this.set_member_admin(channel_id, user_id, !admin, cx))
                        }),
                )
                .item(PopupMenuItem::new(t!("members.remove")).icon(Icon::new(IconName::UserMinus)).on_click(
                    move |_, _, cx| {
                        let (channel_id, user_id) = (c2.clone(), u2.clone());
                        e2.update(cx, |this, cx| this.remove_member(channel_id, user_id, cx))
                    },
                ))
        }
    }

    fn set_member_admin(&mut self, channel_id: String, user_id: String, admin: bool, cx: &mut Context<Self>) {
        let admins = self.channel_admins.entry(channel_id.clone()).or_default();
        admins.retain(|id| *id != user_id);
        if admin {
            admins.push(user_id.clone());
        }
        self.send(Command::SetChannelAdmin { channel_id, user_id, admin });
        cx.notify();
    }

    pub(crate) fn remove_member(&mut self, channel_id: String, user_id: String, cx: &mut Context<Self>) {
        if let Some((ids, total)) = self.members.get_mut(&channel_id)
            && let Some(i) = ids.iter().position(|id| *id == user_id)
        {
            ids.remove(i);
            *total -= 1;
        }
        let name = self.user_name(&user_id);
        self.send(Command::RemoveChannelMember { channel_id, user_id });
        self.flash(t!("members.removed", name = name).to_string(), cx);
    }

    // ------------------------------------------------------------------
    // Catégories : renommer, supprimer
    // ------------------------------------------------------------------

    pub(crate) fn open_category_dialog(
        &mut self,
        category_id: String,
        delete: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(category) = self.categories.iter().find(|c| c.id == category_id) else { return };
        let name = category.display_name.clone();
        self.dialog = Some(Dialog::Category { category_id, confirm_delete: delete });
        self.dialog_input.update(cx, |s: &mut InputState, cx| {
            s.set_placeholder(t!("create.category_name"), window, cx);
            s.set_value(name, window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    pub(crate) fn rename_category(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Category { category_id, .. }) = self.dialog.as_ref() else { return };
        let category_id = category_id.clone();
        let name = self.dialog_input.read(cx).value().trim().to_string();
        let Some(team_id) = self.team_id.clone() else { return };
        if name.is_empty() {
            return;
        }
        if let Some(category) = self.categories.iter_mut().find(|c| c.id == category_id) {
            category.display_name = name;
            let categories = vec![category.clone()];
            self.send(Command::UpdateCategories { team_id, categories });
        }
        self.dialog = None;
        cx.notify();
    }

    /// Catégorie supprimée : ses canaux reviennent dans « Canaux ».
    pub(crate) fn delete_category(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Category { category_id, .. }) = self.dialog.as_ref() else { return };
        let category_id = category_id.clone();
        let Some(team_id) = self.team_id.clone() else { return };
        let Some(i) = self.categories.iter().position(|c| c.id == category_id) else { return };
        let removed = self.categories.remove(i);
        if let Some(channels) = self.categories.iter_mut().find(|c| c.kind == "channels") {
            channels.channel_ids.extend(removed.channel_ids);
        }
        self.send(Command::DeleteCategory { team_id, category_id });
        self.dialog = None;
        cx.notify();
    }

    pub(crate) fn render_category_dialog(
        &self,
        category_id: &str,
        confirm_delete: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let count = self.categories.iter().find(|c| c.id == category_id).map_or(0, |c| c.channel_ids.len());
        let content = v_flex()
            .px_4()
            .pb_4()
            .gap_2()
            .capture_action(cx.listener(|this, _: &Enter, _, cx| {
                this.rename_category(cx);
                cx.stop_propagation();
            }))
            .child(Input::new(&self.dialog_input))
            .when(confirm_delete, |el| {
                el.child(
                    h_flex()
                        .gap_2()
                        .px_3()
                        .py_2()
                        .rounded(px(8.))
                        .bg(p.error.opacity(0.08))
                        .text_sm()
                        .child(Icon::new(IconName::TriangleAlert).size(px(15.)).text_color(p.error))
                        .child(div().flex_1().child(t!("category_dialog.confirm_delete", count = count))),
                )
            })
            .into_any_element();
        let footer = h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(if confirm_delete {
                Button::new("cat-delete")
                    .icon(Icon::new(IconName::Trash))
                    .label(t!("category_dialog.delete"))
                    .small()
                    .danger()
                    .on_click(cx.listener(|this, _, _, cx| this.delete_category(cx)))
            } else {
                Button::new("cat-delete")
                    .icon(Icon::new(IconName::Trash))
                    .label(t!("category_dialog.delete"))
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(Dialog::Category { confirm_delete, .. }) = this.dialog.as_mut() {
                            *confirm_delete = true;
                        }
                        cx.notify();
                    }))
            })
            .child(div().flex_1())
            .child(
                Button::new("cat-cancel")
                    .label(t!("common.cancel"))
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.close_dialog(cx))),
            )
            .when(!confirm_delete, |el| {
                el.child(
                    Button::new("cat-save")
                        .label(t!("common.save"))
                        .small()
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| this.rename_category(cx))),
                )
            })
            .into_any_element();
        self.dialog_shell(IconName::FolderOpen, t!("category_dialog.title").into(), 420., content, Some(footer), cx)
    }

    // ------------------------------------------------------------------
    // Raccourcis clavier
    // ------------------------------------------------------------------

    pub(crate) fn open_shortcuts(&mut self, cx: &mut Context<Self>) {
        self.dialog = Some(Dialog::Shortcuts);
        cx.notify();
    }

    /// Maj+Échap : tous les canaux de l'équipe marqués comme lus.
    pub(crate) fn mark_team_read(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<String> = self
            .channels
            .values()
            .filter(|c| c.shows_unread() || c.unread_messages)
            .map(|c| c.channel.id.clone())
            .collect();
        if ids.is_empty() {
            return;
        }
        let count = ids.len();
        self.mark_channels_read(ids, cx);
        self.flash(t!("shortcuts.marked_read", count = count).to_string(), cx);
    }

    /// Alt+← / Alt+→ : canal précédent ou suivant de l'historique.
    pub(crate) fn navigate_history(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (from, to) = if forward {
            (&mut self.nav_forward, &mut self.nav_back)
        } else {
            (&mut self.nav_back, &mut self.nav_forward)
        };
        // Canaux quittés depuis (archivés, retirés) : ignorés.
        let Some(target) = std::iter::from_fn(|| from.pop()).find(|id| self.channels.contains_key(id)) else { return };
        if let Some(current) = self.channel_id.clone() {
            to.push(current);
        }
        self.nav_jumping = true;
        self.select_channel(target, cx);
        self.nav_jumping = false;
        self.focus_composer(window, cx);
    }

    /// Ctrl+Alt+↑ / ↓ : équipe précédente ou suivante.
    pub(crate) fn cycle_team(&mut self, down: bool, cx: &mut Context<Self>) {
        let n = self.teams.len();
        if n < 2 {
            return;
        }
        let i = self.teams.iter().position(|t| Some(&t.id) == self.team_id.as_ref()).unwrap_or(0);
        let next = if down { (i + 1) % n } else { (i + n - 1) % n };
        let id = self.teams[next].id.clone();
        self.select_team(id, cx);
    }

    /// Maj+↑ dans la zone de saisie vide : répondre au dernier message.
    pub(crate) fn reply_to_last(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.composer.read(cx).value().is_empty() {
            return false;
        }
        let Some(root) = self
            .posts
            .iter()
            .rev()
            .map(|v| &v.post)
            .find(|p| !p.is_system() && p.delete_at == 0)
            .map(|p| if p.root_id.is_empty() { p.id.clone() } else { p.root_id.clone() })
        else {
            return false;
        };
        self.open_thread(root, window, cx);
        true
    }

    pub(crate) fn render_shortcuts(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let groups: [(SharedString, Vec<(&str, SharedString)>); 3] = [
            (
                t!("shortcuts.navigation").into(),
                vec![
                    ("Ctrl K", t!("shortcuts.switcher").into()),
                    ("Ctrl Shift K", t!("shortcuts.new_message").into()),
                    ("Alt ↑/↓", t!("shortcuts.prev_next").into()),
                    ("Alt Shift ↑/↓", t!("shortcuts.prev_next_unread").into()),
                    ("Alt ←/→", t!("shortcuts.history").into()),
                    ("Ctrl Alt ↑/↓", t!("shortcuts.teams").into()),
                    ("Ctrl F", t!("shortcuts.search").into()),
                    ("Ctrl Shift M", t!("shortcuts.mentions").into()),
                    ("Ctrl Shift L", t!("shortcuts.focus").into()),
                    ("Shift Esc", t!("shortcuts.mark_all_read").into()),
                    ("Esc", t!("shortcuts.close").into()),
                ],
            ),
            (
                t!("shortcuts.messages").into(),
                vec![
                    ("↑", t!("shortcuts.edit_last").into()),
                    ("Shift ↑", t!("shortcuts.reply_last").into()),
                    ("Shift Enter", t!("shortcuts.newline").into()),
                    ("Ctrl Enter", t!("shortcuts.send_ctrl").into()),
                    ("Ctrl V", t!("shortcuts.paste").into()),
                ],
            ),
            (
                t!("shortcuts.formatting").into(),
                vec![
                    ("Ctrl B", t!("shortcuts.bold").into()),
                    ("Ctrl I", t!("shortcuts.italic").into()),
                    ("Ctrl Alt K", t!("shortcuts.link").into()),
                ],
            ),
        ];
        let keycap = |text: &str| {
            h_flex().gap_1().children(text.split(' ').map(|k| {
                div()
                    .px_1p5()
                    .min_w(px(22.))
                    .flex()
                    .justify_center()
                    .rounded(px(5.))
                    .border_1()
                    .border_b_2()
                    .border_color(p.border())
                    .bg(p.center_color.opacity(0.04))
                    .text_xs()
                    .font_semibold()
                    .child(match k {
                        "Shift" => t!("keys.shift").to_string(),
                        "Esc" => t!("keys.esc").to_string(),
                        "Enter" => t!("keys.enter").to_string(),
                        other => other.to_string(),
                    })
            }))
        };
        let content = v_flex()
            .id("shortcuts-list")
            .max_h(px(480.))
            .overflow_y_scroll()
            .px_4()
            .pb_4()
            .gap_3()
            .children(groups.into_iter().map(|(title, rows)| {
                v_flex().gap_1p5().child(div().text_xs().font_semibold().text_color(p.muted()).child(title)).children(
                    rows.into_iter().map(|(keys, what)| {
                        h_flex().gap_3().text_sm().child(div().flex_1().child(what)).child(keycap(keys))
                    }),
                )
            }))
            .into_any_element();
        self.dialog_shell(IconName::Keyboard, t!("shortcuts.title").into(), 480., content, None, cx)
    }

    // ------------------------------------------------------------------
    // Hors ligne
    // ------------------------------------------------------------------

    /// Bandeau en haut quand la connexion au serveur est perdue.
    pub(crate) fn render_offline_banner(&self) -> Option<AnyElement> {
        if self.connected {
            return None;
        }
        let p = self.palette;
        Some(
            h_flex()
                .flex_none()
                .px_5()
                .py_1p5()
                .gap_2()
                .bg(p.away.opacity(0.22))
                .text_sm()
                .child(Icon::new(IconName::WifiOff).size(px(15.)))
                .child(div().flex_1().child(t!("offline.banner")))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::clean_channel_name;

    #[test]
    fn noms_de_canal_dans_les_liens() {
        assert_eq!(clean_channel_name("  Équipe Été 2026 "), "equipe-ete-2026");
        assert_eq!(clean_channel_name("dev_backend--v2"), "dev_backend-v2");
    }
}
