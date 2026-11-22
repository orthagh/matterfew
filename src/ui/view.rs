//! Rendu : écran de connexion, barre latérale, messages, fil, saisie.
//!
//! La mise en page suit celle du client web Mattermost et ses couleurs
//! viennent du thème de l'utilisateur (voir `theme.rs`).

use gpui_kit::assets::IconName;
use gpui_kit::component::Disableable as _;
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, StyledExt as _,
    avatar::Avatar,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Enter, Escape, IndentInline, Input, MoveDown, MoveUp, Paste, Textarea},
    message_scroller::MessageScroller,
    text::TextView,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use super::{ChatApp, FilesTab, Pane, PanelKind, PickerTarget, Screen, SidebarResize, search_tokens};
use crate::api::{Channel, FileInfo, Post, User, human_size};
use crate::backend::group_reactions;
use crate::editor::Format;
use crate::rich::{self, Embed};
use crate::sidebar::{self, Section};
use crate::theme::Palette;
use crate::{emoji, markdown};
use gpui_kit::component::text::{MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast};

pub(super) mod bookmarks;
pub(super) mod dialogs;
pub(super) mod manage;
pub(super) mod messaging;
pub(super) mod settings;

/// Réactions proposées dans la barre d'actions d'un message.
const QUICK_REACTIONS: [&str; 3] = ["+1", "heart", "joy"];

