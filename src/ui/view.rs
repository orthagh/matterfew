//! Rendu : écran de connexion, barre latérale, messages, fil, saisie.
//!
//! La mise en page suit celle du client web Mattermost et ses couleurs
//! viennent du thème de l'utilisateur (voir `theme.rs`).

use gpui_kit::assets::IconName;
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, StyledExt as _,
    avatar::Avatar,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Enter, Escape, IndentInline, Input, MoveDown, MoveUp, Textarea},
    message_scroller::MessageScroller,
    text::TextView,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use super::{ChatApp, Pane, Screen, SidebarResize};
use crate::api::{FileInfo, Post, User, human_size};
use crate::backend::group_reactions;
use crate::sidebar::{self, Section};
use crate::theme::Palette;
use crate::editor::Format;
use crate::rich::{self, Embed};
use crate::{emoji, markdown};

/// Réactions proposées dans la barre d'actions d'un message.
const QUICK_REACTIONS: [&str; 3] = ["+1", "heart", "joy"];

const TEAM_BAR_WIDTH: f32 = 64.;
const THREAD_WIDTH: f32 = 440.;
const AVATAR: f32 = 32.;
const BODY_TEXT: f32 = 14.5;

impl ChatApp {
    // ------------------------------------------------------------------
    // Éléments communs
    // ------------------------------------------------------------------

    fn status_of(&self, user_id: &str) -> &str {
        self.statuses.get(user_id).map(String::as_str).unwrap_or("offline")
    }

    fn status_color(&self, user_id: &str) -> Hsla {
        let p = &self.palette;
        match self.status_of(user_id) {
            "online" => p.online,
            "away" => p.away,
            "dnd" => p.dnd,
            _ => p.center_color.opacity(0.32),
        }
    }