const TEAM_BAR_WIDTH: f32 = 64.;
/// Pastille des mentions d'une autre équipe : bleu franc, lisible sur tous les thèmes.
const TEAM_BADGE_BLUE: Hsla = Hsla { h: 0.6, s: 0.85, l: 0.52, a: 1. };
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
            // Opaque : un gris transparent laissait voir la photo dessous.
            _ => p.center_bg.blend(p.center_color.opacity(0.38)),
        }
    }

    fn status_label(&self, user_id: &str) -> SharedString {
        match self.status_of(user_id) {
            "online" => t!("status.online"),
            "away" => t!("status.away"),
            "dnd" => t!("status.dnd"),
            _ => t!("status.offline"),
        }
        .into()
    }

    /// Avatar avec pastille de présence ; `ring` = couleur du fond derrière.
    fn avatar(&self, user_id: &str, size: f32, ring: Hsla) -> impl IntoElement {
        let size = px(size);
        let mut avatar = Avatar::new().name(self.user_name(user_id)).size(size);
        if let Some(image) = self.avatars.get(user_id) {
            avatar = avatar.src(image.clone());
        }
        let dot = (size * 0.4).max(px(10.));
        div().relative().flex_none().size(size).child(avatar).child(
            div()
                .absolute()
                .right(-px(3.))
                .bottom(-px(3.))
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
        let label = |text: SharedString| div().text_sm().font_semibold().child(text);
        let separator = |text: SharedString| {
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
                .child(label(t!("login.token").into()))
                .child(Input::new(&self.token_input).mask_toggle())
                .child(
                    Button::new("connect-token")
                        .outline()
                        .label(t!("login.connect_token"))
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
                .child(t!("login.use_token"))
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
            // Grand dégradé façon « hero », des couleurs du thème.
            .bg(linear_gradient(
                135.,
                linear_color_stop(shade(p.sidebar_bg, -0.04, -0.06), 0.),
                linear_color_stop(p.button_bg, 1.),
            ))
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
                    .child(div().text_2xl().font_bold().child(t!("sso.window_title")))
                    .child(label(t!("login.server").into()))
                    .child(Input::new(&self.url_input))
                    .child(
                        Button::new("login-browser")
                            .primary()
                            .label(if busy == Some("browser") {
                                t!("login.finish_in_window")
                            } else {
                                t!("login.via_mattermost")
                            })
                            .loading(busy == Some("browser") || connecting)
                            .on_click(cx.listener(|this, _, window, cx| this.login_browser(window, cx))),
                    )
                    .child(div().text_xs().text_color(p.muted()).child(t!("login.via_mattermost_hint")))
                    .child(separator(t!("login.or_email").into()))
                    .child(label(t!("login.email").into()))
                    .child(Input::new(&self.email_input))
                    .child(label(t!("login.password").into()))
                    .child(Input::new(&self.password_input).mask_toggle())
                    .when(self.show_mfa, |el| {
                        el.child(label(t!("login.mfa_code").into())).child(Input::new(&self.mfa_input))
                    })
                    .child(
                        Button::new("login-password")
                            .outline()
                            .label(t!("login.sign_in"))
                            .loading(busy == Some("password"))
                            .on_click(cx.listener(|this, _, window, cx| this.login_password(window, cx))),
                    )
                    .when_some(self.error.clone(), |el, err| el.child(div().text_sm().text_color(p.error).child(err)))
                    .child(separator(t!("login.advanced").into()))
                    .child(token_section)
                    .child(div().text_xs().text_color(p.muted()).child(t!("login.remembered"))),
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
        // Teinte de la barre latérale, plus profonde : les deux barres se répondent.
        let tb = shade(p.sidebar_bg, 0.0, depth(&p, 0.03));
        Some(
            v_flex()
                .w(px(TEAM_BAR_WIDTH))
                .h_full()
                .flex_none()
                .items_center()
                .gap_3()
                .pt_4()
                .bg(linear_gradient(
                    180.,
                    linear_color_stop(tb, 0.),
                    linear_color_stop(shade(tb, 0.06, depth(&p, 0.05)), 1.),
                ))
                .border_r_1()
                .border_color(p.sidebar_text.opacity(0.08))
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
                    let tile = div()
                        .id(SharedString::from(format!("team-{}", team.id)))
                        .role(Role::Tab)
                        .aria_selected(active)
                        .aria_label(if mentions > 0 {
                            format!("{}, {}", team.display_name, a11y_mentions(mentions))
                        } else {
                            team.display_name.clone()
                        })
                        .flex_none()
                        .relative()
                        .size(px(44.))
                        .p(px(3.))
                        .rounded(px(13.))
                        .border_2()
                        .border_color(if active { p.sidebar_text } else { transparent_black() })
                        .cursor_pointer()
                        .when(!active, |el| el.opacity(if unread || mentions > 0 { 1.0 } else { 0.62 }))
                        .child(face)
                        // Mentions : compteur ; sinon simple pastille de non-lus.
                        .when(mentions > 0, |el| {
                            el.child(
                                div()
                                    .absolute()
                                    .top(-px(7.))
                                    .right(-px(7.))
                                    .min_w(px(19.))
                                    .h(px(19.))
                                    .px_1()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .border_2()
                                    .border_color(tb)
                                    .bg(TEAM_BADGE_BLUE)
                                    .text_color(white())
                                    .text_size(px(10.))
                                    .line_height(px(15.))
                                    .pb(px(1.))
                                    .font_bold()
                                    .child(centered_digits(
                                        if mentions > 99 { "99+".to_string() } else { mentions.to_string() },
                                        10.,
                                        cx,
                                    )),
                            )
                        })
                        .when(unread && mentions == 0, |el| {
                            el.child(
                                div()
                                    .absolute()
                                    .top(-px(4.))
                                    .right(-px(4.))
                                    .size(px(12.))
                                    .rounded_full()
                                    .border_2()
                                    .border_color(tb)
                                    .bg(p.sidebar_unread),
                            )
                        })
                        .tooltip({
                            let name: SharedString = team.display_name.clone().into();
                            move |window, cx| gpui_kit::component::tooltip::Tooltip::new(name.clone()).build(window, cx)
                        })
                        .on_click(cx.listener(move |this, _, _, cx| this.select_team(id.clone(), cx)));
                    // Pilule au bord gauche : haute pour l'équipe affichée, courte si non-lus.
                    let pill = (active || unread || mentions > 0).then(|| {
                        let h = if active { 34. } else { 10. };
                        div()
                            .absolute()
                            .left_0()
                            .top(px((44. - h) / 2.))
                            .w(px(4.))
                            .h(px(h))
                            .rounded_r(px(4.))
                            .bg(p.sidebar_text)
                    });
                    div().relative().w_full().flex().justify_center().children(pill).child(tile)
                })),
        )
    }

    pub(super) fn sidebar_sections(&self) -> Vec<Section> {
        sidebar::build(&sidebar::Input {
            channels: &self.channels,
            categories: &self.categories,
            prefs: &self.prefs,
            current: self.channel_id.as_deref(),
            sticky_unread: self.sticky_unread.as_deref(),
            title: |c| self.channel_title(c),
        })
    }

    fn render_channel_row(&self, channel_id: &str, manual: Option<&str>, cx: &mut Context<Self>) -> Option<AnyElement> {
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

        let ring = if selected { p.sidebar_hover } else { p.sidebar_bg };
        let icon: AnyElement = match channel.direct_partner(self.me_id()) {
            Some(partner) => self.avatar(&partner, 22., ring).into_any_element(),
            None => div()
                .when(state.muted, |el| el.opacity(0.45))
                .child(self.channel_badge(channel, 22., ring))
                .into_any_element(),
        };

        let draft = !selected && self.has_draft(channel_id);
        let menu = self.channel_menu(channel_id, state.muted, false, cx);
        // Lecteur d'écran : nom, mentions, non lu.
        let mut spoken = self.channel_title(channel);
        if state.mentions > 0 {
            spoken = format!("{spoken}, {}", a11y_mentions(state.mentions));
        } else if unread {
            spoken = format!("{spoken}, {}", t!("a11y.unread"));
        }
        let id = channel_id.to_string();
        Some(
            h_flex()
                .id(SharedString::from(format!("ch-{channel_id}")))
                .role(Role::Link)
                .aria_label(spoken)
                .aria_selected(selected)
                // Ne jamais rétrécir : avec beaucoup de canaux, la liste défile.
                .flex_none()
                .h(px(33.))
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
                .children(channel.direct_partner(self.me_id()).and_then(|partner| self.status_emoji(&partner, 15., cx)))
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
                            .line_height(px(18.))
                            // Chiffres de la police un peu bas dans leur ligne : 1 px de rattrapage.
                            .pb(px(2.))
                            .font_bold()
                            .bg(p.mention_bg)
                            .text_color(p.mention_color)
                            .child(centered_digits(state.mentions.to_string(), 11., cx)),
                    )
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.select_channel(id.clone(), cx);
                    this.focus_composer(window, cx);
                }))
                // Catégorie triée à la main : glisser un canal sur un autre prend sa place.
                .when_some(manual.map(str::to_string), |el, category_id| {
                    let dragged = super::DraggedChannel {
                        channel_id: channel_id.to_string(),
                        title: self.channel_title(channel).into(),
                        bg: p.sidebar_hover,
                        fg: p.sidebar_text,
                    };
                    let target = channel_id.to_string();
                    el.on_drag(dragged, |d, _, _, cx| cx.new(|_| d.clone()))
                        .drag_over::<super::DraggedChannel>(move |s, _, _, _| {
                            s.bg(p.sidebar_text.opacity(0.18)).border_color(p.sidebar_active_border)
                        })
                        .on_drop(cx.listener(move |this, d: &super::DraggedChannel, _, cx| {
                            this.move_channel(&d.channel_id, &category_id, &target, cx)
                        }))
                })
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
        panels: bool,
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
            if panels {
                menu = menu
                    .item(act(&t!("panels.pinned"), IconName::Pin, |this, _, cx| this.show_pinned(cx)))
                    .item(act(&t!("panels.saved"), IconName::Bookmark, |this, _, cx| this.show_saved(cx)))
                    .separator();
            }
            if unread {
                menu = menu.item(act(&t!("channel_menu.mark_read"), IconName::CheckCheck, |this, id, cx| {
                    this.mark_channels_read(vec![id], cx)
                }));
            }
            menu = menu
                .item(act(
                    &if favorite { t!("channel_menu.unfavorite") } else { t!("channel_menu.favorite") },
                    IconName::Star,
                    |this, id, cx| this.toggle_favorite(id, cx),
                ))
                .item(act(
                    &if muted { t!("channel_menu.unmute") } else { t!("channel_menu.mute") },
                    if muted { IconName::Bell } else { IconName::BellOff },
                    |this, id, cx| this.toggle_muted(id, cx),
                ))
                .item(act(&t!("channel_menu.notifications"), IconName::BellRing, |this, id, cx| {
                    this.open_notify_dialog(id, cx)
                }))
                .item(act(&t!("channel_menu.copy_link"), IconName::Link, |this, id, cx| {
                    this.copy_channel_link(&id, cx)
                }));
            if !direct {
                let (eb, bid) = (entity.clone(), id.clone());
                menu = menu.item(PopupMenuItem::new(t!("bookmarks.add")).icon(Icon::new(IconName::Bookmark)).on_click(
                    move |_, window, cx| eb.update(cx, |this, cx| this.open_bookmark_dialog(bid.clone(), window, cx)),
                ));
                let (e, cid) = (entity.clone(), id.clone());
                menu = menu.item(
                    PopupMenuItem::new(t!("channel_menu.settings")).icon(Icon::new(IconName::Settings)).on_click(
                        move |_, window, cx| {
                            e.update(cx, |this, cx| this.open_channel_settings(cid.clone(), window, cx))
                        },
                    ),
                );
                if panels {
                    let (entity, id) = (entity.clone(), id.clone());
                    menu = menu.separator().item(
                        PopupMenuItem::new(t!("channel_menu.add_members"))
                            .icon(Icon::new(IconName::UserPlus))
                            .on_click(move |_, window, cx| {
                                entity.update(cx, |this, cx| this.open_add_members(id.clone(), window, cx))
                            }),
                    );
                }
                menu = menu
                    .separator()
                    .item(act(&t!("channel_menu.leave"), IconName::LogOut, |this, id, cx| this.leave_channel(id, cx)));
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

        let mut list = v_flex()
            .id("channel-list")
            .role(Role::Navigation)
            .aria_label(t!("a11y.channels"))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pb_3()
            .children(self.render_threads_entry(cx));
        for section in self.sidebar_sections() {
            let category_id = section.id.clone();
            let group = SharedString::from(format!("catg-{}", section.id));
            let options = section.collapsible.then(|| self.category_options(&section.id, group.clone(), cx));
            // Catégorie triée à la main : ses canaux se réordonnent par glisser-déposer.
            let manual =
                self.categories.iter().find(|c| c.id == section.id && c.sorting == "manual").map(|c| c.id.clone());
            let unread_inside = section.collapsed
                && self.categories.iter().find(|c| c.id == section.id).is_some_and(|c| {
                    c.channel_ids.iter().any(|id| self.channels.get(id).is_some_and(|s| s.shows_unread()))
                });
            let count = self
                .categories
                .iter()
                .find(|c| c.id == section.id)
                .map(|c| c.channel_ids.iter().filter(|id| self.channels.contains_key(*id)).count())
                .unwrap_or(section.channel_ids.len());
            let dm_section = self.categories.iter().any(|c| c.id == section.id && c.kind == "direct_messages");
            let header = h_flex()
                .id(SharedString::from(format!("cat-{}", section.id)))
                .group(group)
                .flex_none()
                .h(px(30.))
                .mt_4()
                .mb_0p5()
                .mx_2()
                .px_2()
                .gap_1p5()
                .rounded(px(6.))
                .text_size(px(13.5))
                .font_bold()
                .text_color(p.sidebar_text.opacity(0.88))
                .child(div().flex_none().w(px(14.)).text_color(p.sidebar_text.opacity(0.5)).when(
                    section.collapsible,
                    |el| {
                        el.child(
                            Icon::new(if section.collapsed { IconName::ChevronRight } else { IconName::ChevronDown })
                                .size(px(13.)),
                        )
                    },
                ))
                .when(section.collapsible, |el| {
                    el.cursor_pointer()
                        .hover(|s| s.bg(p.sidebar_hover).text_color(p.sidebar_text))
                        .on_click(cx.listener(move |this, _, _, cx| this.toggle_category(&category_id, cx)))
                })
                .child(div().flex_none().child(section.title.clone()))
                .child(
                    div()
                        .flex_none()
                        .text_size(px(11.))
                        .font_semibold()
                        .text_color(p.sidebar_text.opacity(0.4))
                        .child(count.to_string()),
                )
                .when(unread_inside, |el| el.child(div().flex_none().size(px(7.)).rounded_full().bg(p.sidebar_unread)))
                .child(div().flex_1())
                .children(options)
                .when(dm_section, |el| {
                    el.child(
                        Button::new("dm-new")
                            .icon(Icon::new(IconName::Plus).text_color(p.sidebar_text))
                            .ghost()
                            .xsmall()
                            .accessibility_label(t!("direct.tooltip"))
                            .tooltip(t!("direct.tooltip"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.open_direct_dialog(window, cx)
                            })),
                    )
                });
            list = list.child(header);
            for channel_id in &section.channel_ids {
                if let Some(row) = self.render_channel_row(channel_id, manual.as_deref(), cx) {
                    list = list.child(row);
                }
            }
        }

        let me_id = self.me_id().to_string();
        let me_name = self.me.as_ref().map(User::display_name).unwrap_or_default();
        let status = if self.connected { self.status_label(&me_id) } else { t!("sidebar.reconnecting").into() };
        let account_menu = self.account_menu(cx);

        let resize_handle = div()
            .id("sidebar-resize")
            .absolute()
            .top_0()
            .right_0()
            .w(px(7.))
            .h_full()
            .cursor(CursorStyle::ResizeLeftRight)
            .on_drag(SidebarResize, |_, _, _, cx| cx.new(|_| SidebarResize));

        let open_team = cx.listener(|this, _, _, cx| this.open_team_card(cx));
        // Gros bouton « + » : créer un canal, une catégorie, inviter quelqu'un.
        let create_button = {
            let entity = cx.entity();
            Button::new("sidebar-create")
                .icon(Icon::new(IconName::Plus).size(px(18.)).text_color(p.sidebar_header_text))
                .rounded(px(8.))
                .with_size(px(32.))
                .ghost()
                .bg(p.sidebar_header_text.opacity(0.1))
                .accessibility_label(t!("sidebar.create_tooltip"))
                .tooltip(t!("sidebar.create_tooltip"))
                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                    let item = |label: &str, icon: IconName, kind: super::CreateKind| {
                        let entity = entity.clone();
                        PopupMenuItem::new(label.to_string()).icon(Icon::new(icon)).on_click(move |_, window, cx| {
                            entity.update(cx, |this, cx| this.open_create(kind, window, cx))
                        })
                    };
                    let (browse, direct) = (entity.clone(), entity.clone());
                    menu.min_w(px(240.))
                        .item(item(&t!("create.channel_title"), IconName::Hash, super::CreateKind::Channel))
                        .item(PopupMenuItem::new(t!("browse.title")).icon(Icon::new(IconName::Compass)).on_click(
                            move |_, window, cx| browse.update(cx, |this, cx| this.open_browse_dialog(window, cx)),
                        ))
                        .item(PopupMenuItem::new(t!("direct.title")).icon(Icon::new(IconName::SquarePen)).on_click(
                            move |_, window, cx| direct.update(cx, |this, cx| this.open_direct_dialog(window, cx)),
                        ))
                        .item(item(&t!("create.category_title"), IconName::FolderPlus, super::CreateKind::Category))
                        .separator()
                        .item(item(&t!("create.invite_title"), IconName::UserPlus, super::CreateKind::Invite))
                })
        };
        let goto = cx.listener(|this, _, window, cx| this.open_switcher(window, cx));
        v_flex()
            .relative()
            .w(px(self.settings.sidebar_width))
            .h_full()
            .flex_none()
            .bg(p.sidebar_bg)
            .bg(linear_gradient(
                180.,
                linear_color_stop(shade(p.sidebar_bg, -0.02, depth(&p, -0.03)), 0.),
                linear_color_stop(shade(p.sidebar_bg, 0.06, depth(&p, 0.07)), 1.),
            ))
            .text_color(p.sidebar_text)
            .child(
                h_flex()
                    .h(px(62.))
                    .border_b_1()
                    .border_color(p.sidebar_text.opacity(0.08))
                    .flex_none()
                    .pl_2()
                    .pr_3()
                    .gap_2()
                    .bg(p.sidebar_header_bg)
                    .text_color(p.sidebar_header_text)
                    .child(
                        h_flex()
                            .id("team-header")
                            .flex_1()
                            .min_w_0()
                            .h(px(44.))
                            .px_2()
                            .gap_1p5()
                            .rounded(px(8.))
                            .text_size(px(18.))
                            .font_bold()
                            .cursor_pointer()
                            .hover(|s| s.bg(p.sidebar_header_text.opacity(0.06)))
                            .child(div().truncate().child(team_name))
                            .child(
                                Icon::new(IconName::ChevronDown)
                                    .size(px(15.))
                                    .text_color(p.sidebar_header_text.opacity(0.6)),
                            )
                            .when(self.demo, |el| {
                                el.child(
                                    div()
                                        .ml_1()
                                        .px_1p5()
                                        .rounded(px(4.))
                                        .bg(p.away)
                                        .text_color(black())
                                        .text_size(px(10.))
                                        .child(t!("sidebar.demo_badge")),
                                )
                            })
                            .on_click(open_team),
                    )
                    .child(create_button),
            )
            // Comme le client officiel : un champ qui ouvre le sélecteur (Ctrl+K).
            .child(
                div().flex_none().px_3().pt_3().pb_1().child(
                    h_flex()
                        .id("goto-channel")
                        .h(px(32.))
                        .px_3()
                        .gap_2()
                        .rounded(px(8.))
                        .bg(p.sidebar_text.opacity(0.08))
                        .hover(|s| s.bg(p.sidebar_text.opacity(0.13)))
                        .cursor_pointer()
                        .text_sm()
                        .text_color(p.sidebar_text.opacity(0.7))
                        .child(Icon::new(IconName::Search).size(px(14.)))
                        .child(div().flex_1().truncate().child(t!("sidebar.goto_channel")))
                        .child(
                            div()
                                .flex_none()
                                .px_1p5()
                                .rounded(px(5.))
                                .border_1()
                                .border_color(p.sidebar_text.opacity(0.2))
                                .text_size(px(10.5))
                                .child("Ctrl K"),
                        )
                        .on_click(goto),
                ),
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
                        // Un clic : définir son statut personnalisé.
                        v_flex()
                            .id("me-status")
                            .flex_1()
                            .min_w_0()
                            .px_1p5()
                            .py_0p5()
                            .rounded(px(6.))
                            .cursor_pointer()
                            .hover(|s| s.bg(p.sidebar_hover))
                            .child(div().truncate().text_sm().font_semibold().child(me_name))
                            .child(match self.status_line(&me_id, 13.).filter(|_| self.connected) {
                                Some(line) => line.text_xs().text_color(p.sidebar_text.opacity(0.8)).into_any_element(),
                                None => div()
                                    .text_xs()
                                    .text_color(if self.connected { p.sidebar_text.opacity(0.64) } else { p.away })
                                    .child(status)
                                    .into_any_element(),
                            })
                            .on_click(cx.listener(|this, _, window, cx| this.open_status_dialog(window, cx))),
                    )
                    .child(account_menu),
            )
            .child(resize_handle)
    }

    /// « ⋯ » d'une catégorie : tri et « tout marquer comme lu ».
    fn category_options(&self, category_id: &str, group: SharedString, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity();
        let id = category_id.to_string();
        let category = self.categories.iter().find(|c| c.id == category_id);
        let sorting = category.map(|c| c.sorting.clone()).unwrap_or_default();
        let dm = category.is_some_and(|c| c.kind == "direct_messages");
        let custom = category.is_some_and(|c| c.kind == "custom");
        let muted = category.is_some_and(|c| c.muted);
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
                            .label(t!("category_menu.sort"))
                            .item(sort(&t!("category_menu.sort_alpha"), "alpha", alpha))
                            .item(sort(&t!("category_menu.sort_recent"), "recent", sorting == "recent"));
                        if !dm {
                            menu = menu.item(sort(&t!("category_menu.sort_manual"), "manual", sorting == "manual"));
                        }
                        if custom {
                            let (er, ir) = (entity.clone(), id.clone());
                            let (ed, id_d) = (entity.clone(), id.clone());
                            menu = menu
                                .separator()
                                .item(
                                    PopupMenuItem::new(t!("category_menu.rename"))
                                        .icon(Icon::new(IconName::Pencil))
                                        .on_click(move |_, window, cx| {
                                            er.update(cx, |this, cx| {
                                                this.open_category_dialog(ir.clone(), false, window, cx)
                                            })
                                        }),
                                )
                                .item(
                                    PopupMenuItem::new(t!("category_menu.delete"))
                                        .icon(Icon::new(IconName::Trash))
                                        .on_click(move |_, window, cx| {
                                            ed.update(cx, |this, cx| {
                                                this.open_category_dialog(id_d.clone(), true, window, cx)
                                            })
                                        }),
                                );
                        }
                        let (entity2, id2) = (entity.clone(), id.clone());
                        let (entity, id) = (entity.clone(), id.clone());
                        menu.separator()
                            .item(
                                PopupMenuItem::new(if muted {
                                    t!("category_menu.unmute")
                                } else {
                                    t!("category_menu.mute")
                                })
                                .icon(Icon::new(if muted { IconName::Bell } else { IconName::BellOff }))
                                .on_click(move |_, _, cx| {
                                    entity2.update(cx, |this, cx| this.toggle_category_muted(&id2, cx))
                                }),
                            )
                            .item(
                                PopupMenuItem::new(t!("category_menu.mark_read"))
                                    .icon(Icon::new(IconName::CheckCheck))
                                    .on_click(move |_, _, cx| {
                                        entity.update(cx, |this, cx| this.mark_category_read(&id, cx))
                                    }),
                            )
                    }),
            )
            .into_any_element()
    }

    /// Menu du compte : statut et déconnexion.
    fn account_menu(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let entity = cx.entity();
        let current = self.status_of(self.me_id()).to_string();
        let p = self.palette;
        Button::new("account-menu")
            .icon(Icon::new(IconName::EllipsisVertical).text_color(p.sidebar_text))
            .ghost()
            .small()
            .accessibility_label(t!("account.tooltip"))
            .tooltip(t!("account.tooltip"))
            .dropdown_menu(move |menu, _, _| {
                let status = |label: &str, value: &'static str, icon: IconName| {
                    let entity = entity.clone();
                    let color = match value {
                        "online" => p.online,
                        "away" => p.away,
                        "dnd" => p.dnd,
                        _ => p.muted(),
                    };
                    PopupMenuItem::new(label.to_string())
                        .icon(Icon::new(icon).text_color(color))
                        .checked(current == value)
                        .on_click(move |_, _, cx| entity.update(cx, |this, cx| this.set_status(value, cx)))
                };
                let entity = entity.clone();
                menu.min_w(px(220.))
                    .label(t!("account.status"))
                    .item(status(&t!("status.online"), "online", IconName::CircleCheck))
                    .item(status(&t!("status.away"), "away", IconName::Moon))
                    .item(status(&t!("status.dnd"), "dnd", IconName::CircleMinus))
                    .item(status(&t!("status.offline"), "offline", IconName::Circle))
                    .separator()
                    .item({
                        let entity = entity.clone();
                        PopupMenuItem::new(t!("custom_status.menu"))
                            .icon(Icon::new(IconName::FaceSlightlySmiling))
                            .on_click(move |_, window, cx| {
                                entity.update(cx, |this, cx| this.open_status_dialog(window, cx))
                            })
                    })
                    .separator()
                    .item({
                        let entity = entity.clone();
                        PopupMenuItem::new(t!("settings.menu"))
                            .icon(Icon::new(IconName::Settings))
                            .on_click(move |_, window, cx| entity.update(cx, |this, cx| this.open_settings(window, cx)))
                    })
                    .separator()
                    .item(
                        PopupMenuItem::new(t!("account.log_out"))
                            .icon(Icon::new(IconName::LogOut))
                            .on_click(move |_, window, cx| entity.update(cx, |this, cx| this.logout(window, cx))),
                    )
            })
    }

    // ------------------------------------------------------------------
    // Messages
    // ------------------------------------------------------------------

    fn mentions_me(&self, post: &Post) -> bool {
        let Some(me) = &self.me else { return false };
        let needle = format!("@{}", me.username);
        if post.user_id != me.id && markdown::contains_keyword(&post.message, &self.mention_keywords) {
            return true;
        }
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
        // Réponse d'une commande « / » (« visible uniquement par vous ») : du
        // Markdown complet, avec ses sauts de ligne, pas un message système.
        let ephemeral = post.kind == "system_ephemeral";
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
        // Bots (GitHub…) : longs messages réduits, dépliables.
        let is_bot =
            post.is_bot_post() || bot_name.is_some() || self.users.get(&post.user_id).is_some_and(|u| u.is_bot);
        let long_bot = is_bot && !ephemeral && bot_post_lines(post) > 3;
        let condensed = long_bot && !self.expanded_bots.contains(&post.id);
        // Messages successifs d'un même bot : un seul bloc, sans nom ni heure,
        // comme une suite de cartes (l'heure apparaît au survol).
        let bot_key = |p: &Post| {
            let name = rich::override_username(p);
            let bot = p.is_bot_post() || name.is_some() || self.users.get(&p.user_id).is_some_and(|u| u.is_bot);
            bot.then(|| name.unwrap_or_else(|| p.user_id.clone()))
        };
        let my_key = if is_bot && !ephemeral { bot_key(post) } else { None };
        let next = list.get(ix + 1).map(|v| &v.post);
        let run_prev = my_key.is_some()
            && !new_day
            && !first_new
            && prev.filter(|p| p.kind != "system_ephemeral").and_then(&bot_key) == my_key;
        let run_next = my_key.is_some()
            && next.is_some_and(|n| {
                day_of(n.create_at) == day_of(post.create_at)
                    && !(pane == Pane::Channel && self.first_new_index() == Some(ix + 1))
                    && n.kind != "system_ephemeral"
                    && bot_key(n) == my_key
            });
        let bot_block = my_key.is_some();
        let compact = compact || run_prev;
        let header = h_flex()
            .gap_2()
            .items_baseline()
            .child({
                let (entity, uid) = (entity.clone(), post.user_id.clone());
                div()
                    .id(key("name"))
                    .text_size(px(15.))
                    .font_semibold()
                    .when(bot_name.is_none(), |el| {
                        el.cursor_pointer().hover(|s| s.underline()).on_click(move |ev, window, cx| {
                            let (uid, at) = (uid.clone(), ev.position());
                            entity.update(cx, |this, cx| this.open_profile(uid, at, window, cx));
                        })
                    })
                    .child(bot_name.clone().unwrap_or_else(|| self.user_name(&post.user_id)))
            })
            .when(bot_name.is_none(), |el| el.children(self.status_emoji(&post.user_id, 16., cx)))
            .when(is_bot, |el| {
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
            .when(ephemeral, |el| {
                el.child(div().text_xs().italic().text_color(p.muted()).child(t!("post.only_visible_to_you")))
            })
            .when(post.is_pinned, |el| {
                el.child(
                    h_flex()
                        .gap_0p5()
                        .text_xs()
                        .text_color(p.link)
                        .child(Icon::new(IconName::Pin).size(px(12.)))
                        .child(t!("post.pinned")),
                )
            })
            .when(self.is_saved(&post.id), |el| {
                el.child(
                    h_flex()
                        .gap_0p5()
                        .text_xs()
                        .text_color(p.away)
                        .child(Icon::new(IconName::Bookmark).size(px(12.)))
                        .child(t!("post.saved")),
                )
            });

        let body: AnyElement = if post.is_system() && !ephemeral {
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
                        .plugin(self.tag_plugin(entity))
                        .on_link_click({
                            let entity = entity.clone();
                            move |url, _, window, cx| {
                                if let Some(name) = url.strip_prefix(markdown::MENTION_SCHEME) {
                                    let (name, at) = (name.to_string(), window.mouse_position());
                                    entity.update(cx, |this, cx| this.open_profile_by_name(name, at, window, cx));
                                } else if let Some(tag) = url.strip_prefix(markdown::HASHTAG_SCHEME) {
                                    let terms = format!("#{tag}");
                                    entity.update(cx, |this, cx| this.search_for(terms, window, cx));
                                } else if !entity.update(cx, |this, cx| this.open_internal_link(url, window, cx)) {
                                    open_link(url, cx);
                                } else {
                                    cx.stop_propagation();
                                }
                            }
                        }),
                )
                .when(post.edit_at > 0, |el| el.child(div().text_xs().text_color(p.muted()).child(t!("post.edited"))))
                .into_any_element()
        };

        // Réactions : cliquer sur une pastille ajoute ou retire la sienne.
        let reactions = group_reactions(&post.metadata.reactions, me);
        let reaction_row = h_flex()
            .gap_1()
            .mt_1()
            .flex_wrap()
            .children(reactions.into_iter().map(|(name, count, by_me)| {
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
                    .tooltip({
                        let tip: SharedString = self.who_reacted(post, &name).into();
                        move |window, cx| gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                    })
                    .child(match self.custom_emojis.get(&name) {
                        Some(image) => img(image.clone()).size(px(16.)).into_any_element(),
                        None => emoji_face(&emoji::lookup(&name), 16.),
                    })
                    .child(count.to_string())
                    .on_click(move |_, _, cx| {
                        entity.update(cx, |this, _| this.toggle_reaction(post_id.clone(), emoji_name.clone()));
                    })
            }))
            // Une autre réaction : le sélecteur d'emojis.
            .child({
                let (entity, post_id) = (entity.clone(), post.id.clone());
                div()
                    .id(key("react-add"))
                    .h(px(24.))
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(transparent_black())
                    .bg(p.center_color.opacity(0.06))
                    .text_color(p.muted())
                    .cursor_pointer()
                    .hover(|s| s.border_color(p.center_color.opacity(0.24)).text_color(p.center_color))
                    .child(Icon::new(IconName::FaceSlightlySmilingPlus).size(px(15.)))
                    .tooltip(|window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(SharedString::from(t!("post.add_reaction")))
                            .build(window, cx)
                    })
                    .on_click(move |_, window, cx| {
                        let target = PickerTarget::Reaction(post_id.clone());
                        entity.update(cx, |this, cx| this.open_emoji_picker(target, window, cx));
                    })
            });

        let replies = (pane == Pane::Channel && post.reply_count > 0).then(|| {
            let entity = entity.clone();
            let root = post.id.clone();
            let label = if post.reply_count == 1 {
                format!("↩ {}", t!("threads.replies.one"))
            } else {
                format!("↩ {}", t!("threads.replies.other", count = post.reply_count))
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

        let toolbar = (!post.is_system()).then(|| {
            self.render_post_toolbar(entity, pane, post.id.clone(), mine, confirming, group.clone(), bot_block, cx)
        });

        // Colonne de gauche : avatar, ou heure au survol des messages groupés.
        let media_only = {
            let m = post.message.trim();
            m.is_empty()
                || (!m.contains(char::is_whitespace) && (m.starts_with("http://") || m.starts_with("https://")))
        };
        let left = div().w(px(AVATAR)).flex_none().map(|el| {
            if compact && (media_only || bot_block) {
                el
            } else if compact {
                el.pt(px(3.))
                    .text_size(px(10.))
                    .text_color(p.muted())
                    .invisible()
                    .group_hover(group.clone(), |s| s.visible())
                    .child(format_hour(post.create_at))
            } else {
                let (entity, uid) = (entity.clone(), post.user_id.clone());
                el.pt(px(2.)).child(
                    div()
                        .id(key("av"))
                        .cursor_pointer()
                        // Bloc de bot : petit avatar, aligné sur la première ligne.
                        .child(self.avatar(&post.user_id, AVATAR, p.center_bg))
                        .on_click(move |ev, window, cx| {
                            let (uid, at) = (uid.clone(), ev.position());
                            entity.update(cx, |this, cx| this.open_profile(uid, at, window, cx));
                        }),
                )
            }
        });

        // Lecteur d'écran : « Auteur, 14:32 : texte ».
        let spoken: String = format!(
            "{}, {} : {}",
            self.user_name(&post.user_id),
            format_time(post.create_at),
            markdown::plain_text(&post.message)
        )
        .chars()
        .take(400)
        .collect();
        let row = h_flex()
            .id(SharedString::from(format!("{prefix}-post-{}", post.id)))
            .role(Role::Article)
            .aria_label(spoken)
            .group(group)
            .relative()
            .items_start()
            .gap_3()
            .w_full()
            .px_5()
            .map(|el| match (bot_block, run_prev) {
                (true, true) => el,
                (true, false) => el.pt_2(),
                (false, _) if !compact => el.pt_3(),
                _ => el,
            })
            .when(!(bot_block && run_next), |el| el.pb_1())
            .when(mine && !highlighted, |el| {
                el.bg(p.button_bg.opacity(0.05)).border_l_2().border_color(p.button_bg.opacity(0.55))
            })
            .when(highlighted, |el| el.bg(p.mention_highlight_bg.opacity(0.18)))
            .when(being_edited, |el| el.bg(p.away.opacity(0.12)))
            .when(!highlighted && !being_edited, |el| el.hover(|s| s.bg(p.hover())))
            .child(left)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .when(!compact && !bot_block, |el| el.child(header))
                    .child({
                        let inner = v_flex()
                            .child(body)
                            .children(self.render_rich(entity, post, &key("rich"), &p))
                            .when(!post.metadata.files.is_empty(), |el| {
                                el.child(self.render_files(entity, &post.metadata.files, &key("files"), &p))
                            });
                        // (bot déplié) : contenu entier, sans réduction de hauteur.
                        let (inner, content_full) = if bot_block {
                            (div().into_any_element(), inner.into_any_element())
                        } else {
                            (inner.into_any_element(), div().into_any_element())
                        };
                        let inner = if condensed && !bot_block {
                            div()
                                .relative()
                                .max_h(px(BOT_MAX_HEIGHT))
                                .overflow_hidden()
                                .child(inner)
                                .child(div().absolute().left_0().right_0().bottom_0().h(px(30.)).bg(linear_gradient(
                                    180.,
                                    linear_color_stop(p.center_bg.opacity(0.), 0.),
                                    linear_color_stop(p.center_bg, 1.),
                                )))
                                .into_any_element()
                        } else {
                            inner
                        };
                        let more = (long_bot && !bot_block).then(|| {
                            let (entity, id) = (entity.clone(), post.id.clone());
                            {
                                h_flex()
                                    .id(key("bot-more"))
                                    .gap_1()
                                    .mt_0p5()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(p.link)
                                    .cursor_pointer()
                                    .hover(|s| s.underline())
                                    .child(
                                        Icon::new(if condensed {
                                            IconName::ChevronDown
                                        } else {
                                            IconName::ChevronRight
                                        })
                                        .size(px(12.)),
                                    )
                                    .child(if condensed { t!("post.show_more") } else { t!("post.show_less") })
                                    .on_click(move |_, _, cx| {
                                        let id = id.clone();
                                        entity.update(cx, |this, cx| this.toggle_bot_post(id, cx));
                                    })
                            }
                        });
                        if bot_block {
                            // Une ligne par notification (couleur, résumé, heure) ; un clic
                            // (hors liens) déplie ou replie, le chevron restant à droite.
                            let (summary, color) = bot_summary(post);
                            // Tout tient dans le résumé : rien à déplier.
                            let expandable = bot_expandable(post, &summary, self.line_chars(pane));
                            let expanded = expandable && self.expanded_bots.contains(&post.id);
                            let bar = color
                                .as_deref()
                                .and_then(rich::attachment_color)
                                .and_then(crate::theme::parse_color)
                                .or_else(|| notification_color(&post.message));
                            let (entity, id) = (entity.clone(), post.id.clone());
                            let meta = h_flex()
                                .flex_none()
                                .gap_2()
                                .text_xs()
                                .text_color(p.muted())
                                .child(format_hour(post.create_at))
                                .child(div().w(px(13.)).when(expandable, |d| {
                                    d.child(
                                        Icon::new(if expanded { IconName::ChevronUp } else { IconName::ChevronDown })
                                            .size(px(13.)),
                                    )
                                }));
                            // Dépliée : le résumé laisse la place au contenu (sinon il se répète).
                            let line = h_flex()
                                .gap_2()
                                .h(px(30.))
                                .child(
                                    div().flex_none().w(px(3.)).h(px(14.)).rounded(px(2.)).bg(bar.unwrap_or(p.muted())),
                                )
                                .child(div().flex_1().min_w_0().max_h(px(20.)).overflow_hidden().text_sm().when(
                                    !expanded,
                                    |d| {
                                        d.child(
                                            TextView::markdown(
                                                key("bot-sum"),
                                                markdown::prepare_for(&summary, &[], Some(&post.channel_id)),
                                            )
                                            .selectable(false)
                                            .max_lines(1)
                                            .on_link_click(|url, _, _, cx| open_link(url, cx)),
                                        )
                                    },
                                ))
                                .child(meta);
                            // Au repos : rien qu'une ligne ; au survol, un fond et une bordure.
                            v_flex()
                                .id(key("bot-line"))
                                .ml(-px(10.))
                                .pl(px(9.))
                                .pr_3()
                                .rounded(px(8.))
                                .border_1()
                                .border_color(transparent_black())
                                .when(expanded, |d| d.bg(p.center_color.opacity(0.03)).border_color(p.border()))
                                .when(expandable, |d| {
                                    d.cursor_pointer()
                                        .hover(|d| d.bg(p.center_color.opacity(0.04)).border_color(p.border()))
                                })
                                .child(line)
                                .when(expanded, |d| {
                                    d.child(div().mt(-px(26.)).pl(px(11.)).pr(px(70.)).pb_2().child(content_full))
                                })
                                .on_click(move |_, _, cx| {
                                    if expandable {
                                        let id = id.clone();
                                        entity.update(cx, |this, cx| this.toggle_bot_post(id, cx));
                                    }
                                })
                                .into_any_element()
                        } else {
                            v_flex().child(inner).children(more).into_any_element()
                        }
                    })
                    .when(!post.metadata.reactions.is_empty(), |el| el.child(reaction_row))
                    .children(self.out_of_channel_action(post, entity))
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

    /// Caractères (texte de 14 px) qui tiennent sur une ligne de notification,
    /// d'après la largeur réelle de la colonne des messages.
    fn line_chars(&self, pane: Pane) -> usize {
        let w = self.viewport_w;
        let column = match pane {
            Pane::Thread => THREAD_WIDTH,
            Pane::Channel => {
                let sidebar = if w < super::NARROW_SIDEBAR {
                    0.
                } else {
                    self.settings.sidebar_width + if self.teams.len() > 1 { TEAM_BAR_WIDTH } else { 0. }
                };
                let right = if w < super::NARROW_PANELS {
                    0.
                } else if self.thread.is_some() || self.side.is_some() {
                    THREAD_WIDTH
                } else if self.settings.show_members {
                    240.
                } else {
                    0.
                };
                w - sidebar - right
            }
        };
        // Marges, avatar, barre de couleur, heure et chevron.
        ((column - 160.).max(100.) / 7.2) as usize
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
        bot_line: bool,
        cx: &App,
    ) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let id = |what: &str| SharedString::from(format!("{pane:?}-{what}-{post_id}"));

        // Dans la zone du message (et non au-dessus) : en allant vers la barre,
        // la souris reste sur le message et la barre ne saute pas ailleurs.
        // Sur une notification de bot (une ligne), la barre laisse voir l'heure et le chevron.
        let mut bar = h_flex()
            .absolute()
            .top(px(if bot_line { 1. } else { 4. }))
            .right(px(if bot_line { 120. } else { 20. }))
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
                .child(div().px_2().text_xs().child(t!("post_menu.delete_confirm")))
                .child(
                    Button::new(id("del-yes"))
                        .label(t!("common.delete"))
                        .xsmall()
                        .danger()
                        .on_click(move |_, _, cx| e1.update(cx, |this, cx| this.delete_post(pid.clone(), cx))),
                )
                .child(Button::new(id("del-no")).label(t!("common.cancel")).xsmall().ghost().on_click(
                    move |_, _, cx| {
                        e2.update(cx, |this, cx| {
                            this.confirm_delete = None;
                            cx.notify();
                        })
                    },
                ));
        }

        for name in self.quick_reactions() {
            let (entity, pid) = (entity.clone(), post_id.clone());
            let face = match self.custom_emojis.get(&name) {
                Some(image) => img(image.clone()).size(px(14.)).into_any_element(),
                None => emoji_face(&emoji::lookup(&name), 14.),
            };
            bar = bar.child(
                Button::new(id(&format!("qr-{name}")))
                    .child(face)
                    .xsmall()
                    .ghost()
                    .tooltip(format!(":{name}:"))
                    .on_click(move |_, _, cx| {
                        entity.update(cx, |this, _| this.toggle_reaction(pid.clone(), name.clone()));
                    }),
            );
        }
        {
            let (entity, pid) = (entity.clone(), post_id.clone());
            bar = bar.child(
                Button::new(id("react"))
                    .icon(Icon::new(IconName::FaceSlightlySmilingPlus).size(px(16.)))
                    .xsmall()
                    .ghost()
                    .accessibility_label(t!("post.add_reaction"))
                    .tooltip(t!("post.add_reaction"))
                    .on_click(move |_, window, cx| {
                        entity.update(cx, |this, cx| {
                            this.open_emoji_picker(PickerTarget::Reaction(pid.clone()), window, cx)
                        })
                    }),
            );
        }
        bar = bar.child(div().w(px(1.)).h(px(18.)).mx_1().bg(theme.border));
        if pane == Pane::Channel {
            let (entity, pid) = (entity.clone(), post_id.clone());
            bar = bar.child(
                Button::new(id("reply"))
                    .icon(Icon::new(IconName::Reply).size(px(16.)))
                    .xsmall()
                    .ghost()
                    .accessibility_label(t!("post.reply_in_thread"))
                    .tooltip(t!("post.reply_in_thread"))
                    .on_click(move |_, window, cx| {
                        entity.update(cx, |this, cx| this.open_thread(pid.clone(), window, cx))
                    }),
            );
        }

        // Toutes les options dans « ⋯ ».
        let saved = self.is_saved(&post_id);
        let pinned = self.find_post(&post_id).is_some_and(|p| p.is_pinned);
        let root =
            self.find_post(&post_id).map(|p| if p.root_id.is_empty() { p.id.clone() } else { p.root_id.clone() });
        let following = root.as_deref().is_some_and(|r| self.is_following(r));
        let crt = self.crt;
        let (entity, pid) = (entity.clone(), post_id.clone());
        bar.child(
            Button::new(id("more"))
                .icon(Icon::new(IconName::Ellipsis).size(px(16.)))
                .xsmall()
                .ghost()
                .accessibility_label(t!("post.more_actions"))
                .tooltip(t!("post.more_actions"))
                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                    let item =
                        |label: &str,
                         icon: IconName,
                         f: fn(&mut ChatApp, String, &mut Window, &mut Context<ChatApp>)| {
                            let (entity, pid) = (entity.clone(), pid.clone());
                            PopupMenuItem::new(label.to_string()).icon(Icon::new(icon)).on_click(
                                move |_, window, cx| entity.update(cx, |this, cx| f(this, pid.clone(), window, cx)),
                            )
                        };
                    let mut menu = menu.min_w(px(230.));
                    if pane == Pane::Channel {
                        menu = menu.item(item(&t!("post_menu.reply"), IconName::Reply, |this, id, window, cx| {
                            this.open_thread(id, window, cx)
                        }));
                    }
                    menu = menu
                        .item(item(&t!("post_menu.mark_unread"), IconName::MailOpen, |this, id, _, cx| {
                            this.mark_unread_from(id, cx)
                        }))
                        .item(item(
                            &if saved { t!("post_menu.unsave") } else { t!("post_menu.save") },
                            IconName::Bookmark,
                            |this, id, _, cx| this.toggle_saved(id, cx),
                        ))
                        .item(item(
                            &if pinned { t!("post_menu.unpin") } else { t!("post_menu.pin") },
                            IconName::Pin,
                            |this, id, _, _| this.toggle_pinned(id),
                        ));
                    if crt {
                        menu = menu.item(item(
                            &if following { t!("post_menu.unfollow") } else { t!("post_menu.follow") },
                            if following { IconName::BellOff } else { IconName::Bell },
                            |this, id, _, cx| this.toggle_follow(id, cx),
                        ));
                    }
                    let remind = |label: &str, minutes: Option<i64>| {
                        let (entity, pid) = (entity.clone(), pid.clone());
                        PopupMenuItem::new(label.to_string())
                            .icon(Icon::new(IconName::Clock))
                            .on_click(move |_, _, cx| entity.update(cx, |this, _| this.remind(pid.clone(), minutes)))
                    };
                    menu = menu
                        .separator()
                        .label(t!("post_menu.remind"))
                        .item(remind(&t!("post_menu.remind_30m"), Some(30)))
                        .item(remind(&t!("post_menu.remind_1h"), Some(60)))
                        .item(remind(&t!("post_menu.remind_2h"), Some(120)))
                        .item(remind(&t!("post_menu.remind_tomorrow"), None))
                        .separator()
                        .item(item(&t!("post_menu.copy_link"), IconName::Link, |this, id, _, cx| {
                            this.copy_post_link(&id, cx)
                        }))
                        .item(item(&t!("post_menu.copy_text"), IconName::Copy, |this, id, _, cx| {
                            this.copy_post_text(&id, cx)
                        }))
                        .item(item(&t!("post_menu.forward"), IconName::Forward, |this, id, window, cx| {
                            this.open_forward(id, window, cx)
                        }));
                    if mine {
                        let (e, id) = (entity.clone(), pid.clone());
                        menu = menu
                            .separator()
                            .item(PopupMenuItem::new(t!("post_menu.edit")).icon(Icon::new(IconName::Pencil)).on_click(
                                move |_, window, cx| {
                                    e.update(cx, |this, cx| this.start_edit(pane, id.clone(), window, cx))
                                },
                            ))
                            .item(item(&t!("common.delete"), IconName::Trash, |this, id, _, cx| {
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
                .small()
                .tooltip(format.label())
                .on_click(cx.listener(move |this, _, window, cx| this.format(pane, format, window, cx)))
        };
        let divider = || div().w(px(1.)).h(px(18.)).mx_1().bg(p.border());
        let show_formatting = self.settings.show_formatting;
        let format_bar = show_formatting.then(|| {
            h_flex()
                .px_2()
                .py_1()
                .gap_1()
                .child(format_button(Format::Bold, IconName::Bold, cx))
                .child(format_button(Format::Italic, IconName::Italic, cx))
                .child(format_button(Format::Strike, IconName::Strikethrough, cx))
                .child(divider())
                .child(format_button(Format::Heading, IconName::Heading, cx))
                .child(format_button(Format::Link, IconName::Link, cx))
                .child(format_button(Format::Code, IconName::Code, cx))
                .child(format_button(Format::CodeBlock, IconName::SquareCode, cx))
                .child(divider())
                .child(format_button(Format::Quote, IconName::TextQuote, cx))
                .child(format_button(Format::Bullet, IconName::List, cx))
                .child(format_button(Format::Numbered, IconName::ListOrdered, cx))
        });
        let tool = |id: String, icon: IconName, tip: SharedString| {
            Button::new(SharedString::from(id)).icon(Icon::new(icon)).ghost().small().tooltip(tip)
        };
        let toolbar = h_flex()
            .px_2()
            .pt_1()
            .pb_2()
            .gap_1()
            .child(
                tool(format!("fmt-toggle-{pane:?}"), IconName::Type, t!("composer.formatting").into())
                    .toggled(show_formatting)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_formatting(cx))),
            )
            .child(
                tool(format!("emoji-{pane:?}"), IconName::FaceSlightlySmiling, t!("composer.insert_emoji").into())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_emoji_picker(PickerTarget::Composer(pane), window, cx)
                    })),
            )
            .child(
                tool(format!("attach-{pane:?}"), IconName::Paperclip, t!("composer.attach_files").into())
                    .on_click(cx.listener(move |this, _, window, cx| this.pick_files(pane, window, cx))),
            )
            .child(
                tool(
                    format!("preview-{pane:?}"),
                    if previewing { IconName::EyeOff } else { IconName::Eye },
                    if previewing { t!("composer.back_to_writing") } else { t!("composer.preview") }.into(),
                )
                .toggled(previewing)
                .on_click(cx.listener(move |this, _, window, cx| this.toggle_preview(pane, window, cx))),
            )
            .child(div().flex_1())
            .child(
                Button::new(SharedString::from(format!("send-{pane:?}")))
                    .icon(Icon::new(IconName::SendHorizontal))
                    .when(editing, |b| b.label(t!("common.save")))
                    .primary()
                    .disabled(text.trim().is_empty() && !self.attachments.iter().any(|a| a.pane == pane))
                    .tooltip(if editing { t!("composer.save_tooltip") } else { t!("composer.send_tooltip") })
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
                    div().text_color(p.muted()).child(t!("composer.nothing_to_preview")).into_any_element()
                } else {
                    TextView::markdown(SharedString::from(format!("preview-text-{pane:?}")), prepared)
                        .into_any_element()
                })
                .into_any_element()
        } else {
            div().px_2().pt_1p5().child(Textarea::new(&state).appearance(false)).into_any_element()
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
            .capture_action(cx.listener(move |this, _: &Paste, _, cx| {
                if this.paste_attachments(pane, cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(move |this, _: &MoveDown, window, cx| this.ac_action(pane, "down", window, cx)))
            .capture_action(cx.listener(move |this, _: &Enter, window, cx| this.ac_action(pane, "enter", window, cx)))
            .capture_action(
                cx.listener(move |this, _: &IndentInline, window, cx| this.ac_action(pane, "tab", window, cx)),
            )
            .capture_action(cx.listener(move |this, _: &Escape, window, cx| this.ac_action(pane, "escape", window, cx)))
            .children(popup)
            .child(
                v_flex()
                    .rounded(px(10.))
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
                                .child(div().flex_1().font_semibold().child(t!("composer.editing_message")))
                                .child(
                                    Button::new(SharedString::from(format!("cancel-edit-{pane:?}")))
                                        .icon(Icon::new(IconName::X))
                                        .xsmall()
                                        .ghost()
                                        .accessibility_label(t!("composer.cancel_edit"))
                                        .tooltip(t!("composer.cancel_edit"))
                                        .on_click(cx.listener(|this, _, window, cx| this.cancel_edit(window, cx))),
                                ),
                        )
                    })
                    .children(self.render_attachments(pane, cx))
                    .children(self.render_spelling(pane, cx))
                    .children(format_bar)
                    .child(input)
                    .child(toolbar),
            )
            .child(div().h(px(18.)).px_1().pt_0p5().text_size(px(11.)).italic().text_color(p.muted()).children(typing))
    }

    /// Image externe déjà chargée, dans une boîte de taille fixe (en attendant :
    /// boîte vide de la même taille, pour que la ligne ne saute pas).
    fn external_image(&self, url: &str, w: f32, h: f32, p: &Palette) -> AnyElement {
        match self.external_images.get(url) {
            Some(image) => {
                img(image.clone()).w(px(w)).h(px(h)).object_fit(ObjectFit::Contain).rounded(px(4.)).into_any_element()
            }
            None => div().w(px(w)).h(px(h)).rounded(px(4.)).bg(p.center_color.opacity(0.05)).into_any_element(),
        }
    }

    /// Pièces jointes des intégrations et aperçus de liens d'un message.
    fn render_rich(&self, entity: &Entity<Self>, post: &Post, key: &SharedString, p: &Palette) -> Vec<AnyElement> {
        let p = *p;
        let mut out = Vec::new();
        let open = |url: String| move |_: &ClickEvent, _: &mut Window, cx: &mut App| open_link(&url, cx);
        let link_click = |url: &SharedString, _: &ClickEvent, _: &mut Window, cx: &mut App| {
            if !url.starts_with(markdown::MENTION_SCHEME) {
                open_link(url, cx);
            }
        };

        for (i, a) in rich::attachments(post).into_iter().enumerate() {
            let id = |what: &str| SharedString::from(format!("{key}-a{i}-{what}"));
            let bar = rich::attachment_color(&a.color).and_then(crate::theme::parse_color).unwrap_or(p.border());

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
                        TextView::markdown(ElementId::Name(id("text")), markdown::prepare(&a.text))
                            .on_link_click(link_click),
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
            let thumb =
                (!a.thumb_url.is_empty()).then(|| div().p_2().child(self.external_image(&a.thumb_url, 64., 64., &p)));
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
                    .child(div().text_xs().text_color(p.muted()).child(t!(
                        "post.permalink_from",
                        author = self.user_name(&user_id),
                        channel = channel
                    )))
                    .child(div().text_sm().line_clamp(4).child(markdown::plain_text(&message)))
                    .into_any_element(),
            };
            out.push(element);
        }
        out
    }

    /// Pièces jointes d'un message : aperçus pour les images, cartes pour
    /// les autres fichiers.
    fn render_files(
        &self,
        entity: &Entity<Self>,
        files: &[FileInfo],
        key: &SharedString,
        p: &Palette,
    ) -> impl IntoElement {
        let p = *p;
        let mut images = h_flex().gap_2().flex_wrap();
        let mut others = v_flex().gap_1p5();
        for (ix, file) in files.iter().enumerate() {
            let id = SharedString::from(format!("{key}-{ix}"));
            if file.is_image() && !self.failed_previews.contains(&file.id) {
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
                        .child(t!("files.loading_preview"))
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
                let kind = if file.extension.is_empty() {
                    t!("files.file").to_string()
                } else {
                    file.extension.to_uppercase()
                };
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
                                .icon(Icon::new(IconName::ExternalLink))
                                .small()
                                .ghost()
                                .accessibility_label(t!("common.open"))
                                .tooltip(t!("common.open"))
                                .on_click(move |_, window, cx| {
                                    let file = f1.clone();
                                    e1.update(cx, |this, cx| this.open_file(file, window, cx));
                                }),
                        )
                        .child(
                            Button::new(SharedString::from(format!("{key}-{ix}-dl")))
                                .icon(Icon::new(IconName::Download))
                                .small()
                                .ghost()
                                .accessibility_label(t!("common.download"))
                                .tooltip(t!("common.download"))
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
                (_, Some(err)) => div().text_color(p.error).child(t!("composer.upload_failed", error = err)),
                (Some(_), None) => div().text_color(p.muted()).child(human_size(a.size)),
                (None, None) => div().text_color(p.muted()).child(t!("composer.uploading")),
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
                .child(v_flex().min_w_0().child(div().truncate().font_semibold().child(a.name.clone())).child(status))
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
        if self.autocomplete_key(pane, key, window, cx) || (key == "up" && self.edit_last_message(pane, window, cx)) {
            cx.stop_propagation();
        }
    }

    /// Propositions d'autocomplétion, au-dessus de la zone de saisie.
    fn render_autocomplete(&self, pane: Pane, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ac = self.autocomplete.as_ref().filter(|a| a.pane == pane && !a.items.is_empty())?;
        let p = self.palette;
        let title = match ac.kind {
            crate::backend::AcKind::User => t!("autocomplete.people"),
            crate::backend::AcKind::Channel => t!("sidebar.channels"),
            crate::backend::AcKind::Emoji => t!("autocomplete.emojis"),
            crate::backend::AcKind::Command => t!("autocomplete.commands"),
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
                    .flex()
                    .justify_center()
                    .text_color(p.muted())
                    .child(
                        Icon::new(if item.label.starts_with('~') { IconName::Hash } else { IconName::SquareTerminal })
                            .size(px(15.)),
                    )
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
        // Description, puis en-tête (Markdown, liens cliquables) : les deux si la
        // place le permet, chacun tronqué sinon.
        let subtitle: Option<AnyElement> = match (&partner, state) {
            (Some(partner), _) => Some(self.status_label(partner).into_any_element()),
            (None, Some(c)) if !c.channel.purpose.is_empty() || !c.channel.header.is_empty() => {
                let (purpose, header) = (c.channel.purpose.clone(), c.channel.header.clone());
                Some(
                    h_flex()
                        .min_w_0()
                        .gap_2()
                        .when(!purpose.is_empty(), |el| {
                            el.child(div().flex_shrink(1.).min_w(px(60.)).truncate().child(purpose.clone()))
                        })
                        .when(!purpose.is_empty() && !header.is_empty(), |el| {
                            el.child(div().flex_none().w(px(1.)).h(px(11.)).bg(p.border()))
                        })
                        .when(!header.is_empty(), |el| {
                            el.child(
                                div().flex_1().min_w(px(60.)).max_h(px(18.)).overflow_hidden().child(
                                    TextView::markdown("channel-header-text", markdown::prepare(&header))
                                        .max_lines(1)
                                        .on_link_click(|url, _, _, cx| open_link(url, cx)),
                                ),
                            )
                        })
                        .into_any_element(),
                )
            }
            _ => None,
        };
        let badge = state
            .filter(|c| matches!(c.channel.kind.as_str(), "O" | "P"))
            .map(|c| self.channel_badge(&c.channel, 32., p.center_bg));
        let actions = self.channel_id.clone().zip(state).map(|(channel_id, state)| {
            let side = self.side.as_ref().map(|s| s.kind.clone());
            let icon_button = |id: &'static str, icon: IconName, on: bool, tip: SharedString| {
                Button::new(id)
                    .icon(Icon::new(icon))
                    .ghost()
                    .small()
                    .toggled(on)
                    .accessibility_label(tip.clone())
                    .tooltip(tip)
            };
            h_flex()
                .gap_1()
                .child(self.render_search_box(cx))
                .child(div().w(px(1.)).h(px(20.)).mx_1().bg(p.border()))
                .child(
                    icon_button(
                        "hdr-mentions",
                        IconName::AtSign,
                        side == Some(PanelKind::Mentions),
                        t!("header.mentions_tooltip").into(),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.show_mentions(cx))),
                )
                .child(
                    icon_button(
                        "hdr-threads",
                        IconName::MessagesSquare,
                        side == Some(PanelKind::Threads),
                        t!("header.threads_tooltip").into(),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.show_threads(cx))),
                )
                .child(
                    icon_button(
                        "hdr-files",
                        IconName::FolderOpen,
                        side == Some(PanelKind::Files),
                        t!("header.files_tooltip").into(),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.show_files(cx))),
                )
                .child(
                    icon_button(
                        "hdr-members",
                        IconName::Users,
                        self.settings.show_members,
                        if self.settings.show_members { t!("header.hide_members") } else { t!("header.show_members") }
                            .into(),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_members(cx))),
                )
                .child(
                    Button::new("hdr-more")
                        .icon(Icon::new(IconName::Ellipsis))
                        .ghost()
                        .small()
                        .accessibility_label(t!("header.more_tooltip"))
                        .tooltip(t!("header.more_tooltip"))
                        .dropdown_menu_with_anchor(
                            Anchor::TopRight,
                            self.channel_menu(&channel_id, state.muted, true, cx),
                        ),
                )
        });
        // Favori et sourdine : signalés près du nom ; l'étoile reste cliquable.
        let flags = self.channel_id.clone().zip(state).map(|(channel_id, state)| {
            let favorite = self.is_favorite(&channel_id);
            h_flex()
                .gap_0p5()
                .flex_none()
                .child(
                    Button::new("hdr-favorite")
                        .icon(
                            Icon::new(if favorite { IconName::StarFill } else { IconName::Star })
                                .size(px(14.))
                                .text_color(if favorite { p.away } else { p.muted().opacity(0.6) }),
                        )
                        .ghost()
                        .xsmall()
                        .tooltip(if favorite { t!("channel_menu.unfavorite") } else { t!("channel_menu.favorite") })
                        .on_click(cx.listener(move |this, _, _, cx| this.toggle_favorite(channel_id.clone(), cx))),
                )
                .when(state.muted, |el| el.child(Icon::new(IconName::BellOff).size(px(14.)).text_color(p.muted())))
        });

        h_flex()
            .h(px(62.))
            .flex_none()
            .px_5()
            .gap_3()
            .border_b_1()
            .border_color(p.border())
            .bg(linear_gradient(
                90.,
                linear_color_stop(p.sidebar_bg.opacity(0.24), 0.),
                linear_color_stop(p.button_bg.opacity(0.07), 1.),
            ))
            .when(self.viewport_w < super::NARROW_SIDEBAR, |el| {
                el.child(
                    Button::new("hdr-channels")
                        .icon(Icon::new(IconName::Menu))
                        .ghost()
                        .small()
                        .accessibility_label(t!("sidebar.channels"))
                        .tooltip(t!("sidebar.channels"))
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_drawer(super::Drawer::Channels, cx))),
                )
            })
            .when_some(partner.clone(), |el, partner| el.child(self.avatar(&partner, 32., p.center_bg)))
            .children(badge)
            .child(
                v_flex()
                    .id("channel-title")
                    .flex_1()
                    .min_w(px(170.))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, ev: &ClickEvent, window, cx| {
                        this.open_channel_card(ev.position(), window, cx)
                    }))
                    .when(self.side.as_ref().is_some_and(|s| s.kind == PanelKind::Channel), |el| {
                        el.text_color(p.button_bg)
                    })
                    .child(
                        h_flex()
                            .gap_1()
                            .min_w_0()
                            .text_size(px(16.))
                            .font_bold()
                            .child(div().min_w_0().truncate().hover(|s| s.underline()).child(title))
                            .child(Icon::new(IconName::ChevronDown).size(px(14.)).text_color(p.muted()).flex_none())
                            .children(flags),
                    )
                    .when_some(subtitle, |el, sub| {
                        el.child(
                            div()
                                .truncate()
                                .max_h(px(18.))
                                .overflow_hidden()
                                .text_size(px(12.))
                                .text_color(p.muted())
                                .child(sub),
                        )
                    }),
            )
            .when(self.loading_older, |el| {
                el.child(div().text_xs().text_color(p.muted()).child(t!("messages.loading_history")))
            })
            .children(actions.map(|a| a.flex_shrink(1.).min_w_0().justify_end()))
    }

    fn render_chat(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        // Revenu en bas : les messages arrivés entre-temps sont vus.
        if self.unseen_new > 0 && !self.scroller.read(cx).is_scrolled_up() {
            self.unseen_new = 0;
        }
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
                .child(Button::new("dismiss-error").icon(Icon::new(IconName::X)).xsmall().ghost().on_click(
                    cx.listener(|this, _, _, cx| {
                        this.error = None;
                        cx.notify();
                    }),
                ))
        });

        let messages: AnyElement = if !self.loading_posts && self.posts.is_empty() && self.channel_id.is_some() {
            v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_1()
                .child(div().text_size(px(16.)).font_semibold().child(t!("messages.empty_title")))
                .child(div().text_color(p.muted()).child(t!("messages.empty_hint")))
                .into_any_element()
        } else if self.loading_posts && self.posts.is_empty() {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(p.muted())
                .child(t!("common.loading"))
                .into_any_element()
        } else {
            let entity = entity.clone();
            let toast = self.render_unread_toast(cx);
            div()
                .id("messages-region")
                .role(Role::Main)
                .aria_label(t!("a11y.messages"))
                .relative()
                .flex_1()
                .min_h_0()
                .child(
                    MessageScroller::new("messages", self.scroller.clone(), move |ix, _, cx| {
                        entity.read(cx).render_post(&entity, Pane::Channel, ix, cx)
                    })
                    .with_jump_button_label(t!("messages.jump_tooltip"))
                    .with_jump_button_renderer({
                        let (bg, fg) = (p.button_bg, p.button_color);
                        let label: SharedString = match self.unseen_new {
                            0 => t!("messages.jump_to_present").into(),
                            1 => t!("toast.new_below.one").into(),
                            n => t!("toast.new_below.other", count = n).into(),
                        };
                        move |b| b.label(label.clone()).bg(bg).text_color(fg).border_color(bg).shadow_md()
                    })
                    // Pas d'espace imposé entre les lignes : chaque message gère le sien.
                    .with_row_style(StyleRefinement::default().pb(px(0.))),
                )
                .children(toast)
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
                .child(Button::new("dismiss-info").icon(Icon::new(IconName::X)).xsmall().ghost().on_click(cx.listener(
                    |this, _, _, cx| {
                        this.info = None;
                        cx.notify();
                    },
                )))
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
            .children(self.render_bookmarks_bar(cx))
            .children(self.render_offline_banner())
            .children(error_banner)
            .children(info_banner)
            .child(messages)
            .children(self.render_failed_sends(Pane::Channel, cx))
            .children(self.render_dm_warning())
            .child(if self.channel_id.as_deref().is_some_and(|id| self.is_archived(id)) {
                // Canal archivé : lecture seule.
                h_flex()
                    .flex_none()
                    .mx_5()
                    .mb_4()
                    .px_4()
                    .py_3()
                    .gap_2()
                    .rounded(px(8.))
                    .bg(p.center_color.opacity(0.06))
                    .text_sm()
                    .text_color(p.muted())
                    .child(Icon::new(IconName::Archive).size(px(16.)))
                    .child(t!("browse.archived_banner"))
                    .into_any_element()
            } else {
                self.render_composer(Pane::Channel, cx).into_any_element()
            });

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
                        .bg(right_header_bg(&p))
                        .flex_none()
                        .px_5()
                        .gap_2()
                        .border_b_1()
                        .border_color(p.border())
                        .child(div().text_size(px(16.)).font_bold().child(t!("threads.thread")))
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
                .children(self.render_failed_sends(Pane::Thread, cx))
                .child(self.render_composer(Pane::Thread, cx))
        });

        let side_panel = self.render_side_panel(cx);
        let thread_panel: Option<AnyElement> = thread_panel.map(IntoElement::into_any_element);
        let w = self.viewport_w;
        let narrow_sidebar = w < super::NARROW_SIDEBAR;
        let float_panels = w < super::NARROW_PANELS;
        // Les membres restent affichés, sauf quand un fil ou un panneau prend la place.
        let members_panel = if !float_panels && thread_panel.is_none() && side_panel.is_none() {
            self.render_members_panel(false, cx)
        } else {
            None
        };
        // Fenêtre étroite : fil ou panneau flottant à droite, au-dessus des messages.
        let (floating_panel, thread_panel, side_panel) =
            if float_panels { (thread_panel.or(side_panel), None, None) } else { (None, thread_panel, side_panel) };
        let drawer = self.drawer.filter(|d| match d {
            super::Drawer::Channels => narrow_sidebar,
            super::Drawer::Members => float_panels,
        });
        let drawer_el = drawer.map(|d| {
            let content: AnyElement = match d {
                super::Drawer::Channels => h_flex()
                    .h_full()
                    .children(self.render_team_bar(cx))
                    .child(self.render_sidebar(cx))
                    .into_any_element(),
                super::Drawer::Members => {
                    div().h_full().bg(p.center_bg).children(self.render_members_panel(true, cx)).into_any_element()
                }
            };
            let left = d == super::Drawer::Channels;
            div()
                .id("drawer-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(black().opacity(0.3))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.drawer = None;
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .id("drawer")
                        .occlude()
                        .absolute()
                        .top_0()
                        .h_full()
                        .when(left, |el| el.left_0())
                        .when(!left, |el| el.right_0())
                        .shadow_xl()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(content),
                )
                .into_any_element()
        });
        let team_bar = if self.teams.len() >= 2 { TEAM_BAR_WIDTH } else { 0. };
        let layout = h_flex()
            .id("chat-root")
            .track_focus(&self.root_focus)
            .size_full()
            .bg(p.center_bg)
            .on_drag_move(cx.listener(move |this, ev: &DragMoveEvent<SidebarResize>, _, cx| {
                let x = (ev.event.position.x - ev.bounds.left()) / px(1.) - team_bar;
                this.resize_sidebar(x, cx);
            }))
            .on_drop(cx.listener(|this, _: &SidebarResize, _, _| this.end_resize()))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.end_resize()))
            .when(!narrow_sidebar, |el| el.children(self.render_team_bar(cx)).child(self.render_sidebar(cx)))
            .child(main)
            .children(thread_panel)
            .children(side_panel)
            .children(members_panel)
            // Ctrl+K : aller à un canal.
            .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                let k = &ev.keystroke;
                let (ctrl, shift, alt) = (k.modifiers.control, k.modifiers.shift, k.modifiers.alt);
                if (ctrl && !alt && (k.key == "/" || k.key == ":")) || k.key == "f1" {
                    this.open_shortcuts(cx);
                    cx.stop_propagation();
                    return;
                }
                // Historique : Alt+←/→, Cmd+[/] sur macOS (Alt+← y déplace d'un mot).
                let history = if cfg!(target_os = "macos") {
                    k.modifiers.platform && !shift && !alt && (k.key == "[" || k.key == "]")
                } else {
                    alt && !ctrl && !shift && (k.key == "left" || k.key == "right")
                };
                if history {
                    this.navigate_history(k.key == "right" || k.key == "]", window, cx);
                    cx.stop_propagation();
                    return;
                }
                if alt && ctrl && (k.key == "up" || k.key == "down") {
                    this.cycle_team(k.key == "down", cx);
                    cx.stop_propagation();
                    return;
                }
                // F6 : passe d'une zone de saisie à l'autre (messages, fil, recherche).
                if k.key == "f6" && !ctrl && !alt {
                    this.cycle_focus(shift, window, cx);
                    cx.stop_propagation();
                    return;
                }
                if ctrl && shift && !alt && k.key == "a" && this.dialog.is_none() {
                    this.open_settings(window, cx);
                    cx.stop_propagation();
                    return;
                }
                if ctrl && shift && !alt && k.key == "m" {
                    this.show_mentions(cx);
                    cx.stop_propagation();
                    return;
                }
                if ctrl && shift && !alt && k.key == "l" {
                    this.focus_composer(window, cx);
                    cx.stop_propagation();
                    return;
                }
                if k.key == "escape" && shift && !ctrl && this.dialog.is_none() {
                    this.mark_team_read(cx);
                    cx.stop_propagation();
                    return;
                }
                if k.key == "up"
                    && shift
                    && !ctrl
                    && !alt
                    && this.composer.read(cx).focus_handle(cx).is_focused(window)
                    && this.reply_to_last(window, cx)
                {
                    cx.stop_propagation();
                    return;
                }
                if k.key == "escape" && this.dialog.is_some() {
                    this.close_dialog(cx);
                    cx.stop_propagation();
                    return;
                }
                if k.modifiers.control && k.modifiers.shift && !k.modifiers.alt && k.key == "k" {
                    this.open_direct_dialog(window, cx);
                    cx.stop_propagation();
                    return;
                }
                if k.key == "escape" && this.profile.is_some() {
                    this.close_profile(cx);
                    cx.stop_propagation();
                    return;
                }
                if k.key == "escape"
                    && (this.team_card.is_some() || this.add_members.is_some() || this.create.is_some())
                {
                    this.close_create(cx);
                    this.close_team_card(cx);
                    this.close_add_members(cx);
                    cx.stop_propagation();
                    return;
                }
                if k.modifiers.alt && !k.modifiers.control && (k.key == "up" || k.key == "down") {
                    this.cycle_channel(k.key == "down", k.modifiers.shift, window, cx);
                    cx.stop_propagation();
                    return;
                }
                if k.modifiers.control && !k.modifiers.shift && !k.modifiers.alt && k.key == "f" {
                    this.open_search(window, cx);
                    cx.stop_propagation();
                    return;
                }
                if k.modifiers.control && !k.modifiers.shift && !k.modifiers.alt && k.key == "k" {
                    this.open_switcher(window, cx);
                    cx.stop_propagation();
                }
            }));

        div()
            .relative()
            .size_full()
            .child(layout)
            .children(
                floating_panel.map(|panel| {
                    div().absolute().top_0().right_0().h_full().shadow_xl().child(panel).into_any_element()
                }),
            )
            .children(drawer_el)
            .children(self.render_viewer(cx))
            .children(self.render_switcher(cx))
            .children(self.render_dialog(cx))
            .children(self.render_emoji_picker(cx))
            .children(self.render_profile_card(cx))
            .children(self.render_team_card(cx))
            .children(self.render_add_members(cx))
            .children(self.render_create(cx))
    }

    /// Photo en médaillon, statut, poste, adresse, heure locale, bouton message.
    fn profile_body(&self, u: &User, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let uid = u.id.clone();
        let is_me = uid == self.me_id();
        // Déjà dans son message direct : pas de bouton « Envoyer un message ».
        let in_dm = self
            .channel_id
            .as_ref()
            .and_then(|id| self.channels.get(id))
            .and_then(|c| c.channel.direct_partner(self.me_id()))
            .is_some_and(|partner| partner == uid);
        let name = self.user_name(&uid);
        let mut photo = Avatar::new().name(name.clone()).size(px(96.));
        if let Some(image) = self.avatars.get(&uid) {
            photo = photo.src(image.clone());
        }
        let medallion =
            div().relative().flex_none().size(px(106.)).p(px(5.)).rounded_full().bg(p.center_bg).child(photo).child(
                div()
                    .absolute()
                    .right(px(6.))
                    .bottom(px(6.))
                    .size(px(24.))
                    .rounded_full()
                    .border_4()
                    .border_color(p.center_bg)
                    .bg(self.status_color(&uid)),
            );
        let line = |icon: IconName, text: String| {
            h_flex()
                .gap_2()
                .text_sm()
                .child(div().flex_none().text_color(p.muted()).child(Icon::new(icon).size(px(15.))))
                .child(div().min_w_0().truncate().child(text))
        };
        let time = u.time_zone().and_then(|tz| local_time(&tz));
        let entity = cx.entity();
        v_flex()
            .child(div().h(px(60.)).bg(linear_gradient(
                120.,
                linear_color_stop(p.sidebar_bg, 0.),
                linear_color_stop(p.button_bg, 1.),
            )))
            .child(
                v_flex()
                    .px_5()
                    .pb_5()
                    .gap_3()
                    .child(div().mt(-px(53.)).child(medallion))
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(div().text_size(px(18.)).font_bold().child(name))
                            .child(div().text_sm().text_color(p.muted()).child(format!("@{}", u.username)))
                            .when(!u.position.is_empty(), |el| el.child(div().text_sm().child(u.position.clone()))),
                    )
                    .children(
                        self.status_line(&uid, 18.)
                            .map(|line| line.px_3().py_2().rounded(px(8.)).text_sm().bg(p.center_color.opacity(0.06))),
                    )
                    .child(
                        v_flex()
                            .gap_2()
                            .pt_3()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .text_sm()
                                    .child(div().mx(px(3.)).size(px(9.)).rounded_full().bg(self.status_color(&uid)))
                                    .child(self.status_label(&uid)),
                            )
                            .when(!u.email.is_empty(), |el| el.child(line(IconName::Mail, u.email.clone())))
                            .when_some(time, |el, (clock, gap)| {
                                el.child(
                                    h_flex()
                                        .gap_2()
                                        .items_start()
                                        .text_sm()
                                        .child(
                                            div()
                                                .flex_none()
                                                .pt(px(2.))
                                                .text_color(p.muted())
                                                .child(Icon::new(IconName::Clock).size(px(15.))),
                                        )
                                        .child(
                                            v_flex().child(t!("profile.local_time", time = clock)).when(!is_me, |d| {
                                                d.child(div().text_xs().text_color(p.muted()).child(gap))
                                            }),
                                        ),
                                )
                            })
                            .when(u.is_system_admin(), |el| {
                                el.child(line(IconName::Shield, t!("profile.system_admin").to_string()))
                            }),
                    )
                    .when(!is_me && !in_dm, |el| {
                        el.child(
                            Button::new("profile-dm")
                                .primary()
                                .w_full()
                                .icon(Icon::new(IconName::SendHorizontal))
                                .label(t!("profile.send_message"))
                                .on_click(move |_, window, cx| {
                                    let uid = uid.clone();
                                    entity.update(cx, |this, cx| {
                                        this.profile = None;
                                        this.open_direct(uid, window, cx);
                                    });
                                }),
                        )
                    }),
            )
            .into_any_element()
    }

    /// Mini-carte de profil : photo en médaillon, statut, adresse, heure locale.
    fn render_profile_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let card = self.profile.as_ref()?;
        let p = self.palette;
        let at = card.at;
        let body: AnyElement = match self.profile_user() {
            None => div()
                .p_5()
                .text_color(p.muted())
                .child(t!("profile.loading", username = card.username))
                .into_any_element(),
            Some(u) => self.profile_body(u, cx),
        };
        Some(
            div()
                .id("profile-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close_profile(cx)))
                .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| this.close_profile(cx)))
                .child(
                    div()
                        .id("profile-card")
                        .occlude()
                        .absolute()
                        .left(at.x)
                        .top(at.y)
                        .w(px(super::PROFILE_CARD_WIDTH))
                        .rounded(px(12.))
                        .overflow_hidden()
                        .border_1()
                        .border_color(p.border())
                        .bg(p.center_bg)
                        .text_color(p.center_color)
                        .shadow_xl()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(body),
                )
                .into_any_element(),
        )
    }

    /// Panneau des messages épinglés, enregistrés ou des résultats de recherche.
    fn render_side_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let side = self.side.as_ref()?;
        let p = self.palette;
        let entity = cx.entity();
        let (title, subtitle) = match side.kind {
            PanelKind::Pinned => (t!("panels.pinned").to_string(), side.subject.clone()),
            PanelKind::Saved => (t!("panels.saved").to_string(), String::new()),
            PanelKind::Search => (t!("search.placeholder").to_string(), String::new()),
            PanelKind::Threads if self.threads_all => {
                (t!("threads.sidebar").to_string(), t!("threads.all_channels").to_string())
            }
            PanelKind::Threads => (
                t!("panels.threads").to_string(),
                self.channel_id
                    .as_ref()
                    .and_then(|id| self.channels.get(id))
                    .map(|c| self.channel_title(&c.channel))
                    .unwrap_or_default(),
            ),
            PanelKind::Mentions => (t!("panels.mentions").to_string(), t!("panels.all_teams").to_string()),
            PanelKind::Channel => (t!("panels.about").to_string(), String::new()),
            PanelKind::Files => (
                t!("panels.files").to_string(),
                self.channel_id
                    .as_ref()
                    .and_then(|id| self.channels.get(id))
                    .map(|c| self.channel_title(&c.channel))
                    .unwrap_or_default(),
            ),
        };
        let searching = side.kind == PanelKind::Search;
        let body: AnyElement = if side.kind == PanelKind::Threads {
            self.render_threads_list(cx)
        } else if side.kind == PanelKind::Files {
            self.render_files_panel(cx)
        } else if side.kind == PanelKind::Channel {
            self.render_channel_info(cx).unwrap_or_else(|| div().into_any_element())
        } else {
            match &side.posts {
                None => div().p_5().text_color(p.muted()).child(t!("common.loading")).into_any_element(),
                Some(posts) if posts.is_empty() && searching && side.subject.is_empty() => {
                    div().p_5().text_color(p.muted()).child(t!("search.empty_hint")).into_any_element()
                }
                Some(posts) if posts.is_empty() => div()
                    .p_5()
                    .text_color(p.muted())
                    .child(match side.kind {
                        PanelKind::Pinned => t!("panels.pinned_empty"),
                        PanelKind::Saved => t!("panels.saved_empty"),
                        PanelKind::Search => t!("search.no_results"),
                        PanelKind::Threads => "".into(),
                        PanelKind::Mentions => t!("panels.mentions_empty"),
                        PanelKind::Files | PanelKind::Channel => "".into(),
                    })
                    .into_any_element(),
                Some(posts) => v_flex()
                    .id("side-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(posts.iter().map(|post| {
                        let channel = match self.channels.get(&post.channel_id) {
                            Some(c) => self.channel_title(&c.channel),
                            None => self
                                .channel_teams
                                .get(&post.channel_id)
                                .and_then(|t| self.teams.iter().find(|team| &team.id == t))
                                .map(|team| t!("search.other_team", team = team.display_name).to_string())
                                .unwrap_or_default(),
                        };
                        let (entity, jump) = (entity.clone(), post.clone());
                        let custom: Vec<String> = post.metadata.emojis.iter().map(|e| e.name.clone()).collect();
                        v_flex()
                            .id(SharedString::from(format!("side-{}", post.id)))
                            .px_5()
                            .py_3()
                            .gap_1()
                            .cursor_pointer()
                            .hover(|s| s.bg(p.hover()))
                            .child(
                                h_flex()
                                    .gap_1()
                                    .text_xs()
                                    .text_color(p.muted())
                                    .child(self.channel_badge_by_id(&post.channel_id, 16.))
                                    .child(channel),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(self.avatar(&post.user_id, 24., p.center_bg))
                                    .child(div().font_semibold().child(self.user_name(&post.user_id)))
                                    .child(div().text_xs().text_color(p.muted()).child(format!(
                                        "{}, {}",
                                        day_label(post.create_at),
                                        format_hour(post.create_at)
                                    ))),
                            )
                            .child(
                                div().text_size(px(BODY_TEXT)).line_clamp(6).child(
                                    TextView::markdown(
                                        SharedString::from(format!("side-text-{}", post.id)),
                                        markdown::prepare_for(&post.message, &custom, Some(&post.channel_id)),
                                    )
                                    .image_source(emoji_resolver(self.custom_emojis.clone())),
                                ),
                            )
                            .on_click(move |_, window, cx| {
                                let post = jump.clone();
                                entity.update(cx, |this, cx| this.jump_to_post(post, window, cx));
                            })
                    }))
                    .into_any_element(),
            }
        };
        Some(
            v_flex()
                .w(px(THREAD_WIDTH))
                .h_full()
                .flex_none()
                .bg(p.center_bg)
                .text_color(p.center_color)
                .border_l_1()
                .border_color(p.border())
                .child(
                    h_flex()
                        .h(px(62.))
                        .bg(right_header_bg(&p))
                        .flex_none()
                        .px_5()
                        .gap_2()
                        .border_b_1()
                        .border_color(p.border())
                        .child(div().text_size(px(16.)).font_bold().child(title))
                        .child(div().flex_1().truncate().text_size(px(12.)).text_color(p.muted()).child(subtitle))
                        .child(
                            Button::new("close-side")
                                .icon(Icon::new(IconName::X))
                                .small()
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| this.close_side(cx))),
                        ),
                )
                .when(searching && !side.subject.is_empty(), |el| {
                    // Recherche en cours, en jetons : un clic sur × retire le filtre.
                    el.child(
                        h_flex().flex_wrap().gap_1p5().px_4().py_2p5().border_b_1().border_color(p.border()).children(
                            search_tokens(&side.subject).into_iter().enumerate().map(|(i, token)| {
                                let filter = token.contains(':') || token.starts_with('#') || token.starts_with('-');
                                h_flex()
                                    .id(SharedString::from(format!("tok-{i}")))
                                    .gap_1()
                                    .pl_2()
                                    .pr_1()
                                    .py_0p5()
                                    .rounded(px(6.))
                                    .text_xs()
                                    .bg(if filter { p.link.opacity(0.12) } else { p.center_color.opacity(0.08) })
                                    .text_color(if filter { p.link } else { p.center_color })
                                    .child(token)
                                    .child(
                                        div()
                                            .id(SharedString::from(format!("tok-x-{i}")))
                                            .rounded(px(4.))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(p.center_color.opacity(0.12)))
                                            .child(Icon::new(IconName::X).size(px(12.)))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.remove_search_token(i, window, cx)
                                            })),
                                    )
                            }),
                        ),
                    )
                })
                .child(body)
                .into_any_element(),
        )
    }

    /// Liste des fils suivis : message d'origine, participants, réponses non lues.
    fn render_threads_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let Some(all) = &self.threads else {
            return div().p_5().text_color(p.muted()).child(t!("common.loading")).into_any_element();
        };
        let current = self.channel_id.clone().unwrap_or_default();
        let threads: Vec<_> = all
            .iter()
            .filter(|t| {
                if self.threads_all {
                    !self.threads_unread || t.unread_replies > 0
                } else {
                    t.post.channel_id == current
                }
            })
            .collect();
        let toolbar = self.threads_all.then(|| self.render_threads_toolbar(cx));
        if threads.is_empty() {
            let empty = if self.threads_all && self.threads_unread {
                t!("threads.no_unread")
            } else if self.threads_all {
                t!("threads.empty_all")
            } else {
                t!("threads.empty")
            };
            return v_flex().children(toolbar).child(div().p_5().text_color(p.muted()).child(empty)).into_any_element();
        }
        let entity = cx.entity();
        let list =
            v_flex().id("threads-list").flex_1().min_h_0().overflow_y_scroll().children(threads.into_iter().map(|t| {
                let channel =
                    self.channels.get(&t.post.channel_id).map(|c| self.channel_title(&c.channel)).unwrap_or_default();
                let unread = t.unread_replies > 0;
                let (entity, jump) = (entity.clone(), t.post.clone());
                let replies = if t.reply_count == 1 {
                    t!("threads.replies.one")
                } else {
                    t!("threads.replies.other", count = t.reply_count)
                };
                v_flex()
                    .id(SharedString::from(format!("thr-{}", t.id)))
                    .px_5()
                    .py_3()
                    .gap_1()
                    .cursor_pointer()
                    .hover(|s| s.bg(p.hover()))
                    .child(
                        h_flex()
                            .gap_1()
                            .text_xs()
                            .text_color(p.muted())
                            .child(self.channel_badge_by_id(&t.post.channel_id, 16.))
                            .child(div().flex_1().child(channel))
                            .when(unread, |el| {
                                el.child(
                                    div()
                                        .px_1p5()
                                        .rounded_full()
                                        .font_bold()
                                        .bg(if t.unread_mentions > 0 { p.mention_bg } else { p.button_bg })
                                        .text_color(if t.unread_mentions > 0 {
                                            p.mention_color
                                        } else {
                                            p.button_color
                                        })
                                        .child(if t.unread_replies == 1 {
                                            t!("threads.new_replies.one")
                                        } else {
                                            t!("threads.new_replies.other", count = t.unread_replies)
                                        }),
                                )
                            }),
                    )
                    .child(
                        h_flex().gap_2().child(self.avatar(&t.post.user_id, 22., p.center_bg)).child(
                            div()
                                .when(unread, |el| el.font_bold())
                                .when(!unread, |el| el.font_semibold())
                                .child(self.user_name(&t.post.user_id)),
                        ),
                    )
                    .child(div().text_sm().line_clamp(2).child(markdown::plain_text(&t.post.message)))
                    .child(
                        h_flex()
                            .gap_2()
                            .mt_0p5()
                            .text_xs()
                            .text_color(p.muted())
                            .child(h_flex().pl(px(6.)).children(t.participants.iter().take(5).map(|u| {
                                // Avatars empilés, légèrement décalés, cerclés du fond.
                                let mut avatar = Avatar::new().name(self.user_name(&u.id)).size(px(20.));
                                if let Some(image) = self.avatars.get(&u.id) {
                                    avatar = avatar.src(image.clone());
                                }
                                let (entity, uid) = (entity.clone(), u.id.clone());
                                div()
                                    .id(SharedString::from(format!("thr-{}-{}", t.id, u.id)))
                                    .ml(-px(6.))
                                    .p(px(2.))
                                    .rounded_full()
                                    .bg(p.center_bg)
                                    .child(avatar)
                                    .on_click(move |ev, window, cx| {
                                        cx.stop_propagation();
                                        let (uid, at) = (uid.clone(), ev.position());
                                        entity.update(cx, |this, cx| this.open_profile(uid, at, window, cx));
                                    })
                            })))
                            .when(t.participants.len() > 5, |el| el.child(format!("+{}", t.participants.len() - 5)))
                            .child(div().text_color(p.link).font_semibold().child(replies))
                            .child(t!("threads.last_reply", day = day_label_inline(t.last_reply_at))),
                    )
                    .on_click(move |_, window, cx| {
                        let post = jump.clone();
                        entity.update(cx, |this, cx| this.jump_to_post(post, window, cx));
                    })
            }));
        v_flex().flex_1().min_h_0().children(toolbar).child(list).into_any_element()
    }

    /// Liste permanente des membres du canal, par statut (comme Discord).
    fn render_members_panel(&self, in_drawer: bool, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.settings.show_members && !in_drawer {
            return None;
        }
        let channel_id = self.channel_id.as_ref()?;
        let p = self.palette;
        let partner = self.channels.get(channel_id).and_then(|c| c.channel.direct_partner(self.me_id()));
        if let Some(user) = partner.as_ref().and_then(|id| self.users.get(id)) {
            return Some(
                v_flex()
                    .id("dm-profile")
                    .w(px(280.))
                    .h_full()
                    .flex_none()
                    .overflow_y_scroll()
                    .bg(p.center_bg)
                    .text_color(p.center_color)
                    .border_l_1()
                    .border_color(p.border())
                    .child(self.profile_body(user, cx))
                    .into_any_element(),
            );
        }
        let (ids, total) = match self.members.get(channel_id) {
            Some((ids, total)) => (ids.clone(), *total),
            None => (Vec::new(), 0),
        };
        let channel_admins = self.channel_admins.get(channel_id);
        let is_admin = |id: &String| {
            channel_admins.is_some_and(|a| a.contains(id)) || self.users.get(id).is_some_and(|u| u.is_system_admin())
        };
        let status = |id: &String| {
            self.statuses
                .get(id)
                .map(String::as_str)
                .filter(|s| matches!(*s, "online" | "away" | "dnd"))
                .unwrap_or("offline")
        };
        // Groupes désignés par un identifiant stable ; le titre affiché est traduit.
        let groups: [(&str, MemberFilter<'_>); 5] = [
            ("admins", Box::new(|id| is_admin(id))),
            ("online", Box::new(|id| !is_admin(id) && status(id) == "online")),
            ("away", Box::new(|id| !is_admin(id) && status(id) == "away")),
            ("dnd", Box::new(|id| !is_admin(id) && status(id) == "dnd")),
            ("offline", Box::new(|id| !is_admin(id) && status(id) == "offline")),
        ];
        let mut list = v_flex().id("members-list").flex_1().min_h_0().overflow_y_scroll().pb_3();
        for (title, keep) in groups {
            let mut people: Vec<&String> = ids.iter().filter(|id| keep(id)).collect();
            if people.is_empty() {
                continue;
            }
            // Administrateurs : ceux en ligne d'abord.
            people.sort_by_key(|id| (status(id) == "offline", crate::fuzzy::fold(&self.user_name(id))));
            let collapsed = self.collapsed_member_groups.contains(title);
            list = list.child(
                h_flex()
                    .id(SharedString::from(format!("mgrp-{title}")))
                    .flex_none()
                    .h(px(28.))
                    .mt_3()
                    .mx_2()
                    .px_2()
                    .gap_1p5()
                    .rounded(px(6.))
                    .text_size(px(13.))
                    .font_bold()
                    .text_color(p.center_color.opacity(0.85))
                    .cursor_pointer()
                    .hover(|s| s.bg(p.hover()))
                    .child(div().flex_none().w(px(14.)).text_color(p.muted()).child(
                        Icon::new(if collapsed { IconName::ChevronRight } else { IconName::ChevronDown }).size(px(13.)),
                    ))
                    .child(div().flex_none().child(member_group_title(title)))
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(11.))
                            .font_semibold()
                            .text_color(p.muted())
                            .child(people.len().to_string()),
                    )
                    .child(div().flex_1())
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_member_group(title, cx))),
            );
            if collapsed {
                continue;
            }
            let manage = self.can_manage_members(channel_id);
            for id in people {
                let user_id = id.clone();
                let offline = status(id) == "offline";
                let menu = (manage && id != self.me_id()).then(|| self.member_menu(channel_id, id, cx));
                let role = (title == "admins").then(|| -> SharedString {
                    if self.users.get(id).is_some_and(|u| u.is_system_admin()) {
                        t!("members.system_admin").into()
                    } else {
                        t!("members.channel_admin").into()
                    }
                });
                let row = h_flex()
                    .id(SharedString::from(format!("mbr-{id}")))
                    .flex_none()
                    .gap_2()
                    .mx_2()
                    .px_2()
                    .py_1()
                    .rounded(px(6.))
                    .when(offline, |el| el.opacity(0.55))
                    .cursor_pointer()
                    .hover(|s| s.bg(p.hover()))
                    .child(self.avatar(id, 28., p.center_bg))
                    .child(
                        v_flex()
                            .min_w_0()
                            .child(
                                h_flex()
                                    .gap_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .truncate()
                                            .text_sm()
                                            .font_semibold()
                                            .when(role.is_some(), |d| d.text_color(group_color("admins", &p)))
                                            .child(self.user_name(id)),
                                    )
                                    .children(self.status_emoji(id, 14., cx)),
                            )
                            .when_some(self.users.get(id).map(|u| u.username.clone()), |el, name| {
                                el.child(div().truncate().text_xs().text_color(p.muted()).child(format!("@{name}")))
                            }),
                    )
                    .when_some(role, |el, role| {
                        el.child(div().flex_1()).child(
                            div()
                                .flex_none()
                                .text_color(p.muted())
                                .child(Icon::new(IconName::Shield).size(px(14.)))
                                .id(SharedString::from(format!("mbr-role-{id}")))
                                .tooltip(move |window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(role.clone()).build(window, cx)
                                }),
                        )
                    })
                    .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                        this.open_profile(user_id.clone(), ev.position(), window, cx)
                    }));
                list = list.child(match menu {
                    Some(menu) => row.context_menu(menu).into_any_element(),
                    None => row.into_any_element(),
                });
            }
        }
        let hidden = total - ids.len() as i64;
        if hidden > 0 {
            list = list.child(
                div().px_4().pt_3().text_xs().text_color(p.muted()).child(t!("members.hidden_count", count = hidden)),
            );
        }
        Some(
            v_flex()
                .w(px(240.))
                .h_full()
                .flex_none()
                .bg(p.center_color.opacity(0.02))
                .text_color(p.center_color)
                .border_l_1()
                .border_color(p.border())
                .child(
                    h_flex()
                        .h(px(62.))
                        .bg(right_header_bg(&p))
                        .flex_none()
                        .px_4()
                        .gap_2()
                        .border_b_1()
                        .border_color(p.border())
                        .child(div().text_size(px(15.)).font_bold().child(t!("members.title")))
                        .when(total > 0, |el| el.child(div().text_sm().text_color(p.muted()).child(total.to_string()))),
                )
                .when(ids.is_empty(), |el| el.child(div().p_4().text_color(p.muted()).child(t!("common.loading"))))
                .child(list)
                .into_any_element(),
        )
    }

    /// Bandeau d'orthographe au-dessus de la saisie : premier mot inconnu,
    /// propositions cliquables, « Ajouter au dictionnaire ».
    fn render_spelling(&self, pane: Pane, cx: &mut Context<Self>) -> Option<AnyElement> {
        let list = self.misspellings.get(&pane)?;
        let first = list.first()?;
        let p = self.palette;
        let more = list.len() - 1;
        let word = first.word.clone();
        Some(
            h_flex()
                .flex_wrap()
                .gap_1p5()
                .px_3()
                .py_1p5()
                .text_xs()
                .child(Icon::new(IconName::SpellCheck).size(px(14.)).text_color(p.error))
                .child(div().text_color(p.muted()).child(t!("spelling.label")))
                .child(div().font_semibold().text_color(p.error).child(t!("spelling.word", word = word)))
                .children(first.suggestions.iter().enumerate().map(|(i, s)| {
                    let (w, s2) = (word.clone(), s.clone());
                    div()
                        .id(SharedString::from(format!("sp-{pane:?}-{i}")))
                        .px_2()
                        .py_0p5()
                        .rounded_full()
                        .bg(p.button_bg.opacity(0.12))
                        .text_color(p.button_bg)
                        .font_semibold()
                        .cursor_pointer()
                        .hover(|st| st.bg(p.button_bg.opacity(0.2)))
                        .child(s.clone())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.correct_spelling(pane, w.clone(), s2.clone(), window, cx)
                        }))
                }))
                .when(first.suggestions.is_empty(), |el| {
                    el.child(div().text_color(p.muted()).child(t!("spelling.no_suggestions")))
                })
                .child(div().flex_1())
                .when(more > 0, |el| {
                    el.child(div().text_color(p.muted()).child(if more == 1 {
                        t!("spelling.more.one")
                    } else {
                        t!("spelling.more.other", count = more)
                    }))
                })
                .child(
                    div()
                        .id(SharedString::from(format!("sp-learn-{pane:?}")))
                        .text_color(p.link)
                        .cursor_pointer()
                        .hover(|st| st.underline())
                        .child(t!("spelling.add_to_dictionary"))
                        .on_click(cx.listener(move |this, _, _, cx| this.learn_word(pane, word.clone(), cx))),
                )
                .into_any_element(),
        )
    }

    /// Pastilles des @mentions et #étiquettes dans le texte des messages.
    fn tag_plugin(&self, entity: &Entity<Self>) -> TagPlugin {
        TagPlugin {
            me: self.me.as_ref().map(|u| u.username.clone()).unwrap_or_default(),
            keywords: markdown::keywords(),
            palette: self.palette,
            app: entity.downgrade(),
        }
    }

    /// Champ de recherche permanent de l'en-tête, avec ses propositions.
    fn render_search_box(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        // Panneau ou fil ouvert : une loupe ; le champ apparaît dans les propositions.
        let compact = self.side.is_some() || self.thread.is_some();
        if compact {
            return div()
                .id("search-box")
                .relative()
                .flex_none()
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                    if ev.keystroke.key == "escape" && this.search_open {
                        this.search_open = false;
                        window.blur(cx);
                        cx.notify();
                    }
                }))
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    if this.search_open {
                        this.search_open = false;
                        cx.notify();
                    }
                }))
                .child(
                    Button::new("hdr-search")
                        .icon(Icon::new(IconName::Search))
                        .ghost()
                        .small()
                        .toggled(self.search_open)
                        .accessibility_label(t!("search.tooltip"))
                        .tooltip(t!("search.tooltip"))
                        .on_click(cx.listener(|this, _, window, cx| this.open_search(window, cx))),
                )
                .when(self.search_open, |el| el.child(deferred(self.render_search_dropdown(cx)).with_priority(2)))
                .into_any_element();
        }
        div()
            .id("search-box")
            .relative()
            .w(px(250.))
            .min_w(px(120.))
            .flex_shrink(1.)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                if ev.keystroke.key == "escape" && this.search_open {
                    this.search_open = false;
                    window.blur(cx);
                    cx.notify();
                }
            }))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                if this.search_open {
                    this.search_open = false;
                    cx.notify();
                }
            }))
            .child({
                let open = self.search_open;
                let empty = self.search_input.read(cx).value().is_empty();
                h_flex()
                    .id("search-field")
                    .h(px(32.))
                    .pl_3()
                    .pr_1p5()
                    .gap_2()
                    .rounded_full()
                    .border_1()
                    .cursor_text()
                    .map(|el| {
                        if open {
                            el.bg(p.center_bg).border_color(p.button_bg.opacity(0.7)).shadow_sm()
                        } else {
                            el.bg(p.center_color.opacity(0.06))
                                .border_color(transparent_black())
                                .hover(|s| s.bg(p.center_color.opacity(0.09)))
                        }
                    })
                    .child(Icon::new(IconName::Search).size(px(15.)).text_color(if open {
                        p.button_bg
                    } else {
                        p.muted()
                    }))
                    .child(div().flex_1().min_w_0().child(Input::new(&self.search_input).appearance(false).small()))
                    .when(!open && empty, |el| {
                        el.child(
                            div()
                                .flex_none()
                                .px_1p5()
                                .rounded(px(5.))
                                .border_1()
                                .border_color(p.center_color.opacity(0.14))
                                .text_size(px(10.5))
                                .text_color(p.muted())
                                .child("Ctrl F"),
                        )
                    })
                    .when(!empty, |el| {
                        el.child(
                            div()
                                .id("search-clear")
                                .flex_none()
                                .p_1()
                                .rounded_full()
                                .cursor_pointer()
                                .text_color(p.muted())
                                .hover(|s| s.bg(p.center_color.opacity(0.1)))
                                .child(Icon::new(IconName::X).size(px(13.)))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.search_input.update(cx, |s, cx| s.set_value("", window, cx));
                                    cx.notify();
                                })),
                        )
                    })
                    .on_click(cx.listener(|this, _, window, cx| this.open_search(window, cx)))
            })
            .when(self.search_open, |el| el.child(deferred(self.render_search_dropdown(cx)).with_priority(2)))
            .into_any_element()
    }

    /// Propositions : filtres, suggestions pour ce canal, étiquettes, recherches récentes.
    fn render_search_dropdown(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let value = self.search_input.read(cx).value().to_string();
        let word = value.rsplit(' ').next().unwrap_or("").to_string();
        let section = |title: SharedString| {
            div().px_2().pt_2().pb_1().text_xs().font_semibold().text_color(p.muted()).child(title)
        };
        // Une ligne cliquable qui insère un filtre (ou lance une recherche).
        let row =
            |id: String, icon: AnyElement, label: String, hint: String, action: SearchPick, cx: &mut Context<Self>| {
                h_flex()
                    .id(SharedString::from(id))
                    .gap_2()
                    .px_2()
                    .py_1p5()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|s| s.bg(p.hover()))
                    .child(div().flex_none().w(px(22.)).flex().justify_center().text_color(p.muted()).child(icon))
                    .child(div().flex_1().min_w_0().truncate().text_sm().child(label))
                    .child(div().flex_none().text_xs().text_color(p.muted()).child(hint))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            match &action {
                                SearchPick::Insert(t) => this.insert_search_token(t.clone(), window, cx),
                                SearchPick::Run(t) => {
                                    this.search_for(t.clone(), window, cx);
                                    window.blur(cx);
                                }
                            }
                        }),
                    )
            };
        let icon = |name: IconName| Icon::new(name).size(px(15.)).into_any_element();
        let mut list = v_flex().p_1();
        let lower = crate::fuzzy::fold(&word);

        if let Some(prefix) = word.strip_prefix("from:").or_else(|| word.strip_prefix('@')) {
            // Personnes : les connues dont le nom ou l'identifiant correspond.
            let prefix = crate::fuzzy::fold(prefix);
            let mut people: Vec<&User> = self
                .users
                .values()
                .filter(|u| {
                    crate::fuzzy::fold(&u.username).starts_with(&prefix)
                        || crate::fuzzy::fold(&self.user_name(&u.id)).contains(&prefix)
                })
                .collect();
            people.sort_by_key(|u| crate::fuzzy::fold(&u.username));
            list = list.child(section(t!("search.from_section").into()));
            for u in people.into_iter().take(7) {
                let avatar = self.avatar(&u.id, 20., p.center_bg).into_any_element();
                list = list.child(row(
                    format!("sg-u-{}", u.id),
                    avatar,
                    self.user_name(&u.id),
                    format!("@{}", u.username),
                    SearchPick::Insert(format!("from:{}", u.username)),
                    cx,
                ));
            }
        } else if let Some(prefix) = word.strip_prefix("in:").or_else(|| word.strip_prefix('~')) {
            let prefix = crate::fuzzy::fold(prefix);
            let mut channels: Vec<_> = self
                .channels
                .values()
                .filter(|c| !c.channel.is_direct() && !c.channel.is_group())
                .filter(|c| {
                    crate::fuzzy::fold(&c.channel.name).starts_with(&prefix)
                        || crate::fuzzy::fold(&c.channel.display_name).contains(&prefix)
                })
                .collect();
            channels.sort_by_key(|c| std::cmp::Reverse(c.channel.recency()));
            list = list.child(section(t!("search.in_section").into()));
            for c in channels.into_iter().take(7) {
                let lock = if c.channel.kind == "P" { IconName::Lock } else { IconName::Hash };
                list = list.child(row(
                    format!("sg-c-{}", c.channel.id),
                    icon(lock),
                    self.channel_title(&c.channel),
                    format!("~{}", c.channel.name),
                    SearchPick::Insert(format!("in:{}", c.channel.name)),
                    cx,
                ));
            }
        } else if word.starts_with('#') && word.len() > 1 {
            list = list.child(section(t!("search.hashtags").into()));
            for tag in
                self.recent_hashtags().into_iter().filter(|t| crate::fuzzy::fold(t).starts_with(&lower[1..])).take(7)
            {
                list = list.child(row(
                    format!("sg-h-{tag}"),
                    icon(IconName::Hash),
                    tag.clone(),
                    String::new(),
                    SearchPick::Insert(format!("#{tag}")),
                    cx,
                ));
            }
        } else {
            // Filtres de la syntaxe Mattermost, en pastilles.
            let filters: [(&str, SharedString, &str); 6] = [
                ("from:", t!("search.filter_from").into(), "sg-f-from"),
                ("in:", t!("search.filter_in").into(), "sg-f-in"),
                ("on:", t!("search.filter_on").into(), "sg-f-on"),
                ("after:", t!("search.filter_after").into(), "sg-f-after"),
                ("before:", t!("search.filter_before").into(), "sg-f-before"),
                ("-", t!("search.filter_exclude").into(), "sg-f-not"),
            ];
            list = list.child(section(t!("search.filters").into())).child(
                h_flex().flex_wrap().gap_1p5().px_2().pb_1().children(filters.into_iter().map(|(token, label, id)| {
                    let token = token.to_string();
                    h_flex()
                        .id(id)
                        .gap_1()
                        .px_2()
                        .py_0p5()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(p.border())
                        .cursor_pointer()
                        .text_xs()
                        .hover(|s| s.bg(p.hover()))
                        .child(div().font_semibold().text_color(p.link).child(token.clone()))
                        .child(div().text_color(p.muted()).child(label))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.insert_search_token(token.clone(), window, cx);
                            }),
                        )
                })),
            );

            // Suggestions toutes prêtes pour ce canal et cette personne.
            list = list.child(section(t!("search.suggestions").into()));
            let current = self.channel_id.as_ref().and_then(|id| self.channels.get(id));
            if let Some(c) = current {
                match c.channel.direct_partner(self.me_id()).and_then(|id| self.users.get(&id)) {
                    Some(partner) => {
                        list = list.child(row(
                            "sg-s-partner".into(),
                            icon(IconName::Users),
                            t!("search.messages_from", name = self.user_name(&partner.id)).into(),
                            format!("from:{}", partner.username),
                            SearchPick::Insert(format!("from:{}", partner.username)),
                            cx,
                        ))
                    }
                    None if !c.channel.is_group() => {
                        list = list.child(row(
                            "sg-s-here".into(),
                            icon(IconName::Hash),
                            t!("search.in_channel", channel = self.channel_title(&c.channel)).into(),
                            format!("in:{}", c.channel.name),
                            SearchPick::Insert(format!("in:{}", c.channel.name)),
                            cx,
                        ))
                    }
                    None => {}
                }
            }
            if let Some(me) = &self.me {
                list = list.child(row(
                    "sg-s-mine".into(),
                    icon(IconName::Pencil),
                    t!("search.my_messages").into(),
                    format!("from:{}", me.username),
                    SearchPick::Insert(format!("from:{}", me.username)),
                    cx,
                ));
            }
            let today = chrono::Local::now().date_naive();
            list = list
                .child(row(
                    "sg-s-today".into(),
                    icon(IconName::Clock),
                    t!("dates.today").into(),
                    format!("on:{today}"),
                    SearchPick::Insert(format!("on:{today}")),
                    cx,
                ))
                .child(row(
                    "sg-s-week".into(),
                    icon(IconName::Clock),
                    t!("search.last_7_days").into(),
                    format!("after:{}", today - chrono::Duration::days(8)),
                    SearchPick::Insert(format!("after:{}", today - chrono::Duration::days(8))),
                    cx,
                ));

            // Étiquettes vues récemment dans le canal : un clic les recherche.
            let tags = self.recent_hashtags();
            if !tags.is_empty() {
                list = list.child(section(t!("search.channel_hashtags").into())).child(
                    h_flex().flex_wrap().gap_1p5().px_2().pb_1().children(tags.into_iter().take(8).map(|tag| {
                        let terms = format!("#{tag}");
                        div()
                            .id(SharedString::from(format!("sg-tag-{tag}")))
                            .px_2()
                            .py_0p5()
                            .rounded_full()
                            .bg(p.link.opacity(0.12))
                            .text_color(p.link)
                            .text_xs()
                            .font_semibold()
                            .cursor_pointer()
                            .child(terms.clone())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.search_for(terms.clone(), window, cx);
                                    window.blur(cx);
                                }),
                            )
                    })),
                );
            }

            // Recherches récentes.
            if !self.settings.recent_searches.is_empty() {
                list = list.child(
                    h_flex().child(section(t!("search.recent").into())).child(div().flex_1()).child(
                        div()
                            .id("sg-forget")
                            .px_2()
                            .pt_2()
                            .text_xs()
                            .text_color(p.link)
                            .cursor_pointer()
                            .hover(|s| s.underline())
                            .child(t!("search.clear"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.forget_searches(cx);
                                }),
                            ),
                    ),
                );
                for (i, terms) in self.settings.recent_searches.clone().into_iter().enumerate() {
                    list = list.child(row(
                        format!("sg-r-{i}"),
                        icon(IconName::Clock),
                        terms.clone(),
                        String::new(),
                        SearchPick::Run(terms),
                        cx,
                    ));
                }
            }
        }

        let compact = self.side.is_some() || self.thread.is_some();
        v_flex()
            .id("search-dropdown")
            .occlude()
            .absolute()
            .top(px(38.))
            .right_0()
            .w(px(380.))
            .max_h(px(520.))
            .overflow_y_scroll()
            .rounded(px(10.))
            .border_1()
            .border_color(p.border())
            .bg(p.center_bg)
            .text_color(p.center_color)
            .shadow_lg()
            .when(compact, |el| {
                el.child(
                    div().p_2().border_b_1().border_color(p.border()).child(
                        Input::new(&self.search_input)
                            .small()
                            .prefix(Icon::new(IconName::Search).size(px(14.)).text_color(p.muted()))
                            .cleanable(true),
                    ),
                )
            })
            .child(list)
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_t_1()
                    .border_color(p.border())
                    .text_xs()
                    .text_color(p.muted())
                    .child(t!("search.syntax_hint")),
            )
            .into_any_element()
    }

    /// Étiquettes des messages chargés du canal, les plus récentes d'abord.
    fn recent_hashtags(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for view in self.posts.iter().rev() {
            for tag in markdown::hashtags(&view.post.message) {
                if !out.iter().any(|t| t.eq_ignore_ascii_case(&tag)) {
                    out.push(tag);
                }
            }
            if out.len() >= 12 {
                break;
            }
        }
        out
    }

    /// Fichiers du canal : recherche du serveur et messages connus, sans doublon.
    fn channel_file_list(&self) -> Vec<FileInfo> {
        let Some(channel_id) = &self.channel_id else { return Vec::new() };
        let mut files: Vec<FileInfo> = self.channel_files.get(channel_id).cloned().unwrap_or_default();
        for view in &self.posts {
            for f in &view.post.metadata.files {
                if !files.iter().any(|g| g.id == f.id) {
                    let mut f = f.clone();
                    f.post_id = view.post.id.clone();
                    f.channel_id = view.post.channel_id.clone();
                    f.user_id = view.post.user_id.clone();
                    f.create_at = view.post.create_at;
                    files.push(f);
                }
            }
        }
        files.sort_by_key(|x| std::cmp::Reverse(x.create_at));
        files
    }

    /// Liens des messages chargés du canal (avec le titre de leur aperçu s'il y en a un).
    fn channel_links(&self) -> Vec<ChannelLink> {
        let mut out: Vec<ChannelLink> = Vec::new();
        for view in self.posts.iter().rev() {
            let post = &view.post;
            let titles: HashMap<String, String> = post
                .metadata
                .embeds
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| {
                    let url = e.get("url")?.as_str()?.to_string();
                    let title = e.pointer("/data/title").and_then(|t| t.as_str()).unwrap_or_default().to_string();
                    Some((url, title))
                })
                .collect();
            for url in extract_urls(&post.message) {
                if out.iter().any(|l| l.url == url) {
                    continue;
                }
                let title = titles.get(&url).cloned().filter(|t| !t.is_empty());
                out.push(ChannelLink { title, url, user_id: post.user_id.clone(), at: post.create_at });
            }
        }
        out
    }

    /// Panneau « Fichiers » : onglets Médias, Fichiers, Liens.
    fn render_files_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let entity = cx.entity();
        // Filtre : nom du fichier, adresse ou titre du lien.
        let needle = crate::fuzzy::fold(self.files_filter.read(cx).value().trim());
        let keep = |text: &str| needle.is_empty() || crate::fuzzy::fold(text).contains(&needle);
        let files: Vec<FileInfo> = self.channel_file_list().into_iter().filter(|f| keep(&f.name)).collect();
        let (media, documents): (Vec<FileInfo>, Vec<FileInfo>) =
            files.into_iter().partition(|f| f.is_image() || f.is_video());
        let links: Vec<ChannelLink> = self
            .channel_links()
            .into_iter()
            .filter(|l| keep(&format!("{} {}", l.url, l.title.as_deref().unwrap_or_default())))
            .collect();
        let loaded = self.channel_id.as_ref().is_some_and(|id| self.channel_files.contains_key(id));

        let tab = |t: FilesTab, label: SharedString, count: usize, cx: &mut Context<Self>| {
            let active = self.files_tab == t;
            h_flex()
                .id(SharedString::from(format!("ftab-{t:?}")))
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
                .child(
                    div().px_1p5().rounded_full().text_xs().bg(p.center_color.opacity(0.08)).child(count.to_string()),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.set_files_tab(t, cx)))
        };
        let tabs = h_flex()
            .px_3()
            .border_b_1()
            .border_color(p.border())
            .child(tab(FilesTab::Media, t!("files.tab_media").into(), media.len(), cx))
            .child(tab(FilesTab::Documents, t!("panels.files").into(), documents.len(), cx))
            .child(tab(FilesTab::Links, t!("files.tab_links").into(), links.len(), cx));
        let search = div().px_4().py_2().border_b_1().border_color(p.border()).child(
            Input::new(&self.files_filter)
                .small()
                .prefix(Icon::new(IconName::Search).size(px(13.)).text_color(p.muted()))
                .cleanable(true),
        );

        let empty = |text: SharedString| div().p_5().text_color(p.muted()).child(text).into_any_element();
        let meta = |user_id: &str, at: i64| format!("{} · {}", self.user_name(user_id), day_label_inline(at));

        let body: AnyElement = match self.files_tab {
            FilesTab::Media if media.is_empty() => {
                empty(if loaded { t!("files.no_media") } else { t!("common.loading") }.into())
            }
            FilesTab::Media => {
                // Galerie justifiée : chaque ligne remplit la largeur, les images
                // gardent leurs proportions (rien n'est rogné), groupées par mois.
                let mut gallery = v_flex().gap_1p5();
                let mut month = String::new();
                let mut group: Vec<FileInfo> = Vec::new();
                let flush = |gallery: Div, title: &str, group: &mut Vec<FileInfo>, cx: &mut Context<Self>| {
                    if group.is_empty() {
                        return gallery;
                    }
                    let gallery = gallery.child(
                        div().pt_2().pb_0p5().text_xs().font_semibold().text_color(p.muted()).child(title.to_string()),
                    );
                    let rows = justify_rows(group, MEDIA_ROW_WIDTH, MEDIA_ROW_HEIGHT, 6.);
                    group.clear();
                    gallery.children(rows.into_iter().map(|row| {
                        h_flex().gap(px(6.)).children(row.into_iter().map(|(f, w, h)| self.media_tile(f, w, h, cx)))
                    }))
                };
                for f in media {
                    let m = month_label(f.create_at);
                    if m != month {
                        gallery = flush(gallery, &month, &mut group, cx);
                        month = m;
                    }
                    group.push(f);
                }
                gallery = flush(gallery, &month, &mut group, cx);
                div()
                    .id("media-grid")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_3()
                    .pb_3()
                    .child(gallery)
                    .into_any_element()
            }
            FilesTab::Documents if documents.is_empty() => {
                empty(if loaded { t!("files.no_files") } else { t!("common.loading") }.into())
            }
            FilesTab::Documents => v_flex()
                .id("docs-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(documents.into_iter().map(|f| {
                    let (e1, e2, f1, f2) = (entity.clone(), entity.clone(), f.clone(), f.clone());
                    h_flex()
                        .id(SharedString::from(format!("doc-{}", f.id)))
                        .gap_3()
                        .px_4()
                        .py_2p5()
                        .hover(|s| s.bg(p.hover()))
                        .child(div().text_size(px(24.)).child(file_icon(&f.extension)))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(div().truncate().text_sm().font_semibold().child(f.name.clone()))
                                .child(div().truncate().text_xs().text_color(p.muted()).child(format!(
                                    "{} · {}",
                                    f.human_size(),
                                    meta(&f.user_id, f.create_at)
                                ))),
                        )
                        .child(
                            Button::new(SharedString::from(format!("doc-open-{}", f.id)))
                                .icon(Icon::new(IconName::ExternalLink))
                                .ghost()
                                .small()
                                .accessibility_label(t!("common.open"))
                                .tooltip(t!("common.open"))
                                .on_click(move |_, window, cx| {
                                    let file = f1.clone();
                                    e1.update(cx, |this, cx| this.open_file(file, window, cx));
                                }),
                        )
                        .child(
                            Button::new(SharedString::from(format!("doc-dl-{}", f.id)))
                                .icon(Icon::new(IconName::Download))
                                .ghost()
                                .small()
                                .accessibility_label(t!("common.download"))
                                .tooltip(t!("common.download"))
                                .on_click(move |_, _, cx| {
                                    let file = f2.clone();
                                    e2.update(cx, |this, cx| this.download(file, false, cx));
                                }),
                        )
                }))
                .into_any_element(),
            FilesTab::Links if links.is_empty() => empty(t!("files.no_links").into()),
            FilesTab::Links => v_flex()
                .id("links-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(links.into_iter().enumerate().map(|(i, l)| {
                    let domain = l
                        .url
                        .split("://")
                        .nth(1)
                        .and_then(|r| r.split('/').next())
                        .unwrap_or(&l.url)
                        .trim_start_matches("www.")
                        .to_string();
                    let url = l.url.clone();
                    h_flex()
                        .id(SharedString::from(format!("link-{i}")))
                        .gap_3()
                        .px_4()
                        .py_2p5()
                        .cursor_pointer()
                        .hover(|s| s.bg(p.hover()))
                        .child(
                            div()
                                .flex_none()
                                .size(px(32.))
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(p.link.opacity(0.12))
                                .text_color(p.link)
                                .child(Icon::new(IconName::Link2).size(px(16.))),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .truncate()
                                        .text_sm()
                                        .font_semibold()
                                        .child(l.title.clone().unwrap_or(domain.clone())),
                                )
                                .child(div().truncate().text_xs().text_color(p.link).child(l.url.clone()))
                                .child(div().truncate().text_xs().text_color(p.muted()).child(meta(&l.user_id, l.at))),
                        )
                        .on_click(move |_, _, cx| open_link(&url, cx))
                }))
                .into_any_element(),
        };
        v_flex().flex_1().min_h_0().child(tabs).child(search).child(body).into_any_element()
    }

    /// Pastille colorée d'un canal (lettres de son nom), cadenas si privé.
    fn channel_badge(&self, channel: &Channel, size: f32, ring: Hsla) -> AnyElement {
        let name = self.channel_title(channel);
        let color = badge_color(&channel.name);
        let radius = px((size * 0.28).round());
        let face = div()
            .size(px(size))
            .flex_none()
            .rounded(radius)
            .flex()
            .items_center()
            .justify_center()
            .bg(linear_gradient(
                160.,
                linear_color_stop(shade(color, 0.0, 0.06), 0.),
                linear_color_stop(shade(color, 0.03, -0.06), 1.),
            ))
            .text_color(white())
            .text_size(px((size * 0.5).round()))
            .font_bold();
        let face = if channel.kind == "G" {
            face.child(Icon::new(IconName::Users).size(px(size * 0.6)))
        } else {
            face.child(abbreviation(&name))
        };
        let lock_size = (size * 0.5).max(10.);
        div()
            .relative()
            .flex_none()
            .size(px(size))
            .child(face)
            .when(channel.kind == "P", |el| {
                el.child(
                    div()
                        .absolute()
                        .right(-px(3.))
                        .bottom(-px(3.))
                        .size(px(lock_size))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .border_1()
                        .border_color(ring)
                        .bg(ring)
                        .child(Icon::new(IconName::Lock).size(px(lock_size - 4.)).text_color(color)),
                )
            })
            .into_any_element()
    }

    /// Pastille d'un canal connu, sinon le « # » habituel.
    fn channel_badge_by_id(&self, channel_id: &str, size: f32) -> AnyElement {
        match self.channels.get(channel_id) {
            Some(c) if c.channel.direct_partner(self.me_id()).is_none() => {
                self.channel_badge(&c.channel, size, self.palette.center_bg)
            }
            _ => Icon::new(IconName::Hash).size(px(12.)).into_any_element(),
        }
    }

    /// Vignette d'image ou de vidéo, aux proportions du fichier ; nom au survol.
    fn media_tile(&self, f: FileInfo, w: f32, h: f32, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let entity = cx.entity();
        let face: AnyElement = match self.previews.get(&f.id) {
            Some(image) if f.is_image() => {
                img(image.clone()).size_full().object_fit(ObjectFit::Cover).into_any_element()
            }
            _ => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(26.))
                .child(file_icon(&f.extension))
                .into_any_element(),
        };
        let group = SharedString::from(format!("media-g-{}", f.id));
        let name = f.name.clone();
        div()
            .id(SharedString::from(format!("media-{}", f.id)))
            .group(group.clone())
            .relative()
            .w(px(w))
            .h(px(h))
            .flex_none()
            .rounded(px(6.))
            .overflow_hidden()
            .bg(p.center_color.opacity(0.06))
            .cursor_pointer()
            .child(face)
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .px_2()
                    .pt_4()
                    .pb_1()
                    .invisible()
                    .group_hover(group, |s| s.visible())
                    .bg(linear_gradient(
                        180.,
                        linear_color_stop(black().opacity(0.0), 0.),
                        linear_color_stop(black().opacity(0.65), 1.),
                    ))
                    .text_color(white())
                    .text_xs()
                    .child(div().truncate().child(name)),
            )
            .on_click(move |_, window, cx| {
                let file = f.clone();
                entity.update(cx, |this, cx| this.open_file(file, window, cx));
            })
            .into_any_element()
    }

    /// Carte de l'équipe : logo, nom, type, membres, description (modifiable).
    fn render_team_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editing = self.team_card?;
        let team = self.team_id.as_ref().and_then(|id| self.teams.iter().find(|t| &t.id == id))?;
        let p = self.palette;
        let info = self.team_info.get(&team.id).copied();
        let initials: String = team
            .display_name
            .split_whitespace()
            .filter_map(|w| w.chars().next())
            .take(2)
            .collect::<String>()
            .to_uppercase();
        let logo: AnyElement = match self.team_icons.get(&team.id) {
            Some(icon) => div()
                .size(px(64.))
                .rounded(px(16.))
                .bg(white())
                .p(px(6.))
                .child(img(icon.clone()).size_full().object_fit(ObjectFit::Contain))
                .into_any_element(),
            None => div()
                .size(px(64.))
                .rounded(px(16.))
                .flex()
                .items_center()
                .justify_center()
                .bg(linear_gradient(135., linear_color_stop(p.button_bg, 0.), linear_color_stop(p.sidebar_bg, 1.)))
                .text_color(white())
                .text_size(px(24.))
                .font_bold()
                .child(initials)
                .into_any_element(),
        };
        let kind = if team.team_type == "O" { t!("team.open") } else { t!("team.invite_only") };
        let members = match info {
            Some(i) if i.total == 1 => t!("team.members.one", active = i.active).to_string(),
            Some(i) => t!("team.members.other", count = i.total, active = i.active).to_string(),
            None => t!("common.loading").to_string(),
        };
        let since =
            (team.create_at > 0).then(|| t!("team.created", day = day_label_inline(team.create_at)).to_string());
        let admin = info.is_some_and(|i| i.admin);
        let description: AnyElement = if editing {
            v_flex()
                .gap_2()
                .child(Input::new(&self.team_desc_input))
                .child(
                    h_flex()
                        .gap_2()
                        .justify_end()
                        .child(Button::new("team-desc-cancel").label(t!("common.cancel")).ghost().small().on_click(
                            cx.listener(|this, _, _, cx| {
                                this.team_card = Some(false);
                                cx.notify();
                            }),
                        ))
                        .child(
                            Button::new("team-desc-save")
                                .label(t!("common.save"))
                                .primary()
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| this.save_team_description(cx))),
                        ),
                )
                .into_any_element()
        } else {
            h_flex()
                .items_start()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .when(team.description.is_empty(), |d| d.italic().text_color(p.muted()))
                        .child(if team.description.is_empty() {
                            t!("common.no_description").to_string()
                        } else {
                            team.description.clone()
                        }),
                )
                .when(admin, |el| {
                    el.child(
                        Button::new("team-desc-edit")
                            .icon(Icon::new(IconName::PencilLine))
                            .ghost()
                            .xsmall()
                            .accessibility_label(t!("team.edit_description"))
                            .tooltip(t!("team.edit_description"))
                            .on_click(cx.listener(|this, _, window, cx| this.edit_team_description(window, cx))),
                    )
                })
                .into_any_element()
        };
        let left = if self.teams.len() > 1 { TEAM_BAR_WIDTH + 10. } else { 10. };
        Some(
            div()
                .id("team-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close_team_card(cx)))
                .child(
                    v_flex()
                        .id("team-card")
                        .occlude()
                        .absolute()
                        .left(px(left))
                        .top(px(56.))
                        .w(px(340.))
                        .rounded(px(14.))
                        .overflow_hidden()
                        .border_1()
                        .border_color(p.border())
                        .bg(p.center_bg)
                        .text_color(p.center_color)
                        .shadow_xl()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(div().h(px(64.)).bg(linear_gradient(
                            120.,
                            linear_color_stop(p.sidebar_bg, 0.),
                            linear_color_stop(p.button_bg, 1.),
                        )))
                        .child(
                            v_flex()
                                .px_5()
                                .pb_5()
                                .gap_3()
                                .child(
                                    div()
                                        .mt(-px(36.))
                                        .p(px(4.))
                                        .rounded(px(19.))
                                        .bg(p.center_bg)
                                        .w(px(72.))
                                        .child(logo),
                                )
                                .child(
                                    v_flex()
                                        .gap_0p5()
                                        .child(div().text_size(px(19.)).font_bold().child(team.display_name.clone()))
                                        .child(div().text_sm().text_color(p.muted()).child(kind)),
                                )
                                .child(
                                    v_flex()
                                        .gap_2()
                                        .pt_3()
                                        .child(
                                            h_flex()
                                                .gap_2()
                                                .text_sm()
                                                .child(Icon::new(IconName::Users).size(px(15.)).text_color(p.muted()))
                                                .child(members),
                                        )
                                        .when_some(since, |el, since| {
                                            el.child(
                                                h_flex()
                                                    .gap_2()
                                                    .text_sm()
                                                    .child(
                                                        Icon::new(IconName::Clock).size(px(15.)).text_color(p.muted()),
                                                    )
                                                    .child(since),
                                            )
                                        }),
                                )
                                .child(
                                    v_flex()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_semibold()
                                                .text_color(p.muted())
                                                .child(t!("common.description")),
                                        )
                                        .child(description),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    /// Panneau du canal : pastille, nom, type, membres, création, description
    /// et en-tête (modifiables), actions rapides.
    fn render_channel_info(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editing = self.channel_editing;
        let id = self.channel_id.clone()?;
        let state = self.channels.get(&id)?;
        let channel = &state.channel;
        let p = self.palette;
        let kind = if channel.kind == "P" { t!("channel_card.private") } else { t!("channel_card.public") };
        let members = match self.members.get(&id) {
            Some((_, 1)) => t!("channel_card.members.one").to_string(),
            Some((_, total)) => t!("channel_card.members.other", count = total).to_string(),
            None => t!("channel_card.members_loading").to_string(),
        };
        let since = (channel.create_at > 0)
            .then(|| t!("channel_card.created", day = day_label_inline(channel.create_at)).to_string());
        let favorite = self.is_favorite(&id);
        let section = |title: SharedString| div().text_xs().font_semibold().text_color(p.muted()).child(title);
        // Markdown rendu (liens cliquables), ou une invite si vide.
        let text_or = |key: &str, text: &str, empty: SharedString| {
            let id = SharedString::from(format!("ch-info-{key}"));
            div().min_w_0().text_sm().when(text.is_empty(), |d| d.italic().text_color(p.muted())).child(
                if text.is_empty() {
                    empty.into_any_element()
                } else {
                    TextView::markdown(id, markdown::prepare(text))
                        .on_link_click(|url, _, _, cx| open_link(url, cx))
                        .into_any_element()
                },
            )
        };
        let details: AnyElement = if editing {
            v_flex()
                .gap_2()
                .child(section(t!("common.description").into()))
                .child(Input::new(&self.channel_purpose_input))
                .child(section(t!("channel_card.header").into()))
                .child(Input::new(&self.channel_header_input))
                .child(
                    h_flex()
                        .gap_2()
                        .justify_end()
                        .child(Button::new("ch-card-cancel").label(t!("common.cancel")).ghost().small().on_click(
                            cx.listener(|this, _, _, cx| {
                                this.channel_editing = false;
                                cx.notify();
                            }),
                        ))
                        .child(
                            Button::new("ch-card-save")
                                .label(t!("common.save"))
                                .primary()
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| this.save_channel_card(cx))),
                        ),
                )
                .into_any_element()
        } else {
            v_flex()
                .gap_3()
                .child(
                    v_flex()
                        .gap_1()
                        .child(
                            h_flex().child(div().flex_1().child(section(t!("common.description").into()))).child(
                                Button::new("ch-card-edit")
                                    .icon(Icon::new(IconName::PencilLine))
                                    .ghost()
                                    .xsmall()
                                    .accessibility_label(t!("channel_card.edit"))
                                    .tooltip(t!("channel_card.edit"))
                                    .on_click(cx.listener(|this, _, window, cx| this.edit_channel_card(window, cx))),
                            ),
                        )
                        .child(text_or("purpose", &channel.purpose, t!("common.no_description").into())),
                )
                .child(v_flex().gap_1().child(section(t!("channel_card.header").into())).child(text_or(
                    "header",
                    &channel.header,
                    t!("channel_card.no_header").into(),
                )))
                .into_any_element()
        };
        let (fav_id, mute_id, link_id, add_id) = (id.clone(), id.clone(), id.clone(), id.clone());
        let action = |key: &'static str, icon: IconName, label: SharedString| {
            v_flex()
                .id(key)
                .flex_1()
                .items_center()
                .gap_1()
                .py_2()
                .rounded(px(8.))
                .cursor_pointer()
                .text_xs()
                .text_color(p.center_color)
                .hover(|s| s.bg(p.hover()))
                .child(Icon::new(icon).size(px(17.)).text_color(p.button_bg))
                .child(label)
        };
        Some(
            v_flex()
                .id("channel-info")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(div().h(px(56.)).bg(linear_gradient(
                    120.,
                    linear_color_stop(badge_color(&channel.name), 0.),
                    linear_color_stop(p.sidebar_bg, 1.),
                )))
                .child(
                    v_flex()
                        .px_5()
                        .pb_4()
                        .gap_3()
                        .child(
                            div()
                                .mt(-px(30.))
                                .p(px(4.))
                                .rounded(px(14.))
                                .bg(p.center_bg)
                                .w(px(60.))
                                .child(self.channel_badge(channel, 52., p.center_bg)),
                        )
                        .child(
                            v_flex()
                                .gap_0p5()
                                .child(div().text_size(px(19.)).font_bold().child(self.channel_title(channel)))
                                .child(
                                    div().text_sm().text_color(p.muted()).child(format!("{kind} · ~{}", channel.name)),
                                ),
                        )
                        .child(
                            h_flex()
                                .gap_4()
                                .text_sm()
                                .child(
                                    h_flex()
                                        .gap_1p5()
                                        .child(Icon::new(IconName::Users).size(px(15.)).text_color(p.muted()))
                                        .child(members),
                                )
                                .when_some(since, |el, since| {
                                    el.child(
                                        h_flex()
                                            .gap_1p5()
                                            .child(Icon::new(IconName::Clock).size(px(15.)).text_color(p.muted()))
                                            .child(since),
                                    )
                                }),
                        )
                        .child(div().pt_3().child(details))
                        .child(
                            h_flex()
                                .gap_1()
                                .pt_2()
                                .child(
                                    action(
                                        "ch-card-fav",
                                        IconName::Star,
                                        if favorite {
                                            t!("channel_menu.unfavorite")
                                        } else {
                                            t!("channel_card.favorite")
                                        }
                                        .into(),
                                    )
                                    .on_click(
                                        cx.listener(move |this, _, _, cx| this.toggle_favorite(fav_id.clone(), cx)),
                                    ),
                                )
                                .child(
                                    action(
                                        "ch-card-mute",
                                        if state.muted { IconName::Bell } else { IconName::BellOff },
                                        if state.muted { t!("channel_card.unmute") } else { t!("channel_card.mute") }
                                            .into(),
                                    )
                                    .on_click(
                                        cx.listener(move |this, _, _, cx| this.toggle_muted(mute_id.clone(), cx)),
                                    ),
                                )
                                .child(
                                    action("ch-card-link", IconName::Link, t!("channel_menu.copy_link").into())
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| this.copy_channel_link(&link_id, cx)),
                                        ),
                                )
                                .child(action("ch-card-add", IconName::UserPlus, t!("common.add").into()).on_click(
                                    cx.listener(move |this, _, window, cx| {
                                        this.close_channel_card(cx);
                                        this.open_add_members(add_id.clone(), window, cx)
                                    }),
                                )),
                        ),
                )
                .into_any_element(),
        )
    }

    /// Fenêtre du bouton « + » : canal (nom, description, public/privé),
    /// catégorie (nom) ou invitation (adresses).
    fn render_create(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let kind = self.create?;
        let p = self.palette;
        let (title, icon, action) = match kind {
            super::CreateKind::Channel => (t!("create.channel_title"), IconName::Hash, t!("create.channel_submit")),
            super::CreateKind::Category => {
                (t!("create.category_title"), IconName::FolderPlus, t!("create.category_submit"))
            }
            super::CreateKind::Invite => (t!("create.invite_title"), IconName::UserPlus, t!("create.invite_submit")),
        };
        let label = |t: SharedString| div().text_xs().font_semibold().text_color(p.muted()).child(t);
        let choice = |id: &'static str, on: bool, icon: IconName, t: SharedString, hint: SharedString| {
            h_flex()
                .id(id)
                .flex_1()
                .gap_2()
                .p_2()
                .rounded(px(8.))
                .border_1()
                .border_color(if on { p.button_bg } else { p.border() })
                .when(on, |el| el.bg(p.button_bg.opacity(0.08)))
                .cursor_pointer()
                .child(Icon::new(icon).size(px(16.)).text_color(if on { p.button_bg } else { p.muted() }))
                .child(
                    v_flex()
                        .child(div().text_sm().font_semibold().child(t))
                        .child(div().text_xs().text_color(p.muted()).child(hint)),
                )
        };
        let name = self.channel_slug_preview(cx);
        Some(
            div()
                .id("create-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(black().opacity(0.35))
                .flex()
                .justify_center()
                .items_start()
                .pt(px(110.))
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close_create(cx)))
                .child(
                    v_flex()
                        .id("create")
                        .occlude()
                        .w(px(460.))
                        .gap_3()
                        .p_5()
                        .rounded(px(12.))
                        .border_1()
                        .border_color(p.border())
                        .bg(p.center_bg)
                        .text_color(p.center_color)
                        .shadow_xl()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            h_flex()
                                .gap_2()
                                .child(Icon::new(icon).size(px(18.)).text_color(p.button_bg))
                                .child(div().flex_1().text_size(px(16.)).font_bold().child(title))
                                .child(
                                    Button::new("create-close")
                                        .icon(Icon::new(IconName::X))
                                        .ghost()
                                        .small()
                                        .on_click(cx.listener(|this, _, _, cx| this.close_create(cx))),
                                ),
                        )
                        .child(
                            v_flex()
                                .gap_1()
                                .child(label(
                                    match kind {
                                        super::CreateKind::Invite => t!("create.emails_label"),
                                        _ => t!("create.name_label"),
                                    }
                                    .into(),
                                ))
                                .child(Input::new(&self.create_name_input))
                                .when(kind == super::CreateKind::Channel && !name.is_empty(), |el| {
                                    el.child(
                                        div()
                                            .text_xs()
                                            .text_color(p.muted())
                                            .child(t!("create.link_preview", name = name)),
                                    )
                                }),
                        )
                        .when(kind == super::CreateKind::Channel, |el| {
                            el.child(
                                v_flex()
                                    .gap_1()
                                    .child(label(t!("common.description").into()))
                                    .child(Input::new(&self.create_purpose_input)),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        choice(
                                            "create-public",
                                            !self.create_private,
                                            IconName::Hash,
                                            t!("create.public").into(),
                                            t!("create.public_hint").into(),
                                        )
                                        .on_click(cx.listener(
                                            |this, _, _, cx| {
                                                this.create_private = false;
                                                cx.notify();
                                            },
                                        )),
                                    )
                                    .child(
                                        choice(
                                            "create-private",
                                            self.create_private,
                                            IconName::Lock,
                                            t!("create.private").into(),
                                            t!("create.private_hint").into(),
                                        )
                                        .on_click(cx.listener(
                                            |this, _, _, cx| {
                                                this.create_private = true;
                                                cx.notify();
                                            },
                                        )),
                                    ),
                            )
                        })
                        .when(kind == super::CreateKind::Invite, |el| {
                            el.child(div().text_xs().text_color(p.muted()).child(t!("create.invite_hint")))
                        })
                        .child(
                            h_flex()
                                .gap_2()
                                .justify_end()
                                .child(
                                    Button::new("create-cancel")
                                        .label(t!("common.cancel"))
                                        .ghost()
                                        .on_click(cx.listener(|this, _, _, cx| this.close_create(cx))),
                                )
                                .child(
                                    Button::new("create-submit")
                                        .label(action)
                                        .primary()
                                        .on_click(cx.listener(|this, _, _, cx| this.submit_create(cx))),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    fn channel_slug_preview(&self, cx: &App) -> String {
        let name = self.create_name_input.read(cx).value().trim().to_string();
        if name.is_empty() { String::new() } else { super::channel_slug(&name) }
    }

    /// Fenêtre « Ajouter des membres » : recherche dans l'équipe, ajout un par un.
    fn render_add_members(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let add = self.add_members.as_ref()?;
        let p = self.palette;
        let title = self.channels.get(&add.channel_id).map(|c| self.channel_title(&c.channel)).unwrap_or_default();
        let body: AnyElement = match &add.results {
            None => div().p_4().text_color(p.muted()).child(t!("members.searching")).into_any_element(),
            Some(users) if users.is_empty() => {
                div().p_4().text_color(p.muted()).child(t!("members.nobody_to_add")).into_any_element()
            }
            Some(users) => v_flex()
                .id("add-members-list")
                .max_h(px(360.))
                .overflow_y_scroll()
                .py_1()
                .children(users.iter().map(|u| {
                    let added = add.added.contains(&u.id);
                    let uid = u.id.clone();
                    h_flex()
                        .gap_3()
                        .px_4()
                        .py_2()
                        .hover(|s| s.bg(p.hover()))
                        .child(self.avatar(&u.id, 32., p.center_bg))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(div().truncate().text_sm().font_semibold().child(self.user_name(&u.id)))
                                .child(
                                    div().truncate().text_xs().text_color(p.muted()).child(format!("@{}", u.username)),
                                ),
                        )
                        .child(if added {
                            h_flex()
                                .gap_1()
                                .text_sm()
                                .text_color(p.online)
                                .child(Icon::new(IconName::CircleCheck).size(px(15.)))
                                .child(t!("members.added"))
                                .into_any_element()
                        } else {
                            Button::new(SharedString::from(format!("add-{}", u.id)))
                                .icon(Icon::new(IconName::UserPlus))
                                .label(t!("common.add"))
                                .small()
                                .primary()
                                .on_click(cx.listener(move |this, _, _, cx| this.add_member(uid.clone(), cx)))
                                .into_any_element()
                        })
                }))
                .into_any_element(),
        };
        Some(
            div()
                .id("add-members-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(black().opacity(0.35))
                .flex()
                .justify_center()
                .items_start()
                .pt(px(110.))
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.close_add_members(cx)))
                .child(
                    v_flex()
                        .id("add-members")
                        .occlude()
                        .w(px(460.))
                        .rounded(px(12.))
                        .overflow_hidden()
                        .border_1()
                        .border_color(p.border())
                        .bg(p.center_bg)
                        .text_color(p.center_color)
                        .shadow_xl()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            h_flex()
                                .px_4()
                                .pt_4()
                                .pb_2()
                                .gap_2()
                                .child(Icon::new(IconName::UserPlus).size(px(18.)).text_color(p.button_bg))
                                .child(
                                    div()
                                        .flex_1()
                                        .text_size(px(16.))
                                        .font_bold()
                                        .child(t!("members.add_to", channel = title)),
                                )
                                .child(
                                    Button::new("add-members-close")
                                        .icon(Icon::new(IconName::X))
                                        .ghost()
                                        .small()
                                        .on_click(cx.listener(|this, _, _, cx| this.close_add_members(cx))),
                                ),
                        )
                        .child(
                            div().px_4().pb_3().border_b_1().border_color(p.border()).child(
                                Input::new(&self.member_search_input)
                                    .prefix(Icon::new(IconName::Search).size(px(14.)).text_color(p.muted()))
                                    .cleanable(true),
                            ),
                        )
                        .child(body),
                )
                .into_any_element(),
        )
    }

    /// Sélecteur de canaux (Ctrl+K).
    fn render_switcher(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let selected = self.switcher?;
        let p = self.palette;
        let matches = self.switcher_matches(cx);
        let rows = matches.iter().enumerate().filter_map(|(i, id)| {
            let state = self.channels.get(id)?;
            let channel = &state.channel;
            let icon: AnyElement = match channel.direct_partner(self.me_id()) {
                Some(partner) => self.avatar(&partner, 22., p.center_bg).into_any_element(),
                None => self.channel_badge(channel, 22., p.center_bg),
            };
            let id = id.clone();
            Some(
                h_flex()
                    .id(SharedString::from(format!("sw-{i}")))
                    .gap_3()
                    .px_4()
                    .py_2()
                    .cursor_pointer()
                    .when(i == selected, |el| el.bg(p.button_bg.opacity(0.16)))
                    .hover(|s| s.bg(p.button_bg.opacity(0.08)))
                    .child(div().w(px(22.)).flex().justify_center().child(icon))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .when(state.shows_unread(), |el| el.font_semibold())
                            .child(self.channel_title(channel)),
                    )
                    .when(state.mentions > 0, |el| {
                        el.child(
                            div()
                                .px_1p5()
                                .rounded_full()
                                .text_xs()
                                .font_bold()
                                .bg(p.mention_bg)
                                .text_color(p.mention_color)
                                .child(state.mentions.to_string()),
                        )
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.switcher = None;
                        this.select_channel(id.clone(), cx);
                        this.focus_composer(window, cx);
                    })),
            )
        });
        let empty = matches.is_empty();
        Some(
            div()
                .id("switcher-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .bg(black().opacity(0.4))
                .flex()
                .justify_center()
                // Sans cela, la fenêtre s'étirerait jusqu'à sa hauteur maximale.
                .items_start()
                .pt(px(110.))
                .on_click(cx.listener(|this, _, window, cx| this.close_switcher(window, cx)))
                .child(
                    v_flex()
                        .id("switcher")
                        .w(px(580.))
                        .max_h(px(520.))
                        .rounded(px(10.))
                        .bg(p.center_bg)
                        .text_color(p.center_color)
                        .shadow_lg()
                        .border_1()
                        .border_color(p.border())
                        // Un clic dans la fenêtre ne la ferme pas.
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .capture_action(cx.listener(|this, _: &MoveUp, window, cx| {
                            this.switcher_key("up", window, cx);
                            cx.stop_propagation();
                        }))
                        .capture_action(cx.listener(|this, _: &MoveDown, window, cx| {
                            this.switcher_key("down", window, cx);
                            cx.stop_propagation();
                        }))
                        .capture_action(cx.listener(|this, _: &Enter, window, cx| {
                            this.switcher_key("enter", window, cx);
                            cx.stop_propagation();
                        }))
                        .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                            this.switcher_key("escape", window, cx);
                            cx.stop_propagation();
                        }))
                        .child(
                            div().p_3().child(
                                Input::new(&self.switcher_input)
                                    .large()
                                    .prefix(Icon::new(IconName::Search).text_color(p.muted())),
                            ),
                        )
                        .child(div().h(px(1.)).bg(p.border()))
                        .child(v_flex().py_1().children(rows).when(empty, |el| {
                            el.child(div().px_4().py_3().text_color(p.muted()).child(t!("switcher.no_match")))
                        })),
                )
                .into_any_element(),
        )
    }

    /// Sélecteur d'emojis : recherche, catégories, emojis du serveur.
    fn render_emoji_picker(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.emoji_picker.as_ref()?;
        let p = self.palette;
        const PER_ROW: usize = 9;

        // Lignes : titres de catégorie et rangées d'emojis (nom, caractère ou
        // vide pour un emoji personnalisé).
        enum Row {
            Title(String),
            Emojis(Vec<(String, String)>),
        }
        let query = crate::fuzzy::fold(self.emoji_search.read(cx).value().trim());
        let mut rows: Vec<Row> = Vec::new();
        let push_group = |rows: &mut Vec<Row>, title: &str, list: Vec<(String, String)>| {
            if list.is_empty() {
                return;
            }
            if query.is_empty() {
                rows.push(Row::Title(title.to_string()));
            }
            for chunk in list.chunks(PER_ROW) {
                rows.push(Row::Emojis(chunk.to_vec()));
            }
        };
        let custom: Vec<(String, String)> = {
            let mut names: Vec<&String> = self.custom_emojis.keys().collect();
            names.sort();
            names.into_iter().filter(|n| n.contains(&query)).map(|n| (n.clone(), String::new())).collect()
        };
        if query.is_empty() {
            let recent: Vec<(String, String)> = self
                .recent_emojis
                .iter()
                .map(|n| (n.clone(), if self.custom_emojis.contains_key(n) { String::new() } else { emoji::lookup(n) }))
                .collect();
            push_group(&mut rows, &t!("emoji.recent"), recent);
            push_group(&mut rows, &t!("emoji.custom"), custom);
            for (title, list) in crate::emoji::catalog() {
                push_group(&mut rows, &t!(*title), list.clone());
            }
        } else {
            let mut found = custom;
            for (_, list) in crate::emoji::catalog() {
                found.extend(list.iter().filter(|(name, _)| name.contains(&query)).cloned());
            }
            push_group(&mut rows, "", found);
        }
        let empty = rows.is_empty();
        let rows = Arc::new(rows);
        let entity = cx.entity();
        let customs = self.custom_emojis.clone();
        let list = uniform_list("emoji-rows", rows.len(), move |range, _, _| {
            range
                .map(|ix| match &rows[ix] {
                    Row::Title(t) => div()
                        .h(px(36.))
                        .px_2()
                        .flex()
                        .items_end()
                        .pb_1()
                        .text_xs()
                        .font_semibold()
                        .text_color(p.muted())
                        .child(t.clone())
                        .into_any_element(),
                    Row::Emojis(list) => h_flex()
                        .h(px(36.))
                        .children(list.iter().map(|(name, glyph)| {
                            let (entity, n, g) = (entity.clone(), name.clone(), glyph.clone());
                            let face: AnyElement = if glyph.is_empty() {
                                match customs.get(name) {
                                    Some(image) => img(image.clone()).size(px(24.)).into_any_element(),
                                    None => div().child(format!(":{name}:")).into_any_element(),
                                }
                            } else {
                                emoji_face(glyph, 24.)
                            };
                            div()
                                .id(SharedString::from(format!("em-{name}")))
                                .size(px(36.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(6.))
                                .cursor_pointer()
                                .hover(|s| s.bg(p.button_bg.opacity(0.14)))
                                .child(face)
                                .tooltip({
                                    let tip: SharedString = format!(":{name}:").into();
                                    move |window, cx| {
                                        gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                                    }
                                })
                                .on_click(move |_, window, cx| {
                                    entity.update(cx, |this, cx| this.pick_emoji(n.clone(), g.clone(), window, cx))
                                })
                        }))
                        .into_any_element(),
                })
                .collect::<Vec<_>>()
        })
        .flex_1();

        Some(
            div()
                .id("emoji-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .on_click(cx.listener(|this, _, window, cx| this.close_emoji_picker(window, cx)))
                .child(
                    v_flex()
                        .id("emoji-picker")
                        .absolute()
                        .right(px(28.))
                        .bottom(px(120.))
                        .w(px(36. * PER_ROW as f32 + 24.))
                        .h(px(400.))
                        .p_3()
                        .gap_2()
                        .rounded(px(10.))
                        .bg(p.center_bg)
                        .text_color(p.center_color)
                        .shadow_lg()
                        .border_1()
                        .border_color(p.border())
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                            this.close_emoji_picker(window, cx);
                            cx.stop_propagation();
                        }))
                        .child(
                            Input::new(&self.emoji_search)
                                .prefix(Icon::new(IconName::Search).size(px(14.)).text_color(p.muted())),
                        )
                        .when(empty, |el| el.child(div().p_2().text_color(p.muted()).child(t!("emoji.no_match"))))
                        .child(list),
                )
                .into_any_element(),
        )
    }

    /// Visionneuse d'images plein écran (Échap ou clic à côté pour fermer).
    fn render_viewer(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let viewer = self.viewer.as_ref()?;
        let file = viewer.file.clone();
        let light = white();
        let action = |id: &'static str, icon: IconName, tip: SharedString| {
            div()
                .id(id)
                .p_2()
                .rounded(px(6.))
                .cursor_pointer()
                .text_color(light.opacity(0.85))
                .hover(|s| s.bg(light.opacity(0.12)).text_color(light))
                .child(Icon::new(icon).size(px(20.)))
                .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx))
        };

        let content: AnyElement = match (&viewer.image, &viewer.error) {
            (_, None) if viewer.pdf.as_ref().is_some_and(|(pages, _)| !pages.is_empty()) => {
                let (pages, total) = viewer.pdf.as_ref().expect("pages");
                let more = (*total as usize).saturating_sub(pages.len());
                v_flex()
                    .id("pdf-pages")
                    .size_full()
                    .overflow_y_scroll()
                    .items_center()
                    .gap_4()
                    .pb_6()
                    .children(pages.iter().enumerate().map(|(i, page)| {
                        // Format A4 supposé ; « contain » garde les proportions des autres formats.
                        div()
                            .id(SharedString::from(format!("pdf-page-{i}")))
                            .flex_none()
                            .w(px(820.))
                            .h(px(1160.))
                            .max_w_full()
                            .bg(white())
                            .shadow_lg()
                            .child(img(page.clone()).size_full().object_fit(ObjectFit::Contain))
                    }))
                    .when(more > 0, |el| {
                        el.child(
                            div().text_sm().text_color(light.opacity(0.7)).child(t!("viewer.pdf_more", count = more)),
                        )
                    })
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .into_any_element()
            }
            (_, Some(err)) => div().text_color(light.opacity(0.8)).child(err.clone()).into_any_element(),
            (Some(image), None) => div()
                .id("viewer-image")
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child({
                    // Pixels de l'image ramenés en pixels d'écran : jamais plus grande
                    // que nature ; réduite seulement si elle dépasse la fenêtre.
                    let (w, h) = (viewer.file.width as f32, viewer.file.height as f32);
                    let picture = img(image.clone()).object_fit(ObjectFit::Contain).max_w_full().max_h_full();
                    if w > 0. && h > 0. {
                        picture.w(px(w / self.scale)).h(px(h / self.scale))
                    } else {
                        picture.size_full()
                    }
                })
                // Un clic sur l'image ne ferme pas la visionneuse.
                .on_click(|_, _, cx| cx.stop_propagation())
                .into_any_element(),
            (None, None) => div().text_color(light.opacity(0.8)).child(t!("common.loading")).into_any_element(),
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
                                .child(
                                    action("viewer-download", IconName::Download, t!("common.download").into())
                                        .on_click(cx.listener({
                                            let file = file.clone();
                                            move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.download(file.clone(), false, cx);
                                            }
                                        })),
                                )
                                .child(action("viewer-close", IconName::X, t!("viewer.close").into()).on_click(
                                    cx.listener(|this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.close_viewer(cx);
                                    }),
                                )),
                        )
                        .child(div().flex_1().min_h_0().p_6().flex().items_center().justify_center().child(content)),
                ),
        )
    }
}