    fn status_label(&self, user_id: &str) -> &'static str {
        match self.status_of(user_id) {
            "online" => "En ligne",
            "away" => "Absent",
            "dnd" => "Ne pas déranger",
            _ => "Hors ligne",
        }
    }

    /// Avatar avec pastille de présence ; `ring` = couleur du fond derrière.
    fn avatar(&self, user_id: &str, size: f32, ring: Hsla) -> impl IntoElement {
        let size = px(size);
        let mut avatar = Avatar::new().name(self.user_name(user_id)).size(size);
        if let Some(image) = self.avatars.get(user_id) {
            avatar = avatar.src(image.clone());
        }
        let dot = (size * 0.34).max(px(8.));
        div()
            .relative()
            .flex_none()
            .size(size)
            .child(avatar)
            .child(
                div()
                    .absolute()
                    .right(-px(2.))
                    .bottom(-px(2.))
                    .size(dot)
                    .rounded_full()
                    .border_2()
                    .border_color(ring)
                    .bg(self.status_color(user_id)),
            )
    }

    // ------------------------------------------------------------------
    // Connexion
    // ------------------------------------------------------------------

    fn render_login(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let connecting = self.screen == Screen::Connecting;
        let busy = self.login_busy;
        let label = |text: &'static str| div().text_sm().font_semibold().child(text);
        let separator = |text: &'static str| {
            h_flex()
                .gap_3()
                .py_1()
                .child(div().flex_1().h(px(1.)).bg(p.border()))
                .child(div().text_xs().text_color(p.muted()).child(text))
                .child(div().flex_1().h(px(1.)).bg(p.border()))
        };

        let token_section = if self.show_token {
            v_flex()
                .gap_2()
                .child(label("Jeton d'accès personnel"))
                .child(Input::new(&self.token_input).mask_toggle())
                .child(
                    Button::new("connect-token")
                        .outline()
                        .label("Se connecter avec le jeton")
                        .on_click(cx.listener(|this, _, window, cx| this.connect(window, cx))),
                )
                .into_any_element()
        } else {
            div()
                .id("show-token")
                .text_xs()
                .text_color(p.link)
                .cursor_pointer()
                .hover(|s| s.underline())
                .child("Utiliser plutôt un jeton d'accès personnel")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show_token = true;
                    cx.notify();
                }))
                .into_any_element()
        };

        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .bg(p.sidebar_bg)
            .child(
                v_flex()
                    .id("login-card")
                    .w(px(440.))
                    .max_h_full()
                    .overflow_y_scroll()
                    .gap_3()
                    .p_8()
                    .rounded(px(8.))
                    .bg(p.center_bg)
                    .text_color(p.center_color)
                    .shadow_lg()
                    .child(div().text_2xl().font_bold().child("Connexion à Mattermost"))
                    .child(label("Serveur"))
                    .child(Input::new(&self.url_input))
                    .child(
                        Button::new("login-browser")
                            .primary()
                            .label(if busy == Some("browser") {
                                "Terminez la connexion dans la fenêtre ouverte…"
                            } else {
                                "Se connecter via Mattermost"
                            })
                            .loading(busy == Some("browser") || connecting)
                            .on_click(cx.listener(|this, _, window, cx| this.login_browser(window, cx))),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(p.muted())
                            .child("Ouvre la page de connexion de votre serveur : GitLab, mot de passe, double authentification…"),
                    )
                    .child(separator("ou avec votre e-mail"))
                    .child(label("E-mail"))
                    .child(Input::new(&self.email_input))
                    .child(label("Mot de passe"))
                    .child(Input::new(&self.password_input).mask_toggle())
                    .when(self.show_mfa, |el| {
                        el.child(label("Code de double authentification")).child(Input::new(&self.mfa_input))
                    })
                    .child(
                        Button::new("login-password")
                            .outline()
                            .label("Se connecter")
                            .loading(busy == Some("password"))
                            .on_click(cx.listener(|this, _, window, cx| this.login_password(window, cx))),
                    )
                    .when_some(self.error.clone(), |el, err| {
                        el.child(div().text_sm().text_color(p.error).child(err))
                    })
                    .child(separator("avancé"))
                    .child(token_section)
                    .child(
                        div()
                            .text_xs()
                            .text_color(p.muted())
                            .child("La connexion est mémorisée dans le gestionnaire d'identifiants Windows."),
                    ),
            )
    }

    // ------------------------------------------------------------------
    // Barre des équipes et barre latérale
    // ------------------------------------------------------------------

    fn render_team_bar(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        if self.teams.len() < 2 {
            return None;
        }
        let p = self.palette;
        let current = self.team_id.clone();
        Some(
            v_flex()
                .w(px(TEAM_BAR_WIDTH))
                .h_full()
                .flex_none()
                .items_center()
                .gap_3()
                .pt_4()
                .bg(p.team_bar_bg)
                .children(self.teams.iter().map(|team| {
                    let active = current.as_deref() == Some(&team.id);
                    let id = team.id.clone();
                    let initials: String = team
                        .display_name
                        .split_whitespace()
                        .filter_map(|w| w.chars().next())
                        .take(2)
                        .collect::<String>()
                        .to_uppercase();
                    // Pastilles : rien pour l'équipe affichée (ses canaux le montrent).
                    let (unread, mentions) = if active {
                        (false, 0)
                    } else {
                        self.team_unreads.get(&team.id).copied().unwrap_or((false, 0))
                    };
                    let face: AnyElement = match self.team_icons.get(&team.id) {
                        Some(icon) => div()
                            .size_full()
                            .rounded(px(6.))
                            .bg(white())
                            .p(px(3.))
                            .child(img(icon.clone()).size_full().object_fit(ObjectFit::Contain))
                            .into_any_element(),
                        None => div()
                            .size_full()
                            .rounded(px(6.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(p.sidebar_text.opacity(if active { 0.24 } else { 0.12 }))
                            .text_color(p.sidebar_text)
                            .text_sm()
                            .font_bold()
                            .hover(|s| s.bg(p.sidebar_text.opacity(0.24)))
                            .child(initials)
                            .into_any_element(),
                    };
                    div()
                        .id(SharedString::from(format!("team-{}", team.id)))
                        .flex_none()
                        .relative()
                        .size(px(44.))
                        .p(px(3.))
                        .rounded(px(10.))
                        .border_2()
                        .border_color(if active { p.sidebar_text } else { transparent_black() })
                        .cursor_pointer()
                        .when(!active, |el| el.opacity(if unread || mentions > 0 { 1.0 } else { 0.72 }))
                        .child(face)
                        // Non lus : petite barre à gauche, comme Mattermost.
                        .when(unread && mentions == 0, |el| {
                            el.child(
                                div()
                                    .absolute()
                                    .left(-px(13.))
                                    .top(px(14.))
                                    .w(px(5.))
                                    .h(px(12.))
                                    .rounded_r(px(3.))
                                    .bg(p.sidebar_unread),
                            )
                        })
                        .when(mentions > 0, |el| {
                            el.child(
                                div()
                                    .absolute()
                                    .top(-px(7.))
                                    .right(-px(7.))
                                    .min_w(px(18.))
                                    .h(px(18.))
                                    .px_1()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .border_2()
                                    .border_color(p.team_bar_bg)
                                    .bg(p.mention_bg)
                                    .text_color(p.mention_color)
                                    .text_size(px(10.))
                                    .font_bold()
                                    .child(mentions.to_string()),
                            )
                        })
                        .tooltip({
                            let name: SharedString = team.display_name.clone().into();
                            move |window, cx| gpui_kit::component::tooltip::Tooltip::new(name.clone()).build(window, cx)
                        })
                        .on_click(cx.listener(move |this, _, _, cx| this.select_team(id.clone(), cx)))
                })),
        )
    }

    fn sidebar_sections(&self) -> Vec<Section> {
        sidebar::build(&sidebar::Input {
            channels: &self.channels,
            categories: &self.categories,
            prefs: &self.prefs,
            current: self.channel_id.as_deref(),
            sticky_unread: self.sticky_unread.as_deref(),
            title: |c| self.channel_title(c),
        })
    }

    fn render_channel_row(&self, channel_id: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.palette;
        let state = self.channels.get(channel_id)?;
        let channel = &state.channel;
        let selected = self.channel_id.as_deref() == Some(channel_id);
        let unread = state.shows_unread();

        let text_color = if selected {
            p.sidebar_active_text
        } else if unread {
            p.sidebar_unread
        } else {
            p.sidebar_text.opacity(if state.muted { 0.4 } else { 0.64 })
        };

        let icon: AnyElement = match channel.direct_partner(self.me_id()) {
            Some(partner) => self
                .avatar(&partner, 22., if selected { p.sidebar_hover } else { p.sidebar_bg })
                .into_any_element(),
            None => div()
                .w(px(22.))
                .flex_none()
                .flex()
                .justify_center()
                .child(
                    Icon::new(match channel.kind.as_str() {
                        "P" => IconName::Lock,
                        "G" => IconName::Users,
                        _ => IconName::Hash,
                    })
                    .size(px(15.))
                    .text_color(text_color.opacity(0.8)),
                )
                .into_any_element(),
        };

        let draft = !selected && self.has_draft(channel_id);
        let menu = self.channel_menu(channel_id, state.muted, cx);
        let id = channel_id.to_string();
        Some(
            h_flex()
                .id(SharedString::from(format!("ch-{channel_id}")))
                // Ne jamais rétrécir : avec beaucoup de canaux, la liste défile.
                .flex_none()
                .h(px(30.))
                .gap(px(10.))
                .pl(px(18.))
                .pr_3()
                .cursor_pointer()
                .text_size(px(15.))
                .text_color(text_color)
                .border_l_4()
                .border_color(if selected { p.sidebar_active_border } else { transparent_black() })
                .when(selected, |el| el.bg(p.sidebar_text.opacity(0.16)))
                .when(!selected, |el| el.hover(|s| s.bg(p.sidebar_hover)))
                .child(icon)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .when(unread, |el| el.font_semibold())
                        .child(self.channel_title(channel)),
                )
                .when(draft, |el| {
                    el.child(Icon::new(IconName::Pencil).size(px(13.)).text_color(p.sidebar_text.opacity(0.64)))
                })
                .when(state.mentions > 0, |el| {
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
                            .font_bold()
                            .bg(p.mention_bg)
                            .text_color(p.mention_color)
                            .child(state.mentions.to_string()),
                    )
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.select_channel(id.clone(), cx);
                    this.focus_composer(window, cx);
                }))
                // Clic droit : options du canal.
                .context_menu(menu)
                .into_any_element(),
        )
    }

    /// Options d'un canal (clic droit dans la barre latérale, « ⋯ » de l'en-tête).
    fn channel_menu(
        &self,
        channel_id: &str,
        muted: bool,
        cx: &mut Context<Self>,
    ) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
        let entity = cx.entity();
        let id = channel_id.to_string();
        let favorite = self.is_favorite(channel_id);
        let unread = self.channels.get(channel_id).is_some_and(|c| c.shows_unread());
        let direct = self.channels.get(channel_id).is_some_and(|c| c.channel.is_direct() || c.channel.is_group());
        move |menu, _, _| {
            let act = |label: &str, icon: IconName, f: fn(&mut ChatApp, String, &mut Context<ChatApp>)| {
                let (entity, id) = (entity.clone(), id.clone());
                PopupMenuItem::new(label.to_string())
                    .icon(Icon::new(icon))
                    .on_click(move |_, _, cx| entity.update(cx, |this, cx| f(this, id.clone(), cx)))
            };
            let mut menu = menu.min_w(px(220.));
            if unread {
                menu = menu.item(act("Marquer comme lu", IconName::CheckCheck, |this, id, cx| {
                    this.mark_channels_read(vec![id], cx)
                }));
            }
            menu = menu
                .item(act(
                    if favorite { "Retirer des favoris" } else { "Ajouter aux favoris" },
                    IconName::Star,
                    |this, id, cx| this.toggle_favorite(id, cx),
                ))
                .item(act(
                    if muted { "Réactiver les notifications" } else { "Mettre en sourdine" },
                    if muted { IconName::Bell } else { IconName::BellOff },
                    |this, id, cx| this.toggle_muted(id, cx),
                ))
                .item(act("Copier le lien", IconName::Link, |this, id, cx| this.copy_channel_link(&id, cx)));
            if !direct {
                menu = menu
                    .separator()
                    .item(act("Quitter le canal", IconName::LogOut, |this, id, cx| this.leave_channel(id, cx)));
            }
            menu
        }
    }

    fn render_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let team_name = self
            .team_id
            .as_ref()
            .and_then(|id| self.teams.iter().find(|t| &t.id == id))
            .map(|t| t.display_name.clone())
            .unwrap_or_default();

        let mut list = v_flex().id("channel-list").flex_1().min_h_0().overflow_y_scroll().pb_3();
        for section in self.sidebar_sections() {
            let category_id = section.id.clone();
            let group = SharedString::from(format!("catg-{}", section.id));
            let options = section.collapsible.then(|| self.category_options(&section.id, group.clone(), cx));
            let header = h_flex()
                .id(SharedString::from(format!("cat-{}", section.id)))
                .group(group)
                .flex_none()
                .h(px(32.))
                .mt_3()
                .gap_1()
                .pl_3()
                .pr_2()
                .text_size(px(13.))
                .font_semibold()
                .text_color(p.sidebar_text.opacity(0.64))
                .when(section.collapsible, |el| {
                    el.cursor_pointer()
                        .hover(|s| s.text_color(p.sidebar_text))
                        .child(
                            Icon::new(if section.collapsed { IconName::ChevronRight } else { IconName::ChevronDown })
                                .size(px(14.)),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| this.toggle_category(&category_id, cx)))
                })
                .when(!section.collapsible, |el| el.pl(px(30.)))
                .child(div().flex_1().child(section.title.clone()))
                .children(options);
            list = list.child(header);
            for channel_id in &section.channel_ids {
                if let Some(row) = self.render_channel_row(channel_id, cx) {
                    list = list.child(row);
                }
            }
        }

        let me_id = self.me_id().to_string();
        let me_name = self.me.as_ref().map(User::display_name).unwrap_or_default();
        let status = if self.connected { self.status_label(&me_id) } else { "Reconnexion…" };
        let account_menu = self.account_menu(cx);

        let resize_handle = div()
            .id("sidebar-resize")
            .absolute()
            .top_0()
            .right_0()
            .w(px(5.))
            .h_full()
            .cursor(CursorStyle::ResizeLeftRight)
            .when(self.resizing, |el| el.bg(p.sidebar_active_border))
            .hover(|s| s.bg(p.sidebar_active_border.opacity(0.7)))
            .on_drag(SidebarResize, |_, _, _, cx| cx.new(|_| SidebarResize));

        v_flex()
            .relative()
            .w(px(self.settings.sidebar_width))
            .h_full()
            .flex_none()
            .bg(p.sidebar_bg)
            .child(resize_handle)
            .text_color(p.sidebar_text)
            .child(
                h_flex()
                    .h(px(60.))
                    .flex_none()
                    .px_4()
                    .bg(p.sidebar_header_bg)
                    .text_color(p.sidebar_header_text)
                    .text_size(px(18.))
                    .font_bold()
                    .child(div().truncate().child(team_name))
                    .when(self.demo, |el| {
                        el.child(
                            div()
                                .ml_2()
                                .px_1p5()
                                .rounded(px(4.))
                                .bg(p.away)
                                .text_color(black())
                                .text_size(px(10.))
                                .child("DÉMO"),
                        )
                    }),
            )
            .child(list)
            .child(
                h_flex()
                    .flex_none()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_t_1()
                    .border_color(p.sidebar_text.opacity(0.12))
                    .child(self.avatar(&me_id, 32., p.sidebar_bg))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().truncate().text_sm().font_semibold().child(me_name))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(if self.connected { p.sidebar_text.opacity(0.64) } else { p.away })
                                    .child(status),
                            ),
                    )
                    .child(account_menu),
            )
    }

    /// « ⋯ » d'une catégorie : tri et « tout marquer comme lu ».
    fn category_options(&self, category_id: &str, group: SharedString, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity();
        let id = category_id.to_string();
        let category = self.categories.iter().find(|c| c.id == category_id);
        let sorting = category.map(|c| c.sorting.clone()).unwrap_or_default();
        let dm = category.is_some_and(|c| c.kind == "direct_messages");
        let p = self.palette;
        div()
            .invisible()
            .group_hover(group, |s| s.visible())
            .child(
                Button::new(SharedString::from(format!("catopt-{category_id}")))
                    .icon(Icon::new(IconName::Ellipsis).text_color(p.sidebar_text))
                    .ghost()
                    .xsmall()
                    .dropdown_menu(move |menu, _, _| {
                        let sort = |label: &str, value: &'static str, checked: bool| {
                            let (entity, id) = (entity.clone(), id.clone());
                            PopupMenuItem::new(label.to_string()).checked(checked).on_click(move |_, _, cx| {
                                entity.update(cx, |this, cx| this.set_category_sorting(&id, value, cx))
                            })
                        };
                        let alpha = sorting == "alpha" || sorting.is_empty();
                        let mut menu = menu
                            .min_w(px(240.))
                            .label("Trier")
                            .item(sort("Par ordre alphabétique", "alpha", alpha))
                            .item(sort("Par activité récente", "recent", sorting == "recent"));
                        if !dm {
                            menu = menu.item(sort("Ordre manuel", "manual", sorting == "manual"));
                        }
                        let (entity, id) = (entity.clone(), id.clone());
                        menu.separator().item(
                            PopupMenuItem::new("Marquer la catégorie comme lue")
                                .icon(Icon::new(IconName::CheckCheck))
                                .on_click(move |_, _, cx| entity.update(cx, |this, cx| this.mark_category_read(&id, cx))),
                        )
                    }),
            )
            .into_any_element()
    }

    /// Menu du compte : statut et déconnexion.
    fn account_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let current = self.status_of(self.me_id()).to_string();
        let p = self.palette;
        Button::new("account-menu")
            .icon(Icon::new(IconName::EllipsisVertical).text_color(p.sidebar_text))
            .ghost()
            .small()
            .tooltip("Statut et compte")
            .dropdown_menu(move |menu, _, _| {
                let status = |label: &str, value: &'static str, icon: IconName| {
                    let entity = entity.clone();
                    PopupMenuItem::new(label.to_string())
                        .icon(Icon::new(icon))
                        .checked(current == value)
                        .on_click(move |_, _, cx| entity.update(cx, |this, cx| this.set_status(value, cx)))
                };
                let entity = entity.clone();
                menu.min_w(px(220.))
                    .label("Statut")
                    .item(status("En ligne", "online", IconName::CircleCheck))
                    .item(status("Absent", "away", IconName::Moon))
                    .item(status("Ne pas déranger", "dnd", IconName::CircleMinus))
                    .item(status("Hors ligne", "offline", IconName::Circle))
                    .separator()
                    .item(PopupMenuItem::new("Se déconnecter").icon(Icon::new(IconName::LogOut)).on_click(
                        move |_, window, cx| entity.update(cx, |this, cx| this.logout(window, cx)),
                    ))
            })
    }

    // ------------------------------------------------------------------
    // Messages
    // ------------------------------------------------------------------

    fn mentions_me(&self, post: &Post) -> bool {
        let Some(me) = &self.me else { return false };
        let needle = format!("@{}", me.username);
        post.user_id != me.id
            && post.message.match_indices(&needle).any(|(i, _)| {
                // « @jean » ne doit pas correspondre à « @jean.dupont ».
                !post.message[i + needle.len()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-'))
            })
    }

    /// Une ligne de la liste des messages (canal ou fil).
    fn render_post(&self, entity: &Entity<Self>, pane: Pane, ix: usize, cx: &App) -> AnyElement {
        let list = match pane {
            Pane::Channel => &self.posts,
            Pane::Thread => match &self.thread {
                Some(t) => &t.posts,
                None => return div().into_any_element(),
            },
        };
        let Some(view) = list.get(ix) else {
            return div().into_any_element();
        };
        let p = self.palette;
        let post = &view.post;
        let me = self.me_id();
        let mine = post.user_id == me;
        let prev = ix.checked_sub(1).and_then(|i| list.get(i)).map(|v| &v.post);

        let new_day = prev.is_none_or(|prev| day_of(prev.create_at) != day_of(post.create_at));
        let first_new = pane == Pane::Channel && self.first_new_index() == Some(ix);
        // Messages successifs du même auteur (< 5 min, même jour) : en-tête masqué.
        let compact = !new_day
            && !first_new
            && prev.is_some_and(|prev| {
                prev.user_id == post.user_id
                    && post.create_at - prev.create_at < 5 * 60 * 1000
                    && !prev.is_system()
                    && !post.is_system()
            });

        let prefix = match pane {
            Pane::Channel => "c",
            Pane::Thread => "t",
        };
        let key = |what: &str| SharedString::from(format!("{prefix}-{what}-{}", post.id));
        let group = key("g");
        let being_edited = self.editing.as_ref().is_some_and(|(p, id)| *p == pane && *id == post.id);
        let confirming = self.confirm_delete.as_deref() == Some(post.id.as_str());
        let highlighted = self.mentions_me(post);

        let bot_name = rich::override_username(post);
        let header = h_flex()
            .gap_2()
            .items_baseline()
            .child(
                div()
                    .text_size(px(15.))
                    .font_semibold()
                    .child(bot_name.clone().unwrap_or_else(|| self.user_name(&post.user_id))),
            )
            .when(bot_name.is_some() || post.from_webhook(), |el| {
                el.child(
                    div()
                        .px_1()
                        .rounded(px(3.))
                        .bg(p.center_color.opacity(0.1))
                        .text_size(px(10.))
                        .font_semibold()
                        .text_color(p.muted())
                        .child("BOT"),
                )
            })
            .child(div().text_xs().text_color(p.muted()).child(format_time(post.create_at)))
            .when(post.is_pinned, |el| {
                el.child(
                    h_flex()
                        .gap_0p5()
                        .text_xs()
                        .text_color(p.link)
                        .child(Icon::new(IconName::Pin).size(px(12.)))
                        .child("Épinglé"),
                )
            })
            .when(self.is_saved(&post.id), |el| {
                el.child(
                    h_flex()
                        .gap_0p5()
                        .text_xs()
                        .text_color(p.away)
                        .child(Icon::new(IconName::Bookmark).size(px(12.)))
                        .child("Enregistré"),
                )
            });

        let body: AnyElement = if post.is_system() {
            div()
                .italic()
                .text_size(px(BODY_TEXT))
                .text_color(p.muted())
                .child(markdown::plain_text(&post.message))
                .into_any_element()
        } else if post.message.trim().is_empty() {
            div().into_any_element()
        } else {
            div()
                .text_size(px(BODY_TEXT))
                .child(
                    TextView::markdown(ElementId::Name(key("body")), view.md.clone())
                        .image_source(emoji_resolver(self.custom_emojis.clone()))
                        .on_link_click(|url, _, _, cx| {
                        if !url.starts_with(markdown::MENTION_SCHEME) {
                            cx.open_url(url);
                        }
                    }),
                )
                .when(post.edit_at > 0, |el| {
                    el.child(div().text_xs().text_color(p.muted()).child("(modifié)"))
                })
                .into_any_element()
        };

        // Réactions : cliquer sur une pastille ajoute ou retire la sienne.
        let reactions = group_reactions(&post.metadata.reactions, me);
        let reaction_row = h_flex().gap_1().mt_1().flex_wrap().children(reactions.into_iter().map(|(name, count, by_me)| {
            let entity = entity.clone();
            let (post_id, emoji_name) = (post.id.clone(), name.clone());
            h_flex()
                .id(SharedString::from(format!("{prefix}-rc-{name}-{}", post.id)))
                .h(px(24.))
                .gap_1()
                .px_2()
                .rounded(px(4.))
                .border_1()
                .cursor_pointer()
                .text_size(px(12.))
                .font_semibold()
                .when(by_me, |el| {
                    el.border_color(p.button_bg).bg(p.button_bg.opacity(0.08)).text_color(p.button_bg)
                })
                .when(!by_me, |el| {
                    el.border_color(transparent_black()).bg(p.center_color.opacity(0.08)).text_color(p.muted())
                })
                .hover(|s| s.border_color(p.center_color.opacity(0.24)))
                .child(match self.custom_emojis.get(&name) {
                    Some(image) => img(image.clone()).size(px(16.)).into_any_element(),
                    None => div().text_size(px(14.)).child(emoji::lookup(&name)).into_any_element(),
                })
                .child(count.to_string())
                .on_click(move |_, _, cx| {
                    entity.update(cx, |this, _| this.toggle_reaction(post_id.clone(), emoji_name.clone()));
                })
        }));

        let replies = (pane == Pane::Channel && post.reply_count > 0).then(|| {
            let entity = entity.clone();
            let root = post.id.clone();
            let label = if post.reply_count == 1 {
                "↩ 1 réponse".to_string()
            } else {
                format!("↩ {} réponses", post.reply_count)
            };
            div()
                .id(key("replies"))
                .mt_1()
                .text_size(px(12.))
                .font_semibold()
                .text_color(p.link)
                .cursor_pointer()
                .hover(|s| s.underline())
                .child(label)
                .on_click(move |_, window, cx| {
                    entity.update(cx, |this, cx| this.open_thread(root.clone(), window, cx));
                })
        });

        let toolbar = (!post.is_system())
            .then(|| self.render_post_toolbar(entity, pane, post.id.clone(), mine, confirming, group.clone(), cx));

        // Colonne de gauche : avatar, ou heure au survol des messages groupés.
        let left = div().w(px(AVATAR)).flex_none().map(|el| {
            if compact {
                el.pt(px(3.))
                    .text_size(px(10.))
                    .text_color(p.muted())
                    .invisible()
                    .group_hover(group.clone(), |s| s.visible())
                    .child(format_hour(post.create_at))
            } else {
                el.pt(px(2.)).child(self.avatar(&post.user_id, AVATAR, p.center_bg))
            }
        });

        let row = h_flex()
            .group(group)
            .relative()
            .items_start()
            .gap_3()
            .w_full()
            .px_5()
            .when(!compact, |el| el.pt_3())
            .pb_1()
            .when(highlighted, |el| el.bg(p.mention_highlight_bg.opacity(0.18)))
            .when(being_edited, |el| el.bg(p.away.opacity(0.12)))
            .when(!highlighted && !being_edited, |el| el.hover(|s| s.bg(p.hover())))
            .child(left)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .when(!compact, |el| el.child(header))
                    .child(body)
                    .children(self.render_rich(entity, post, &key("rich"), &p))
                    .when(!post.metadata.files.is_empty(), |el| {
                        el.child(self.render_files(entity, &post.metadata.files, &key("files"), &p))
                    })
                    .when(!post.metadata.reactions.is_empty(), |el| el.child(reaction_row))
                    .children(replies),
            )
            .children(toolbar);

        // Ligne « Nouveaux messages » avant le premier message non lu.
        if new_day || first_new {
            v_flex()
                .w_full()
                .when(new_day, |el| el.child(date_separator(post.create_at, &p)))
                .when(first_new, |el| el.child(new_messages_separator(&p)))
                .child(row)
                .into_any_element()
        } else {
            row.into_any_element()
        }
    }

    /// Barre d'actions affichée au survol d'un message.
    #[allow(clippy::too_many_arguments)]
    fn render_post_toolbar(
        &self,
        entity: &Entity<Self>,
        pane: Pane,
        post_id: String,
        mine: bool,
        confirming: bool,
        group: SharedString,
        cx: &App,
    ) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let id = |what: &str| SharedString::from(format!("{pane:?}-{what}-{post_id}"));

        // Dans la zone du message (et non au-dessus) : en allant vers la barre,
        // la souris reste sur le message et la barre ne saute pas ailleurs.
        let mut bar = h_flex()
            .absolute()
            .top(px(4.))
            .right_5()
            .gap_0p5()
            .p_0p5()
            .rounded(px(4.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .shadow_md();
        if !confirming {
            bar = bar.invisible().group_hover(group, |s| s.visible());
        }

        if confirming {
            let (e1, e2, pid) = (entity.clone(), entity.clone(), post_id.clone());
            return bar
                .child(div().px_2().text_xs().child("Supprimer ce message ?"))
                .child(
                    Button::new(id("del-yes"))
                        .label("Supprimer")
                        .xsmall()
                        .danger()
                        .on_click(move |_, _, cx| e1.update(cx, |this, cx| this.delete_post(pid.clone(), cx))),
                )
                .child(Button::new(id("del-no")).label("Annuler").xsmall().ghost().on_click(move |_, _, cx| {
                    e2.update(cx, |this, cx| {
                        this.confirm_delete = None;
                        cx.notify();
                    })
                }));
        }

        for name in QUICK_REACTIONS {
            let (entity, pid) = (entity.clone(), post_id.clone());
            bar = bar.child(
                Button::new(id(&format!("qr-{name}")))
                    .label(emoji::lookup(name))
                    .xsmall()
                    .ghost()
                    .tooltip(format!(":{name}:"))
                    .on_click(move |_, _, cx| {
                        entity.update(cx, |this, _| this.toggle_reaction(pid.clone(), name.to_string()));
                    }),
            );
        }
        bar = bar.child(div().w(px(1.)).h(px(18.)).mx_1().bg(theme.border));
        if pane == Pane::Channel {
            let (entity, pid) = (entity.clone(), post_id.clone());
            bar = bar.child(
                Button::new(id("reply"))
                    .icon(Icon::new(IconName::Reply))
                    .xsmall()
                    .ghost()
                    .tooltip("Répondre dans le fil")
                    .on_click(move |_, window, cx| entity.update(cx, |this, cx| this.open_thread(pid.clone(), window, cx))),
            );
        }

        // Toutes les options dans « ⋯ ».
        let saved = self.is_saved(&post_id);
        let pinned = self.find_post(&post_id).is_some_and(|p| p.is_pinned);
        let (entity, pid) = (entity.clone(), post_id.clone());
        bar.child(
            Button::new(id("more"))
                .icon(Icon::new(IconName::Ellipsis))
                .xsmall()
                .ghost()
                .tooltip("Plus d'actions")
                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                    let item = |label: &str, icon: IconName, f: fn(&mut ChatApp, String, &mut Window, &mut Context<ChatApp>)| {
                        let (entity, pid) = (entity.clone(), pid.clone());
                        PopupMenuItem::new(label.to_string())
                            .icon(Icon::new(icon))
                            .on_click(move |_, window, cx| entity.update(cx, |this, cx| f(this, pid.clone(), window, cx)))
                    };
                    let mut menu = menu.min_w(px(230.));
                    if pane == Pane::Channel {
                        menu = menu.item(item("Répondre", IconName::Reply, |this, id, window, cx| {
                            this.open_thread(id, window, cx)
                        }));
                    }
                    menu = menu
                        .item(item("Marquer comme non lu", IconName::MailOpen, |this, id, _, cx| {
                            this.mark_unread_from(id, cx)
                        }))
                        .item(item(
                            if saved { "Retirer des enregistrés" } else { "Enregistrer" },
                            IconName::Bookmark,
                            |this, id, _, cx| this.toggle_saved(id, cx),
                        ))
                        .item(item(
                            if pinned { "Désépingler du canal" } else { "Épingler au canal" },
                            IconName::Pin,
                            |this, id, _, _| this.toggle_pinned(id),
                        ))
                        .separator()
                        .item(item("Copier le lien", IconName::Link, |this, id, _, cx| this.copy_post_link(&id, cx)))
                        .item(item("Copier le texte", IconName::Copy, |this, id, _, cx| this.copy_post_text(&id, cx)));
                    if mine {
                        let (e, id) = (entity.clone(), pid.clone());
                        menu = menu
                            .separator()
                            .item(PopupMenuItem::new("Modifier").icon(Icon::new(IconName::Pencil)).on_click(
                                move |_, window, cx| e.update(cx, |this, cx| this.start_edit(pane, id.clone(), window, cx)),
                            ))
                            .item(item("Supprimer", IconName::Trash, |this, id, _, cx| {
                                this.confirm_delete = Some(id);
                                cx.notify();
                            }));
                    }
                    menu
                }),
        )
    }

    /// Zone de saisie encadrée, bouton Envoyer, bandeau de modification
    /// et indicateur « en train d'écrire ».
    fn render_composer(&self, pane: Pane, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let state = self.composer_for(pane);
        let editing = self.editing.as_ref().is_some_and(|(e, _)| *e == pane);
        let typing = match (pane, &self.channel_id, &self.thread) {
            (Pane::Channel, Some(ch), _) => self.typing_label(ch, ""),
            (Pane::Thread, Some(ch), Some(t)) => self.typing_label(ch, &t.root_id),
            _ => None,
        };

        let popup = self.render_autocomplete(pane, cx);
        let previewing = self.preview.contains(&pane);
        let text = state.read(cx).value().to_string();
        let format_button = |format: Format, icon: IconName, cx: &mut Context<Self>| {
            Button::new(SharedString::from(format!("fmt-{pane:?}-{format:?}")))
                .icon(Icon::new(icon))
                .ghost()
                .xsmall()
                .tooltip(format.label())
                .on_click(cx.listener(move |this, _, window, cx| this.format(pane, format, window, cx)))
        };
        let divider = || div().w(px(1.)).h(px(16.)).mx_1().bg(p.border());
        let toolbar = h_flex()
            .px_2()
            .pb_1p5()
            .gap_0p5()
            .child(format_button(Format::Bold, IconName::Bold, cx))
            .child(format_button(Format::Italic, IconName::Italic, cx))
            .child(format_button(Format::Strike, IconName::Strikethrough, cx))
            .child(divider())
            .child(format_button(Format::Heading, IconName::Heading, cx))
            .child(format_button(Format::Link, IconName::Link, cx))
            .child(format_button(Format::Code, IconName::Code, cx))
            .child(format_button(Format::CodeBlock, IconName::SquareCode, cx))
            .child(format_button(Format::Quote, IconName::TextQuote, cx))
            .child(format_button(Format::Bullet, IconName::List, cx))
            .child(format_button(Format::Numbered, IconName::ListOrdered, cx))
            .child(div().flex_1())
            .child(
                Button::new(SharedString::from(format!("attach-{pane:?}")))
                    .icon(Icon::new(IconName::Paperclip))
                    .ghost()
                    .xsmall()
                    .tooltip("Joindre des fichiers (ou glissez-les dans la fenêtre)")
                    .on_click(cx.listener(move |this, _, window, cx| this.pick_files(pane, window, cx))),
            )
            .child(
                Button::new(SharedString::from(format!("preview-{pane:?}")))
                    .icon(Icon::new(if previewing { IconName::EyeOff } else { IconName::Eye }))
                    .ghost()
                    .xsmall()
                    .toggled(previewing)
                    .tooltip(if previewing { "Revenir à l'écriture" } else { "Aperçu de la mise en forme" })
                    .on_click(cx.listener(move |this, _, window, cx| this.toggle_preview(pane, window, cx))),
            )
            .child(
                Button::new(SharedString::from(format!("send-{pane:?}")))
                    .icon(Icon::new(IconName::SendHorizontal))
                    .when(editing, |b| b.label("Enregistrer"))
                    .small()
                    .primary()
                    .ml_1()
                    .tooltip(if editing { "Enregistrer (Entrée)" } else { "Envoyer (Entrée) — Maj+Entrée pour aller à la ligne" })
                    .on_click(cx.listener(move |this, _, window, cx| this.send_from(pane, window, cx))),
            );
        let input: AnyElement = if previewing {
            let prepared = markdown::prepare(&text);
            div()
                .px_3()
                .pt_2()
                .pb_1()
                .min_h(px(44.))
                .text_size(px(BODY_TEXT))
                .child(if text.trim().is_empty() {
                    div().text_color(p.muted()).child("Rien à prévisualiser.").into_any_element()
                } else {
                    TextView::markdown(SharedString::from(format!("preview-text-{pane:?}")), prepared).into_any_element()
                })
                .into_any_element()
        } else {
            div().px_1().pt_1().child(Textarea::new(&state).appearance(false)).into_any_element()
        };

        v_flex()
            .id(SharedString::from(format!("composer-{pane:?}")))
            .relative()
            .flex_none()
            .px_5()
            .pb_1()
            .pt_1()
            // Raccourcis de mise en forme (non utilisés par la zone de saisie).
            .capture_key_down(cx.listener(move |this, ev: &KeyDownEvent, window, cx| {
                let k = &ev.keystroke;
                let m = &k.modifiers;
                let format = match (m.control, m.shift, m.alt, k.key.as_str()) {
                    (true, false, false, "b") => Some(Format::Bold),
                    (true, false, false, "i") => Some(Format::Italic),
                    (true, true, false, "x") => Some(Format::Strike),
                    (true, false, true, "k") => Some(Format::Link),
                    (true, false, true, "c") => Some(Format::Code),
                    _ => None,
                };
                if let Some(format) = format {
                    this.format(pane, format, window, cx);
                    cx.stop_propagation();
                }
            }))
            // Avant la zone de saisie (phase de capture) : quand des propositions
            // sont affichées, ↑ ↓ Entrée Tab Échap servent à les parcourir.
            .capture_action(cx.listener(move |this, _: &MoveUp, window, cx| this.ac_action(pane, "up", window, cx)))
            .capture_action(cx.listener(move |this, _: &MoveDown, window, cx| this.ac_action(pane, "down", window, cx)))
            .capture_action(cx.listener(move |this, _: &Enter, window, cx| this.ac_action(pane, "enter", window, cx)))
            .capture_action(cx.listener(move |this, _: &IndentInline, window, cx| this.ac_action(pane, "tab", window, cx)))
            .capture_action(cx.listener(move |this, _: &Escape, window, cx| this.ac_action(pane, "escape", window, cx)))
            .children(popup)
            .child(
                v_flex()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(p.center_color.opacity(0.16))
                    .bg(p.center_bg)
                    .shadow_sm()
                    .when(editing, |el| {
                        el.child(
                            h_flex()
                                .gap_2()
                                .px_3()
                                .py_1p5()
                                .rounded_t(px(8.))
                                .bg(p.away.opacity(0.16))
                                .text_xs()
                                .child(div().flex_1().font_semibold().child("Modification du message"))
                                .child(
                                    Button::new(SharedString::from(format!("cancel-edit-{pane:?}")))
                                        .label("Annuler")
                                        .xsmall()
                                        .ghost()
                                        .on_click(cx.listener(|this, _, window, cx| this.cancel_edit(window, cx))),
                                ),
                        )
                    })
                    .children(self.render_attachments(pane, cx))
                    .child(input)
                    .child(toolbar),
            )
            .child(
                div()
                    .h(px(18.))
                    .px_1()
                    .pt_0p5()
                    .text_size(px(11.))
                    .italic()
                    .text_color(p.muted())
                    .children(typing),
            )
    }

    /// Image externe déjà chargée, dans une boîte de taille fixe (en attendant :
    /// boîte vide de la même taille, pour que la ligne ne saute pas).
    fn external_image(&self, url: &str, w: f32, h: f32, p: &Palette) -> AnyElement {
        match self.external_images.get(url) {
            Some(image) => img(image.clone())
                .w(px(w))
                .h(px(h))
                .object_fit(ObjectFit::Contain)
                .rounded(px(4.))
                .into_any_element(),
            None => div().w(px(w)).h(px(h)).rounded(px(4.)).bg(p.center_color.opacity(0.05)).into_any_element(),
        }
    }

    /// Pièces jointes des intégrations et aperçus de liens d'un message.
    fn render_rich(&self, entity: &Entity<Self>, post: &Post, key: &SharedString, p: &Palette) -> Vec<AnyElement> {
        let p = *p;
        let mut out = Vec::new();
        let open = |url: String| move |_: &ClickEvent, _: &mut Window, cx: &mut App| cx.open_url(&url);
        let link_click = |url: &SharedString, _: &ClickEvent, _: &mut Window, cx: &mut App| {
            if !url.starts_with(markdown::MENTION_SCHEME) {
                cx.open_url(url);
            }
        };

        for (i, a) in rich::attachments(post).into_iter().enumerate() {
            let id = |what: &str| SharedString::from(format!("{key}-a{i}-{what}"));
            let bar = rich::attachment_color(&a.color)
                .and_then(crate::theme::parse_color)
                .unwrap_or(p.border());

            let mut card = v_flex().flex_1().min_w_0().px_3().py_2().gap_1();
            if !a.author_name.is_empty() {
                card = card.child(div().text_xs().font_semibold().text_color(p.muted()).child(a.author_name.clone()));
            }
            if !a.title.is_empty() {
                let title = div().id(id("title")).font_semibold().child(a.title.clone());
                card = card.child(if a.title_link.is_empty() {
                    title.into_any_element()
                } else {
                    title
                        .text_color(p.link)
                        .cursor_pointer()
                        .hover(|s| s.underline())
                        .on_click(open(a.title_link.clone()))
                        .into_any_element()
                });
            }
            if !a.text.is_empty() {
                card = card.child(
                    div().text_size(px(BODY_TEXT)).child(
                        TextView::markdown(ElementId::Name(id("text")), markdown::prepare(&a.text)).on_link_click(link_click),
                    ),
                );
            }
            // Champs : les « courts » vont par deux sur une ligne.
            let mut fields = v_flex().gap_1();
            let mut pending_short: Option<rich::Field> = None;
            let field = |f: &rich::Field| {
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_xs().font_semibold().child(f.title.clone()))
                    .child(div().text_sm().child(emoji::replace_shortcodes(&f.value)))
            };
            for f in &a.fields {
                match (f.short, pending_short.take()) {
                    (true, None) => pending_short = Some(f.clone()),
                    (true, Some(prev)) => fields = fields.child(h_flex().gap_4().child(field(&prev)).child(field(f))),
                    (false, prev) => {
                        if let Some(prev) = prev {
                            fields = fields.child(h_flex().child(field(&prev)));
                        }
                        fields = fields.child(field(f));
                    }
                }
            }
            if let Some(prev) = pending_short {
                fields = fields.child(h_flex().child(field(&prev)));
            }
            if !a.fields.is_empty() {
                card = card.child(fields.mt_1());
            }
            if !a.image_url.is_empty() {
                card = card.child(self.external_image(&a.image_url, 400., 240., &p));
            }
            if !a.footer.is_empty() {
                card = card.child(div().text_xs().text_color(p.muted()).child(a.footer.clone()));
            }
            let buttons: Vec<_> = a
                .actions
                .iter()
                .filter(|act| act.kind.is_empty() || act.kind == "button")
                .map(|act| {
                    let (entity, post_id, action_id) = (entity.clone(), post.id.clone(), act.id.clone());
                    let button = Button::new(SharedString::from(format!("{key}-a{i}-act-{}", act.id)))
                        .label(act.name.clone())
                        .small();
                    let button = match act.style.as_str() {
                        "primary" => button.primary(),
                        "danger" => button.danger(),
                        "success" | "good" => button.success(),
                        _ => button.outline(),
                    };
                    button.on_click(move |_, _, cx| {
                        entity.update(cx, |this, _| this.post_action(post_id.clone(), action_id.clone()));
                    })
                })
                .collect();
            if !buttons.is_empty() {
                card = card.child(h_flex().gap_2().mt_1().flex_wrap().children(buttons));
            }

            let mut block = v_flex().gap_1().mt_1();
            if !a.pretext.is_empty() {
                block = block.child(
                    div().text_size(px(BODY_TEXT)).child(
                        TextView::markdown(ElementId::Name(id("pretext")), markdown::prepare(&a.pretext))
                            .on_link_click(link_click),
                    ),
                );
            }
            let thumb = (!a.thumb_url.is_empty()).then(|| div().p_2().child(self.external_image(&a.thumb_url, 64., 64., &p)));
            out.push(
                block
                    .child(
                        h_flex()
                            .max_w(px(680.))
                            .items_start()
                            .rounded(px(4.))
                            .border_1()
                            .border_color(p.border())
                            .overflow_hidden()
                            .child(div().w(px(4.)).flex_none().self_stretch().bg(bar))
                            .child(card)
                            .children(thumb),
                    )
                    .into_any_element(),
            );
        }

        for (i, embed) in rich::embeds(post).into_iter().enumerate() {
            let id = SharedString::from(format!("{key}-e{i}"));
            let element = match embed {
                Embed::Link { url, site, title, description, image } => h_flex()
                    .id(id)
                    .mt_1()
                    .max_w(px(560.))
                    .gap_3()
                    .pl_3()
                    .pr_2()
                    .py_2()
                    .border_l_4()
                    .border_color(p.border())
                    .rounded_r(px(4.))
                    .bg(p.center_color.opacity(0.03))
                    .cursor_pointer()
                    .hover(|s| s.bg(p.hover()))
                    .on_click(open(url))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_0p5()
                            .when(!site.is_empty(), |el| el.child(div().text_xs().text_color(p.muted()).child(site)))
                            .when(!title.is_empty(), |el| {
                                el.child(div().font_semibold().text_color(p.link).child(title))
                            })
                            .when(!description.is_empty(), |el| {
                                el.child(div().text_sm().text_color(p.muted()).line_clamp(3).child(description))
                            }),
                    )
                    .children(image.map(|u| self.external_image(&u, 80., 80., &p)))
                    .into_any_element(),
                Embed::Image { url, width, height } => {
                    let (w, h) = image_box(width, height);
                    div()
                        .id(id)
                        .mt_1()
                        .cursor_pointer()
                        .on_click(open(url.clone()))
                        .child(self.external_image(&url, w, h, &p))
                        .into_any_element()
                }
                Embed::Permalink { channel, message, user_id } => v_flex()
                    .mt_1()
                    .max_w(px(560.))
                    .px_3()
                    .py_2()
                    .gap_1()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(p.border())
                    .child(
                        div()
                            .text_xs()
                            .text_color(p.muted())
                            .child(format!("Message de {} dans {}", self.user_name(&user_id), channel)),
                    )
                    .child(div().text_sm().line_clamp(4).child(markdown::plain_text(&message)))
                    .into_any_element(),
            };
            out.push(element);
        }
        out
    }

    /// Pièces jointes d'un message : aperçus pour les images, cartes pour
    /// les autres fichiers.
    fn render_files(&self, entity: &Entity<Self>, files: &[FileInfo], key: &SharedString, p: &Palette) -> impl IntoElement {
        let p = *p;
        let mut images = h_flex().gap_2().flex_wrap();
        let mut others = v_flex().gap_1p5();
        for (ix, file) in files.iter().enumerate() {
            let id = SharedString::from(format!("{key}-{ix}"));
            if file.is_image() {
                let (w, h) = image_box(file.width, file.height);
                let preview: AnyElement = match self.previews.get(&file.id) {
                    Some(image) => img(image.clone()).size_full().object_fit(ObjectFit::Cover).into_any_element(),
                    None => div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .text_color(p.muted())
                        .child("Chargement de l'aperçu…")
                        .into_any_element(),
                };
                let (entity, file) = (entity.clone(), file.clone());
                images = images.child(
                    div()
                        .id(id)
                        .w(px(w))
                        .h(px(h))
                        .rounded(px(4.))
                        .overflow_hidden()
                        .border_1()
                        .border_color(p.border())
                        .bg(p.center_color.opacity(0.04))
                        .cursor_pointer()
                        .child(preview)
                        .on_click(move |_, window, cx| {
                            let file = file.clone();
                            entity.update(cx, |this, cx| this.open_image(file, window, cx));
                        }),
                );
            } else {
                let (e1, e2) = (entity.clone(), entity.clone());
                let (f1, f2) = (file.clone(), file.clone());
                let kind = if file.extension.is_empty() { "Fichier".to_string() } else { file.extension.to_uppercase() };
                others = others.child(
                    h_flex()
                        .id(id)
                        .w(px(360.))
                        .max_w_full()
                        .gap_3()
                        .px_3()
                        .py_2()
                        .rounded(px(4.))
                        .border_1()
                        .border_color(p.border())
                        .hover(|s| s.bg(p.hover()))
                        .child(div().text_size(px(26.)).child(file_icon(&file.extension)))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(div().truncate().text_sm().font_semibold().child(file.name.clone()))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(p.muted())
                                        .child(format!("{kind} · {}", file.human_size())),
                                ),
                        )
                        .child(
                            Button::new(SharedString::from(format!("{key}-{ix}-open")))
                                .label("Ouvrir")
                                .xsmall()
                                .ghost()
                                .on_click(move |_, _, cx| {
                                    let file = f1.clone();
                                    e1.update(cx, |this, cx| this.download(file, true, cx));
                                }),
                        )
                        .child(
                            Button::new(SharedString::from(format!("{key}-{ix}-dl")))
                                .label("Télécharger")
                                .xsmall()
                                .outline()
                                .on_click(move |_, _, cx| {
                                    let file = f2.clone();
                                    e2.update(cx, |this, cx| this.download(file, false, cx));
                                }),
                        ),
                );
            }
        }
        v_flex().gap_2().mt_1p5().child(images).child(others)
    }

    /// Fichiers en attente au-dessus de la zone de saisie.
    fn render_attachments(&self, pane: Pane, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let p = self.palette;
        let list: Vec<&super::Attachment> = self.attachments.iter().filter(|a| a.pane == pane).collect();
        if list.is_empty() {
            return None;
        }
        Some(h_flex().flex_wrap().gap_2().px_3().pt_2().children(list.into_iter().map(|a| {
            let local_id = a.local_id;
            let status = match (&a.file_id, &a.error) {
                (_, Some(err)) => div().text_color(p.error).child(format!("échec : {err}")),
                (Some(_), None) => div().text_color(p.muted()).child(human_size(a.size)),
                (None, None) => div().text_color(p.muted()).child("envoi…"),
            };
            h_flex()
                .gap_2()
                .max_w(px(300.))
                .pl_2()
                .pr_1()
                .py_1()
                .rounded(px(4.))
                .border_1()
                .border_color(if a.error.is_some() { p.error } else { p.border() })
                .bg(p.center_color.opacity(0.04))
                .text_xs()
                .child(div().text_size(px(16.)).child(file_icon(a.name.rsplit_once('.').map(|(_, e)| e).unwrap_or(""))))
                .child(
                    v_flex()
                        .min_w_0()
                        .child(div().truncate().font_semibold().child(a.name.clone()))
                        .child(status),
                )
                .child(
                    Button::new(SharedString::from(format!("rm-att-{local_id}")))
                        .icon(Icon::new(IconName::X))
                        .xsmall()
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| this.remove_attachment(local_id, cx))),
                )
        })))
    }

    /// Action clavier de la zone de saisie : prise par l'autocomplétion si
    /// des propositions sont affichées, sinon laissée à la zone de saisie.
    fn ac_action(&mut self, pane: Pane, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.autocomplete_key(pane, key, window, cx) {
            cx.stop_propagation();
        }
    }

    /// Propositions d'autocomplétion, au-dessus de la zone de saisie.
    fn render_autocomplete(&self, pane: Pane, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ac = self.autocomplete.as_ref().filter(|a| a.pane == pane && !a.items.is_empty())?;
        let p = self.palette;
        let title = match ac.kind {
            crate::backend::AcKind::User => "Personnes",
            crate::backend::AcKind::Channel => "Canaux",
            crate::backend::AcKind::Emoji => "Emojis",
            crate::backend::AcKind::Command => "Commandes",
        };
        let rows = ac.items.iter().enumerate().map(|(i, item)| {
            let selected = i == ac.selected;
            let icon: AnyElement = match (&item.user_id, &item.glyph) {
                (Some(user_id), _) => self.avatar(user_id, 22., p.center_bg).into_any_element(),
                (_, Some(glyph)) => match self.custom_emojis.get(glyph) {
                    Some(image) => img(image.clone()).size(px(20.)).into_any_element(),
                    None => div().w(px(22.)).text_size(px(18.)).child(glyph.clone()).into_any_element(),
                },
                _ => div()
                    .w(px(22.))
                    .text_center()
                    .text_color(p.muted())
                    .child(if item.label.starts_with('~') { "#" } else { "/" })
                    .into_any_element(),
            };
            h_flex()
                .id(SharedString::from(format!("ac-{pane:?}-{i}")))
                .gap_2()
                .px_3()
                .py_1()
                .cursor_pointer()
                .when(selected, |el| el.bg(p.button_bg.opacity(0.14)))
                .hover(|s| s.bg(p.button_bg.opacity(0.08)))
                .child(icon)
                .child(div().flex_none().font_semibold().child(item.label.clone()))
                .child(div().flex_1().min_w_0().truncate().text_sm().text_color(p.muted()).child(item.detail.clone()))
                .on_click(cx.listener(move |this, _, window, cx| this.accept_suggestion(i, window, cx)))
        });
        Some(
            v_flex()
                .absolute()
                .left_5()
                .right_5()
                .bottom(relative(1.))
                .max_h(px(320.))
                .py_1()
                .rounded(px(6.))
                .border_1()
                .border_color(p.border())
                .bg(p.center_bg)
                .shadow_lg()
                .child(
                    h_flex()
                        .px_3()
                        .pb_1()
                        .text_xs()
                        .font_semibold()
                        .text_color(p.muted())
                        .child(div().flex_1().child(title)),
                )
                .children(rows)
                .into_any_element(),
        )
    }

    fn render_channel_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let state = self.channel_id.as_ref().and_then(|id| self.channels.get(id));
        let title = state.map(|c| self.channel_title(&c.channel)).unwrap_or_default();
        let partner = state.and_then(|c| c.channel.direct_partner(self.me_id()));
        let subtitle = match (&partner, state) {
            (Some(partner), _) => Some(self.status_label(partner).to_string()),
            (None, Some(c)) if !c.channel.purpose.is_empty() => Some(c.channel.purpose.clone()),
            (None, Some(c)) if !c.channel.header.is_empty() => Some(markdown::plain_text(&c.channel.header)),
            _ => None,
        };
        let icon = state.and_then(|c| match c.channel.kind.as_str() {
            "O" => Some(IconName::Hash),
            "P" => Some(IconName::Lock),
            _ => None,
        });
        let actions = self.channel_id.clone().zip(state).map(|(channel_id, state)| {
            let favorite = self.is_favorite(&channel_id);
            let (fav_id, mute_id) = (channel_id.clone(), channel_id.clone());
            h_flex()
                .gap_1()
                .child(
                    Button::new("hdr-favorite")
                        .icon(Icon::new(IconName::Star).when(favorite, |i| i.text_color(p.away)))
                        .ghost()
                        .small()
                        .tooltip(if favorite { "Retirer des favoris" } else { "Ajouter aux favoris" })
                        .on_click(cx.listener(move |this, _, _, cx| this.toggle_favorite(fav_id.clone(), cx))),
                )
                .child(
                    Button::new("hdr-mute")
                        .icon(Icon::new(if state.muted { IconName::BellOff } else { IconName::Bell }))
                        .ghost()
                        .small()
                        .tooltip(if state.muted { "Réactiver les notifications" } else { "Mettre en sourdine" })
                        .on_click(cx.listener(move |this, _, _, cx| this.toggle_muted(mute_id.clone(), cx))),
                )
                .child(
                    Button::new("hdr-more")
                        .icon(Icon::new(IconName::Ellipsis))
                        .ghost()
                        .small()
                        .tooltip("Options du canal")
                        .dropdown_menu_with_anchor(Anchor::TopRight, self.channel_menu(&channel_id, state.muted, cx)),
                )
        });

        h_flex()
            .h(px(62.))
            .flex_none()
            .px_5()
            .gap_3()
            .border_b_1()
            .border_color(p.border())
            .when_some(partner.clone(), |el, partner| el.child(self.avatar(&partner, 32., p.center_bg)))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        h_flex()
                            .gap_1()
                            .text_size(px(16.))
                            .font_bold()
                            .when_some(icon, |el, icon| {
                                el.child(Icon::new(icon).size(px(16.)).text_color(p.muted()))
                            })
                            .child(title),
                    )
                    .when_some(subtitle, |el, sub| {
                        el.child(div().truncate().text_size(px(12.)).text_color(p.muted()).child(sub))
                    }),
            )
            .when(self.loading_older, |el| {
                el.child(div().text_xs().text_color(p.muted()).child("Chargement de l'historique…"))
            })
            .children(actions)
    }

    fn render_chat(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let entity = cx.entity();

        let error_banner = self.error.clone().map(|err| {
            h_flex()
                .px_5()
                .py_2()
                .gap_2()
                .bg(p.error.opacity(0.12))
                .text_sm()
                .text_color(p.error)
                .child(div().flex_1().child(err))
                .child(
                    Button::new("dismiss-error")
                        .icon(Icon::new(IconName::X))
                        .xsmall()
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.error = None;
                            cx.notify();
                        })),
                )
        });

        let messages: AnyElement = if self.loading_posts && self.posts.is_empty() {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(p.muted())
                .child("Chargement…")
                .into_any_element()
        } else {
            let entity = entity.clone();
            div()
                .flex_1()
                .min_h_0()
                .child(
                    MessageScroller::new("messages", self.scroller.clone(), move |ix, _, cx| {
                        entity.read(cx).render_post(&entity, Pane::Channel, ix, cx)
                    })
                    .with_jump_button_label("Derniers messages")
                    // Pas d'espace imposé entre les lignes : chaque message gère le sien.
                    .with_row_style(StyleRefinement::default().pb(px(0.))),
                )
                .into_any_element()
        };

        let info_banner = self.info.clone().map(|info| {
            h_flex()
                .px_5()
                .py_2()
                .gap_2()
                .bg(p.button_bg.opacity(0.08))
                .text_sm()
                .child(div().flex_1().child(info))
                .child(
                    Button::new("dismiss-info")
                        .icon(Icon::new(IconName::X))
                        .xsmall()
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.info = None;
                            cx.notify();
                        })),
                )
        });

        let main = v_flex()
            .id("center")
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(p.center_bg)
            .text_color(p.center_color)
            // Fichiers glissés depuis l'explorateur : joints au message.
            .drag_over::<ExternalPaths>(move |style, _, _, _| style.bg(p.button_bg.opacity(0.06)))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.add_files(Pane::Channel, paths.paths().to_vec(), cx);
            }))
            .child(self.render_channel_header(cx))
            .children(error_banner)
            .children(info_banner)
            .child(messages)
            .child(self.render_composer(Pane::Channel, cx));

        let thread_panel = self.thread.is_some().then(|| {
            let entity = entity.clone();
            let channel = self
                .channel_id
                .as_ref()
                .and_then(|id| self.channels.get(id))
                .map(|c| self.channel_title(&c.channel))
                .unwrap_or_default();
            v_flex()
                .id("thread-panel")
                .w(px(THREAD_WIDTH))
                .h_full()
                .flex_none()
                .drag_over::<ExternalPaths>(move |style, _, _, _| style.bg(p.button_bg.opacity(0.06)))
                .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                    this.add_files(Pane::Thread, paths.paths().to_vec(), cx);
                }))
                .bg(p.center_bg)
                .text_color(p.center_color)
                .border_l_1()
                .border_color(p.border())
                .child(
                    h_flex()
                        .h(px(62.))
                        .flex_none()
                        .px_5()
                        .gap_2()
                        .border_b_1()
                        .border_color(p.border())
                        .child(div().text_size(px(16.)).font_bold().child("Fil"))
                        .child(div().flex_1().truncate().text_size(px(12.)).text_color(p.muted()).child(channel))
                        .child(
                            Button::new("close-thread")
                                .icon(Icon::new(IconName::X))
                                .small()
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| this.close_thread(cx))),
                        ),
                )
                .child(
                    div().flex_1().min_h_0().child(
                        MessageScroller::new("thread", self.thread_scroller.clone(), move |ix, _, cx| {
                            entity.read(cx).render_post(&entity, Pane::Thread, ix, cx)
                        })
                        .jump_button(false)
                        .with_row_style(StyleRefinement::default().pb(px(0.))),
                    ),
                )
                .child(self.render_composer(Pane::Thread, cx))
        });

        let team_bar = if self.teams.len() >= 2 { TEAM_BAR_WIDTH } else { 0. };
        let layout = h_flex()
            .id("chat-root")
            .size_full()
            .bg(p.center_bg)
            .on_drag_move(cx.listener(move |this, ev: &DragMoveEvent<SidebarResize>, _, cx| {
                let x = (ev.event.position.x - ev.bounds.left()) / px(1.) - team_bar;
                this.resize_sidebar(x, cx);
            }))
            .on_drop(cx.listener(|this, _: &SidebarResize, _, _| this.end_resize()))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.end_resize()))
            .children(self.render_team_bar(cx))
            .child(self.render_sidebar(cx))
            .child(main)
            .children(thread_panel);

        div().relative().size_full().child(layout).children(self.render_viewer(cx))
    }

    /// Visionneuse d'images plein écran (Échap ou clic à côté pour fermer).
    fn render_viewer(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let viewer = self.viewer.as_ref()?;
        let file = viewer.file.clone();
        let light = white();
        let action = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .px_3()
                .py_1p5()
                .rounded(px(4.))
                .cursor_pointer()
                .text_sm()
                .text_color(light.opacity(0.85))
                .hover(|s| s.bg(light.opacity(0.12)).text_color(light))
                .child(label)
        };

        let content: AnyElement = match (&viewer.image, &viewer.error) {
            (_, Some(err)) => div().text_color(light.opacity(0.8)).child(err.clone()).into_any_element(),
            (Some(image), None) => div()
                .id("viewer-image")
                .size_full()
                .child(img(image.clone()).size_full().object_fit(ObjectFit::Contain))
                // Un clic sur l'image ne ferme pas la visionneuse.
                .on_click(|_, _, cx| cx.stop_propagation())
                .into_any_element(),
            (None, None) => div().text_color(light.opacity(0.8)).child("Chargement…").into_any_element(),
        };

        Some(
            div()
                .id("viewer")
                // Bloque la souris pour ce qui est dessous (survols, infobulles).
                .occlude()
                .track_focus(&self.viewer_focus)
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(black().opacity(0.9))
                .text_color(light)
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                    if ev.keystroke.key == "escape" {
                        this.close_viewer(cx);
                    }
                }))
                .on_click(cx.listener(|this, _, _, cx| this.close_viewer(cx)))
                .child(
                    v_flex()
                        .size_full()
                        .child(
                            h_flex()
                                .h(px(60.))
                                .flex_none()
                                .px_5()
                                .gap_2()
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .child(div().truncate().font_semibold().child(file.name.clone()))
                                        .child(div().text_xs().text_color(light.opacity(0.6)).child(file.human_size())),
                                )
                                .child(action("viewer-download", "Télécharger").on_click(cx.listener({
                                    let file = file.clone();
                                    move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.download(file.clone(), false, cx);
                                    }
                                })))
                                .child(action("viewer-close", "Fermer ✕").on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.close_viewer(cx);
                                }))),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_h_0()
                                .p_6()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(content),
                        ),
                ),
        )
    }
}