impl Render for ChatApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.scale = window.scale_factor();
        self.viewport_w = window.viewport_size().width / px(1.);
        self.viewport_h = window.viewport_size().height / px(1.);
        // Fenêtre fermée : son champ n'est plus affiché, le focus revient à
        // l'application (sinon les raccourcis ne répondent plus).
        let orphan = self.dialog.is_none()
            && (self.dialog_input.read(cx).focus_handle(cx).is_focused(window)
                || self.dialog_comment.read(cx).focus_handle(cx).is_focused(window));
        if self.screen == Screen::Chat && (window.focused(cx).is_none() || orphan) {
            window.focus(&self.root_focus, cx);
        }
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

/// « 1 mention », « 3 mentions » (lecteur d'écran).
fn a11y_mentions(count: i64) -> String {
    if count == 1 { t!("a11y.mentions.one").to_string() } else { t!("a11y.mentions.other", count = count).to_string() }
}

fn local(ms: i64) -> Option<chrono::DateTime<chrono::Local>> {
    use chrono::TimeZone;
    chrono::Local.timestamp_millis_opt(ms).single()
}

fn day_of(ms: i64) -> Option<chrono::NaiveDate> {
    local(ms).map(|t| t.date_naive())
}

/// « 14:32 ».
fn format_hour(ms: i64) -> String {
    local(ms).map(|t| t.format(settings::time_format()).to_string()).unwrap_or_default()
}