impl Render for ChatApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_badge();
        self.apply_pending_text(window, cx);
        match self.screen {
            Screen::Login | Screen::Connecting => self.render_login(cx).into_any_element(),
            Screen::Chat => self.render_chat(cx).into_any_element(),
        }
    }
}

// ----------------------------------------------------------------------
// Dates
// ----------------------------------------------------------------------

fn local(ms: i64) -> Option<chrono::DateTime<chrono::Local>> {
    use chrono::TimeZone;
    chrono::Local.timestamp_millis_opt(ms).single()
}

fn day_of(ms: i64) -> Option<chrono::NaiveDate> {
    local(ms).map(|t| t.date_naive())
}

/// « 14:32 ».
fn format_hour(ms: i64) -> String {
    local(ms).map(|t| t.format("%H:%M").to_string()).unwrap_or_default()
}

/// Heure dans l'en-tête d'un message (la date est donnée par le séparateur).
fn format_time(ms: i64) -> String {
    format_hour(ms)
}

/// « Aujourd'hui », « Hier », « jeudi 19 novembre » ou « 3 mars 2025 ».
fn day_label(ms: i64) -> String {
    use chrono::Datelike;
    const DAYS: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
    const MONTHS: [&str; 12] = [
        "janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre",
        "décembre",
    ];
    let Some(t) = local(ms) else { return String::new() };
    let today = chrono::Local::now().date_naive();
    let date = t.date_naive();
    let month = MONTHS[date.month0() as usize];
    if date == today {
        "Aujourd'hui".into()
    } else if Some(date) == today.pred_opt() {
        "Hier".into()
    } else if date.year() == today.year() {
        format!("{} {} {month}", DAYS[date.weekday().num_days_from_monday() as usize], date.day())
    } else {
        format!("{} {month} {}", date.day(), date.year())
    }
}

fn date_separator(ms: i64, p: &Palette) -> impl IntoElement {
    h_flex()
        .w_full()
        .px_5()
        .pt_4()
        .pb_1()
        .gap_3()
        .child(div().flex_1().h(px(1.)).bg(p.border()))
        .child(div().text_size(px(12.)).font_semibold().text_color(p.center_color).child(day_label(ms)))
        .child(div().flex_1().h(px(1.)).bg(p.border()))
}

/// Taille d'affichage d'une image jointe : au plus 420 × 300, proportions gardées.
fn image_box(width: i64, height: i64) -> (f32, f32) {
    const MAX_W: f32 = 420.;
    const MAX_H: f32 = 300.;
    if width <= 0 || height <= 0 {
        return (320., 180.);
    }
    let (w, h) = (width as f32, height as f32);
    let scale = (MAX_W / w).min(MAX_H / h).min(1.0);
    ((w * scale).max(48.), (h * scale).max(48.))
}

/// Icône selon l'extension du fichier.
fn file_icon(extension: &str) -> &'static str {
    match extension.to_lowercase().as_str() {
        "pdf" => "📕",
        "doc" | "docx" | "odt" | "rtf" | "txt" | "md" => "📝",
        "xls" | "xlsx" | "ods" | "csv" => "📊",
        "ppt" | "pptx" | "odp" => "📽",
        "zip" | "gz" | "tar" | "7z" | "rar" => "🗜",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" => "🖼",
        "mp3" | "wav" | "ogg" | "flac" => "🎵",
        "mp4" | "mov" | "avi" | "mkv" | "webm" => "🎬",
        "rs" | "js" | "ts" | "py" | "php" | "go" | "java" | "json" | "yml" | "yaml" | "sql" | "sh" => "💻",
        _ => "📎",
    }
}