/// Heure dans l'en-tête d'un message (la date est donnée par le séparateur).
fn format_time(ms: i64) -> String {
    format_hour(ms)
}

/// « Aujourd'hui », « Hier », « jeudi 19 novembre » ou « 3 mars 2025 ».
fn day_label(ms: i64) -> String {
    day_label_with(ms, false)
}

/// Comme `day_label`, au fil d'une phrase (« hier », « today »).
fn day_label_inline(ms: i64) -> String {
    day_label_with(ms, true)
}

fn day_label_with(ms: i64, inline: bool) -> String {
    use chrono::Datelike;
    let Some(t) = local(ms) else { return String::new() };
    let today = chrono::Local::now().date_naive();
    let date = t.date_naive();
    let month = t!(MONTHS[date.month0() as usize]);
    if date == today {
        if inline { t!("dates.today_inline") } else { t!("dates.today") }.into()
    } else if Some(date) == today.pred_opt() {
        if inline { t!("dates.yesterday_inline") } else { t!("dates.yesterday") }.into()
    } else if date.year() == today.year() {
        let weekday = t!(WEEKDAYS[date.weekday().num_days_from_monday() as usize]);
        t!("dates.this_year", weekday = weekday, day = date.day(), month = month).into()
    } else {
        t!("dates.other_year", day = date.day(), month = month, year = date.year()).into()
    }
}