/// Ligne « Nouveaux messages », dans la couleur prévue par le thème.
fn new_messages_separator(p: &Palette) -> impl IntoElement {
    h_flex()
        .w_full()
        .px_5()
        .pt_3()
        .gap_3()
        .child(div().flex_1().h(px(1.)).bg(p.new_separator))
        .child(div().text_size(px(12.)).font_semibold().text_color(p.new_separator).child("Nouveaux messages"))
        .child(div().flex_1().h(px(1.)).bg(p.new_separator))
}

/// Résout les images `mm-emoji:nom` du Markdown vers les emojis personnalisés
/// chargés ; les autres adresses gardent le chargement normal.
fn emoji_resolver(
    emojis: Arc<HashMap<String, Arc<Image>>>,
) -> impl Fn(&SharedUri) -> ImageSource + Send + Sync + 'static {
    move |uri: &SharedUri| match uri.strip_prefix(markdown::CUSTOM_EMOJI_SCHEME) {
        Some(name) => match emojis.get(name) {
            Some(image) => ImageSource::Image(image.clone()),
            // Pas encore chargé : image transparente, remplacée au prochain rendu.
            None => ImageSource::Image(transparent_pixel()),
        },
        None => ImageSource::Resource(Resource::Uri(uri.clone())),
    }
}

fn transparent_pixel() -> Arc<Image> {
    static PIXEL: OnceLock<Arc<Image>> = OnceLock::new();
    PIXEL
        .get_or_init(|| {
            let mut png = Vec::new();
            let _ = image::RgbaImage::new(1, 1).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png);
            Arc::new(Image::from_bytes(ImageFormat::Png, png))
        })
        .clone()
}