/// Clés des jours de la semaine (lundi d'abord) et des mois.
const WEEKDAYS: [&str; 7] = [
    "dates.weekdays.monday",
    "dates.weekdays.tuesday",
    "dates.weekdays.wednesday",
    "dates.weekdays.thursday",
    "dates.weekdays.friday",
    "dates.weekdays.saturday",
    "dates.weekdays.sunday",
];
const MONTHS: [&str; 12] = [
    "dates.months.january",
    "dates.months.february",
    "dates.months.march",
    "dates.months.april",
    "dates.months.may",
    "dates.months.june",
    "dates.months.july",
    "dates.months.august",
    "dates.months.september",
    "dates.months.october",
    "dates.months.november",
    "dates.months.december",
];

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
        .child(div().text_size(px(12.)).font_semibold().text_color(p.new_separator).child(t!("messages.new_messages")))
        .child(div().flex_1().h(px(1.)).bg(p.new_separator))
}

/// Résout les images `mm-emoji:nom` du Markdown vers les emojis personnalisés
/// chargés ; les autres adresses gardent le chargement normal.
fn emoji_resolver(
    emojis: Arc<HashMap<String, Arc<Image>>>,
) -> impl Fn(&SharedUri) -> ImageSource + Send + Sync + 'static {
    move |uri: &SharedUri| {
        if let Some(hex) = uri.strip_prefix(emoji::TWEMOJI_SCHEME) {
            return ImageSource::Image(twemoji_image(&emoji::from_hex(hex)).unwrap_or_else(transparent_pixel));
        }
        match uri.strip_prefix(markdown::CUSTOM_EMOJI_SCHEME) {
            Some(name) => match emojis.get(name) {
                Some(image) => ImageSource::Image(image.clone()),
                // Pas encore chargé : image transparente, remplacée au prochain rendu.
                None => ImageSource::Image(transparent_pixel()),
            },
            None => ImageSource::Resource(Resource::Uri(uri.clone())),
        }
    }
}

/// Image Twemoji d'un emoji (SVG intégré à l'application), gardée en mémoire.
fn twemoji_image(glyph: &str) -> Option<Arc<Image>> {
    use std::sync::Mutex;
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Image>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(image) = cache.lock().ok()?.get(glyph) {
        return Some(image.clone());
    }
    let svg = emoji::twemoji_svg(glyph)?;
    let image = Arc::new(Image::from_bytes(ImageFormat::Svg, svg.as_bytes().to_vec()));
    cache.lock().ok()?.insert(glyph.to_string(), image.clone());
    Some(image)
}

/// Emoji dessiné en Twemoji, sinon avec la police du système.
fn emoji_face(glyph: &str, size: f32) -> AnyElement {
    match twemoji_image(glyph) {
        Some(image) => img(image).size(px(size)).flex_none().into_any_element(),
        None => div().text_size(px(size * 0.9)).child(glyph.to_string()).into_any_element(),
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

/// Heure dans le fuseau `tz` (« 14:32 ») et écart avec l'heure locale.
fn local_time(tz: &str) -> Option<(String, String)> {
    use chrono::Offset as _;
    let tz: chrono_tz::Tz = tz.parse().ok()?;
    let there = chrono::Utc::now().with_timezone(&tz);
    let theirs = there.offset().fix().local_minus_utc();
    let mine = chrono::Local::now().offset().local_minus_utc();
    Some((there.format(settings::time_format()).to_string(), offset_label((theirs - mine) / 60)))
}

fn offset_label(minutes: i32) -> String {
    if minutes == 0 {
        return t!("time.same_as_you").into();
    }
    let (h, m) = (minutes.abs() / 60, minutes.abs() % 60);
    let span = if m == 0 { t!("time.hours", h = h) } else { t!("time.hours_minutes", h = h, m = format!("{m:02}")) };
    if minutes > 0 { t!("time.ahead", span = span) } else { t!("time.behind", span = span) }.into()
}

#[cfg(test)]
mod profile_tests {
    use super::offset_label;

    #[test]
    fn decalage_horaire_lisible() {
        assert_eq!(offset_label(0), t!("time.same_as_you"));
        assert_eq!(offset_label(-360), t!("time.behind", span = t!("time.hours", h = 6)));
        assert_eq!(offset_label(330), t!("time.ahead", span = t!("time.hours_minutes", h = 5, m = "30")));
        // Avec le catalogue français, le texte d'origine.
        let fr = |span| t!("time.ahead", locale = "fr", span = span);
        assert_eq!(fr(t!("time.hours_minutes", locale = "fr", h = 5, m = "30")), "5 h 30 d'avance");
    }
}

/// Action d'une proposition de recherche.
enum SearchPick {
    /// Insère un filtre dans le champ (la saisie continue).
    Insert(String),
    /// Lance la recherche telle quelle.
    Run(String),
}

/// Lien trouvé dans un message du canal.
struct ChannelLink {
    title: Option<String>,
    url: String,
    user_id: String,
    at: i64,
}

/// Adresses web d'un texte (ponctuation finale retirée).
fn extract_urls(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for start in ["https://", "http://"] {
        let mut rest = text;
        while let Some(i) = rest.find(start) {
            let tail = &rest[i..];
            let end = tail
                .find(|c: char| c.is_whitespace() || matches!(c, ')' | '>' | '"' | '\'' | ']' | '<'))
                .unwrap_or(tail.len());
            let url = tail[..end].trim_end_matches(['.', ',', ';', ':', '!', '?']);
            if url.len() > start.len() && !out.iter().any(|u| u == url) {
                out.push(url.to_string());
            }
            rest = &tail[end.max(1)..];
        }
    }
    out
}

/// Mention ou étiquette reconnue dans un message.
struct TagData {
    hashtag: bool,
    value: String,
}

/// Affiche `@nom` et `#étiquette` en pastilles cliquables : carte de profil
/// pour une personne, recherche pour une étiquette. Vos propres mentions (et
/// @here, @channel, @all) sont surlignées.
/// Mot-clé surligné (paramètres de notification).
struct KeywordData;

struct TagPlugin {
    me: String,
    /// Mots-clés surlignés (une mention `@clé` l'est aussi).
    keywords: Vec<String>,
    palette: Palette,
    app: WeakEntity<ChatApp>,
}

impl MarkdownPlugin for TagPlugin {
    fn name(&self) -> &str {
        "mm-tag"
    }

    fn parse(&self, node: &markdown_ast::Node, _cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let markdown_ast::Node::Link(link) = node else { return None };
        if let Some(hex) = link.url.strip_prefix(markdown::KEYWORD_SCHEME) {
            let text = markdown::keyword_text(hex)?;
            return Some(MarkdownNode::new("mm-tag", KeywordData).text(text));
        }
        let (hashtag, value) = match link.url.strip_prefix(markdown::MENTION_SCHEME) {
            Some(v) => (false, v),
            None => (true, link.url.strip_prefix(markdown::HASHTAG_SCHEME)?),
        };
        let text = format!("{}{value}", if hashtag { '#' } else { '@' });
        Some(MarkdownNode::new("mm-tag", TagData { hashtag, value: value.to_string() }).text(text))
    }

    fn render(&self, node: &MarkdownNode, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let p = self.palette;
        if node.data::<KeywordData>().is_some() {
            // Mot-clé surligné : simple fond, rien à cliquer.
            return div()
                .px(px(2.))
                .rounded(px(3.))
                .bg(p.mention_highlight_bg.opacity(0.6))
                .text_color(p.center_color)
                .child(node.as_text().to_string())
                .into_any_element();
        }
        let Some(tag) = node.data::<TagData>() else {
            return div().child(node.as_text().to_string()).into_any_element();
        };
        let mine = !tag.hashtag
            && (tag.value.eq_ignore_ascii_case(&self.me)
                || matches!(tag.value.as_str(), "here" | "channel" | "all")
                || self.keywords.iter().any(|k| k.strip_prefix('@') == Some(tag.value.to_lowercase().as_str())));
        let (bg, fg) = match (tag.hashtag, mine) {
            (true, _) => (transparent_black(), p.link),
            (false, true) => (p.mention_highlight_bg.opacity(0.6), p.center_color),
            (false, false) => (p.link.opacity(0.13), p.link),
        };
        let (app, value, hashtag) = (self.app.clone(), tag.value.clone(), tag.hashtag);
        let start = node.source_range().map(|r| r.start).unwrap_or_default();
        div()
            .id(SharedString::from(format!("tag-{start}-{value}")))
            .px(px(if tag.hashtag { 0. } else { 3. }))
            .rounded(px(4.))
            .bg(bg)
            .text_color(fg)
            .font_semibold()
            .cursor_pointer()
            .hover(|s| s.underline())
            .child(node.as_text().to_string())
            .on_click(move |ev, window, cx| {
                let (value, at) = (value.clone(), ev.position());
                let _ = app.update(cx, |this, cx| {
                    if hashtag {
                        this.search_for(format!("#{value}"), window, cx);
                    } else {
                        this.open_profile_by_name(value, at, window, cx);
                    }
                });
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod link_tests {
    use super::extract_urls;

    #[test]
    fn adresses_dans_le_texte() {
        let urls = extract_urls("Voir https://ex.org/a. et [doc](https://doc.ex.org/x?y=1), http://b.fr!");
        assert_eq!(urls, vec!["https://ex.org/a", "https://doc.ex.org/x?y=1", "http://b.fr"]);
    }
}

/// Largeur utile de la galerie des médias (panneau moins ses marges).
const MEDIA_ROW_WIDTH: f32 = THREAD_WIDTH - 2. * 12. - 2.;
/// Hauteur visée d'une ligne de la galerie.
const MEDIA_ROW_HEIGHT: f32 = 120.;

/// Proportions d'un média (largeur / hauteur), bornées pour les formats extrêmes.
fn media_ratio(f: &FileInfo) -> f32 {
    if f.width > 0 && f.height > 0 {
        (f.width as f32 / f.height as f32).clamp(0.5, 3.0)
    } else if f.is_video() {
        16. / 9.
    } else {
        1.
    }
}

/// Répartit les médias en lignes qui remplissent `width` en gardant les
/// proportions : chaque ligne est mise à l'échelle pour tomber juste. La
/// dernière ligne, incomplète, garde la hauteur visée.
fn justify_rows(files: &[FileInfo], width: f32, target: f32, gap: f32) -> Vec<Vec<(FileInfo, f32, f32)>> {
    let mut rows = Vec::new();
    let mut row: Vec<(FileInfo, f32)> = Vec::new();
    let mut sum = 0.;
    let height = |n: usize, sum: f32| (width - gap * (n - 1) as f32) / sum;
    for f in files {
        let r = media_ratio(f);
        row.push((f.clone(), r));
        sum += r;
        let h = height(row.len(), sum);
        if h <= target {
            // Coupe avant ou après cette image : la hauteur la plus proche de la visée.
            let before = (row.len() > 1).then(|| height(row.len() - 1, sum - r));
            match before {
                Some(hb) if (hb - target).abs() < (target - h) => {
                    let last = row.pop().expect("ligne non vide");
                    rows.push(row.drain(..).map(|(f, r)| (f, (r * hb).floor(), hb.floor())).collect());
                    sum = last.1;
                    row.push(last);
                }
                _ => {
                    rows.push(row.drain(..).map(|(f, r)| (f, (r * h).floor(), h.floor())).collect());
                    sum = 0.;
                }
            }
        }
    }
    if !row.is_empty() {
        rows.push(row.into_iter().map(|(f, r)| (f, (r * target).floor(), target)).collect());
    }
    rows
}

/// « Novembre 2026 ».
fn month_label(ms: i64) -> String {
    use chrono::Datelike as _;
    let Some(date) = chrono::DateTime::from_timestamp_millis(ms) else { return String::new() };
    let date = date.with_timezone(&chrono::Local);
    let month = t!(MONTHS[date.month0() as usize]);
    t!("dates.month_year", month = capitalize(&month), year = date.year()).into()
}

/// Première lettre en majuscule (« novembre » → « Novembre »).
fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// Titre affiché d'un groupe de la liste des membres.
fn member_group_title(group: &str) -> SharedString {
    match group {
        "admins" => t!("members.groups.admins"),
        "online" => t!("status.online"),
        "away" => t!("status.away"),
        "dnd" => t!("status.dnd"),
        _ => t!("status.offline"),
    }
    .into()
}

/// Couleur d'un groupe de la liste des membres.
fn group_color(group: &str, p: &Palette) -> Hsla {
    match group {
        "admins" => p.button_bg,
        "online" => p.online,
        "away" => p.away,
        "dnd" => p.dnd,
        _ => p.center_color.opacity(0.5),
    }
}

/// Même teinte, décalée (`dh`) et plus claire ou plus foncée (`dl`).
fn shade(c: Hsla, dh: f32, dl: f32) -> Hsla {
    hsla((c.h + dh).rem_euclid(1.), c.s, (c.l + dl).clamp(0., 1.), c.a)
}

/// Couleur stable d'un canal, tirée de son nom, parmi des teintes lisibles
/// avec du texte blanc.
fn badge_color(name: &str) -> Hsla {
    const HUES: [f32; 12] = [0.98, 0.04, 0.09, 0.30, 0.40, 0.48, 0.54, 0.60, 0.67, 0.74, 0.82, 0.90];
    let mut hash: u32 = 0x811c9dc5;
    for b in name.bytes() {
        hash = (hash ^ b as u32).wrapping_mul(0x01000193);
    }
    let hue = HUES[(hash % HUES.len() as u32) as usize];
    // Jaunes et verts plus sombres pour garder le texte blanc lisible.
    let l = if (0.08..0.45).contains(&hue) { 0.40 } else { 0.50 };
    hsla(hue, 0.58, l, 1.)
}

/// Deux lettres pour un nom de canal : initiales de deux mots, sinon les
/// deux premières lettres (« Dev backend » → « DB », « général » → « GÉ »).
fn abbreviation(name: &str) -> String {
    let words: Vec<&str> = name.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let letters: String = match words.as_slice() {
        [] => "#".into(),
        [one] => one.chars().take(2).collect(),
        [a, b, ..] => a.chars().take(1).chain(b.chars().take(1)).collect(),
    };
    letters.to_uppercase()
}

#[cfg(test)]
mod badge_tests {
    use super::{FileInfo, abbreviation, badge_color, justify_rows};

    #[test]
    fn abreviations() {
        assert_eq!(abbreviation("Dev backend"), "DB");
        assert_eq!(abbreviation("général"), "GÉ");
        assert_eq!(abbreviation("support-n2"), "SN");
        assert_eq!(abbreviation("🚀 lancement"), "LA");
        assert_eq!(abbreviation("—"), "#");
    }

    #[test]
    fn couleur_stable() {
        assert_eq!(badge_color("town-square"), badge_color("town-square"));
    }

    #[test]
    fn galerie_justifiee() {
        let f = |w, h| FileInfo { width: w, height: h, ..Default::default() };
        let files = vec![f(1600, 900), f(800, 800), f(900, 1600), f(1200, 800), f(1000, 1000)];
        let rows = justify_rows(&files, 400., 120., 6.);
        // Chaque ligne complète remplit la largeur (à l'arrondi près).
        for row in &rows[..rows.len() - 1] {
            let total: f32 = row.iter().map(|(_, w, _)| w).sum::<f32>() + 6. * (row.len() - 1) as f32;
            assert!((total - 400.).abs() <= row.len() as f32, "{total}");
            assert!(row.iter().all(|(_, _, h)| *h <= 180.));
        }
        assert_eq!(rows.iter().map(Vec::len).sum::<usize>(), 5);
    }
}

/// Hauteur d'un message de bot réduit.
const BOT_MAX_HEIGHT: f32 = 92.;

/// Estimation du nombre de lignes d'un message (texte et pièces jointes des
/// intégrations), pour savoir s'il vaut la peine de le réduire.
fn bot_post_lines(post: &Post) -> usize {
    let text_lines = |t: &str| t.lines().map(|l| 1 + l.chars().count() / 90).sum::<usize>();
    let mut lines = text_lines(&post.message);
    if let Some(list) = post.props.get("attachments").and_then(|a| a.as_array()) {
        for a in list {
            let field = |k: &str| a.get(k).and_then(|v| v.as_str()).unwrap_or_default();
            lines += 1 + text_lines(field("pretext")) + text_lines(field("text"));
            lines += a.get("fields").and_then(|f| f.as_array()).map_or(0, |f| f.len().div_ceil(2));
            if !field("image_url").is_empty() {
                lines += 6;
            }
        }
    }
    lines + post.metadata.files.len() * 3
}

/// Ouvre un lien d'un message : seulement le web et le courriel. Jamais
/// `file:`, un chemin local ou un protocole qui lancerait un programme.
fn open_link(url: &str, cx: &mut App) {
    cx.stop_propagation();
    if is_safe_link(url) {
        cx.open_url(url);
    }
}

fn is_safe_link(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    ["https://", "http://", "mailto:"].iter().any(|p| lower.starts_with(p))
}

#[cfg(test)]
mod link_safety_tests {
    use super::is_safe_link;

    #[test]
    fn seuls_web_et_courriel() {
        assert!(is_safe_link("https://github.com/x"));
        assert!(is_safe_link("HTTP://exemple.org"));
        assert!(is_safe_link("mailto:a@b.fr"));
        assert!(!is_safe_link("file:///C:/Windows/System32/calc.exe"));
        assert!(!is_safe_link("C:\\Windows\\notepad.exe"));
        assert!(!is_safe_link("ms-settings:"));
        assert!(!is_safe_link("javascript:alert(1)"));
    }
}

/// Critère d'appartenance à un groupe de la liste des membres.
type MemberFilter<'a> = Box<dyn Fn(&String) -> bool + 'a>;

/// Résumé d'une notification de bot sur une ligne : chapeau et titre de la
/// pièce jointe (sinon son texte, sinon le message), et sa couleur.
fn bot_summary(post: &Post) -> (String, Option<String>) {
    // Markdown d'une ligne : sans titre « ### », citation « > » ni puce, liens gardés.
    let clean = |l: &str| {
        // Titre Markdown (« ## Titre ») ou citation : marqueur retiré ; « #18342 » reste.
        let mut l = l.trim();
        let hashes = l.len() - l.trim_start_matches('#').len();
        if hashes > 0 && l[hashes..].starts_with(' ') {
            l = l[hashes..].trim_start();
        }
        let l = l.trim_start_matches('>').trim();
        let l = l.strip_prefix("- ").or_else(|| l.strip_prefix("* ")).unwrap_or(l);
        l.trim().to_string()
    };
    let first_line = |t: &str| t.lines().map(clean).find(|l| !l.is_empty()).unwrap_or_default();
    if let Some(a) = rich::attachments(post).into_iter().next() {
        let title = first_line(&a.title);
        let title =
            if title.is_empty() || a.title_link.is_empty() { title } else { format!("[{title}]({})", a.title_link) };
        let parts: Vec<String> = [first_line(&a.pretext), title].into_iter().filter(|s| !s.is_empty()).collect();
        let summary = if parts.is_empty() {
            [first_line(&a.text), first_line(&a.fallback), first_line(&post.message)]
                .into_iter()
                .find(|s| !s.is_empty())
                .unwrap_or_default()
        } else {
            parts.join(" — ")
        };
        let color = (!a.color.is_empty()).then(|| a.color.clone());
        return (summary, color);
    }
    // Message Markdown (plugin GitHub) : les deux premières lignes, réunies.
    let lines: Vec<String> = post.message.lines().map(clean).filter(|l| !l.is_empty()).take(2).collect();
    (lines.join(" — "), None)
}

#[cfg(test)]
mod bot_tests {
    use super::bot_summary;
    use crate::api::Post;

    #[test]
    fn plugin_github_reconnu_et_resume() {
        let post = Post {
            message: "#### Corriger l'export\n##### [glpi/glpi#12](https://x) nouvelle pull request\n\nCorps".into(),
            kind: "custom_git_pr".into(),
            props: serde_json::json!({ "from_plugin": true }),
            ..Default::default()
        };
        assert!(post.is_bot_post());
        let (summary, _) = bot_summary(&post);
        assert!(summary.starts_with("Corriger l'export — "), "{summary}");
        assert!(summary.contains("glpi/glpi#12"), "{summary}");
        // Un message ordinaire n'est pas un bot.
        assert!(!Post { message: "salut".into(), ..Default::default() }.is_bot_post());
    }
}

/// Fond des en-têtes de droite : prolonge le dégradé de l'en-tête du canal.
fn right_header_bg(p: &Palette) -> Background {
    linear_gradient(
        90.,
        linear_color_stop(p.button_bg.opacity(0.07), 0.),
        linear_color_stop(p.button_bg.opacity(0.03), 1.),
    )
}

/// Couleur d'une notification de bot sans pièce jointe (plugin GitHub),
/// d'après son contenu, comme les pastilles de GitHub.
fn notification_color(message: &str) -> Option<Hsla> {
    // Seulement le texte de l'événement : sans les lignes de titre (« #### … »)
    // ni le texte des liens (titres de PR ou de ticket, qui peuvent contenir
    // « fusionné », « fermé »…).
    let link = regex_lite_links();
    let m: String = message
        .lines()
        .filter(|l| {
            let hashes = l.trim_start().chars().take_while(|c| *c == '#').count();
            !(1..=4).contains(&hashes)
        })
        .map(|l| link.replace_all(l, " ").to_string())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| m.contains(w));
    let hex = if has(&["merged", "fusionn"]) {
        "#8250df"
    } else if has(&["requested changes", "changes requested", "modifications demand"]) {
        "#bc4c00"
    } else if has(&["closed", "fermé", "fermée"]) {
        "#cf222e"
    } else if has(&[
        "approved",
        "approuv",
        "new pull request",
        "opened",
        "reopened",
        "new issue",
        "nouvelle pull request",
        "nouveau ticket",
        "ouvert",
    ]) {
        "#1a7f37"
    } else if has(&["comment", "review", "revue"]) {
        "#0969da"
    } else if has(&["pushed", "push", "commit"]) {
        "#57606a"
    } else if has(&["release", "deploy", "déploi"]) {
        "#0e8a16"
    } else {
        return None;
    };
    crate::theme::parse_color(hex)
}

/// `[texte](adresse)` : un lien Markdown entier.
fn regex_lite_links() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"\[[^\]]*\]\([^)]*\)").expect("motif valide"))
}

#[cfg(test)]
mod notification_color_tests {
    use super::notification_color;

    #[test]
    fn couleurs_github() {
        let c = |t: &str| notification_color(t).map(|h| (h.h * 360.).round() as i32);
        assert!(c("[glpi] Pull request #12 merged by lea").is_some());
        assert_ne!(c("Pull request merged"), c("Pull request closed"));
        assert_eq!(c("New pull request #3 opened"), c("Review approved"));
        assert_eq!(c("salut à tous"), None);
        // Le titre de la PR ne compte pas : nouvelle PR (vert), pas fusionnée (violet).
        let pr = "#### Corriger les tickets fusionnés
##### [glpi#12](https://x) nouvelle pull request de [lea](https://y)";
        assert_eq!(c(pr), c("New pull request"));
        let comment =
            "[glpi] Nouveau commentaire de [lea](https://y) sur [#12 Corriger les tickets fusionnés](https://x) :";
        assert_eq!(c(comment), c("commented"));
    }
}

/// La notification a-t-elle plus à montrer que son résumé d'une ligne ?
/// `fits` : nombre de caractères qui tiennent sur la ligne (selon la largeur).
fn bot_expandable(post: &Post, summary: &str, fits: usize) -> bool {
    let long_summary = markdown::plain_text(summary).chars().count() > fits;
    if let Some(a) = rich::attachments(post).into_iter().next() {
        let more =
            !a.text.trim().is_empty() || !a.fields.is_empty() || !a.image_url.is_empty() || !a.actions.is_empty();
        return more || long_summary || rich::attachments(post).len() > 1;
    }
    let lines = post.message.lines().filter(|l| !l.trim().is_empty()).count();
    lines > 2 || long_summary || !post.metadata.files.is_empty()
}

#[cfg(test)]
mod expand_tests {
    use super::{bot_expandable, bot_summary};
    use crate::api::Post;

    #[test]
    fn une_ligne_sans_repli() {
        let short = Post { message: "[glpi] lea a approuvé #12".into(), ..Default::default() };
        assert!(!bot_expandable(&short, &bot_summary(&short).0, 90));
        // Une ligne plus longue que la place disponible : dépliable.
        let one = Post {
            message: "[glpi] Nouvelle pull request #12 « Refonte complète du moteur de recherche » ouverte par lea"
                .into(),
            ..Default::default()
        };
        assert!(!bot_expandable(&one, &bot_summary(&one).0, 120));
        assert!(bot_expandable(&one, &bot_summary(&one).0, 40));
        let long = Post {
            message: "#### Titre\n##### [glpi#12](https://x) nouvelle PR\n\nCorps de la PR".into(),
            ..Default::default()
        };
        assert!(bot_expandable(&long, &bot_summary(&long).0, 90));
        // Un numéro « #18342 » n'est pas un titre Markdown.
        let pr = Post { message: "[glpi] #18342 Refonte".into(), ..Default::default() };
        assert_eq!(bot_summary(&pr).0, "[glpi] #18342 Refonte");
        let titre = Post { message: "#### Titre".into(), ..Default::default() };
        assert_eq!(bot_summary(&titre).0, "Titre");
    }
}

/// Sens des dégradés : vers le clair en thème sombre, vers le foncé sinon.
fn depth(p: &Palette, amount: f32) -> f32 {
    if p.center_bg.l < 0.5 { amount } else { -amount }
}

/// Chiffres d'une pastille, centrés sur leur dessin et non sur leur boîte :
/// certains glyphes (le « 1 » de Segoe UI…) ont leur encre décalée dans leur
/// largeur. Le décalage est calculé avec les mesures de la police.
fn centered_digits(text: String, size: f32, cx: &App) -> Div {
    let shift = ink_shift(&text, px(size), cx).unwrap_or_default();
    div().relative().left(shift).child(text)
}

/// Écart horizontal entre le centre de la boîte d'un texte et le centre de son encre.
fn ink_shift(text: &str, size: Pixels, cx: &App) -> Option<Pixels> {
    let system = cx.text_system();
    let mut font = font(cx.theme().font_family.clone());
    font.weight = FontWeight::BOLD;
    let id = system.resolve_font(&font);
    let (mut x, mut left, mut right) = (px(0.), None, px(0.));
    for c in text.chars() {
        let ink = system.typographic_bounds(id, size, c).ok()?;
        let advance = system.advance(id, size, c).ok()?.width;
        left.get_or_insert(x + ink.origin.x);
        right = x + ink.origin.x + ink.size.width;
        x += advance;
    }
    let shift = x / 2. - (left? + right) / 2.;
    // Au pixel entier : un décalage de moins d'un pixel serait arrondi à rien
    // à l'affichage. Garde-fou : jamais plus d'un quart de la taille du texte.
    let shift = if shift > px(0.2) {
        shift.ceil()
    } else if shift < px(-0.2) {
        shift.floor()
    } else {
        px(0.)
    };
    Some(shift.clamp(-size / 4., size / 4.))
}
