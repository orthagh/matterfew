//! Rendu : écran de connexion, barre latérale, messages, fil, saisie.
//!
//! La mise en page suit celle du client web Mattermost et ses couleurs
//! viennent du thème de l'utilisateur (voir `theme.rs`).

use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, StyledExt as _,
    avatar::Avatar,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, Textarea},
    message_scroller::MessageScroller,
    text::TextView,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{ChatApp, Pane, Screen};
use crate::api::{Post, User};
use crate::backend::group_reactions;
use crate::sidebar::{self, Section};
use crate::theme::Palette;
use crate::{emoji, markdown};

/// Réactions proposées dans la barre d'actions d'un message.
const QUICK_REACTIONS: [&str; 6] = ["+1", "heart", "joy", "tada", "eyes", "white_check_mark"];

const SIDEBAR_WIDTH: f32 = 260.;
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

    /// Petit bouton texte pour les fonds colorés (barre latérale), où les
    /// boutons standards, prévus pour la zone centrale, seraient illisibles.
    fn sidebar_button(
        id: &'static str,
        label: &'static str,
        p: &Palette,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        div()
            .id(id)
            .px_2()
            .py_1()
            .rounded(px(4.))
            .text_xs()
            .cursor_pointer()
            .text_color(p.sidebar_text.opacity(0.72))
            .hover(|s| s.bg(p.sidebar_text.opacity(0.12)).text_color(p.sidebar_text))
            .child(label)
            .on_click(on_click)
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
                    div()
                        .id(SharedString::from(format!("team-{}", team.id)))
                        .size(px(44.))
                        .p(px(3.))
                        .rounded(px(10.))
                        .border_2()
                        .border_color(if active { p.sidebar_text } else { transparent_black() })
                        .cursor_pointer()
                        .child(
                            div()
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
                                .child(initials),
                        )
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
                .avatar(&partner, 20., if selected { p.sidebar_hover } else { p.sidebar_bg })
                .into_any_element(),
            None => div()
                .w(px(20.))
                .flex_none()
                .text_center()
                .text_color(text_color.opacity(0.8))
                .child(match channel.kind.as_str() {
                    "P" => "🔒",
                    "G" => "👥",
                    _ => "#",
                })
                .into_any_element(),
        };

        let id = channel_id.to_string();
        Some(
            h_flex()
                .id(SharedString::from(format!("ch-{channel_id}")))
                .h(px(32.))
                .gap_2()
                .pl(px(16.))
                .pr_3()
                .cursor_pointer()
                .text_size(px(14.))
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
                .into_any_element(),
        )
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
            let header = h_flex()
                .id(SharedString::from(format!("cat-{}", section.id)))
                .h(px(28.))
                .mt_2()
                .gap_1()
                .px_3()
                .text_size(px(12.))
                .font_semibold()
                .text_color(p.sidebar_text.opacity(0.64))
                .when(section.collapsible, |el| {
                    el.cursor_pointer()
                        .hover(|s| s.text_color(p.sidebar_text))
                        .child(div().w(px(12.)).text_size(px(10.)).child(if section.collapsed { "▶" } else { "▼" }))
                        .on_click(cx.listener(move |this, _, _, cx| this.toggle_category(&category_id, cx)))
                })
                .when(!section.collapsible, |el| el.pl(px(28.)))
                .child(section.title.clone());
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
        let logout = cx.listener(|this, _, window, cx| this.logout(window, cx));

        v_flex()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .flex_none()
            .bg(p.sidebar_bg)
            .text_color(p.sidebar_text)
            .child(
                h_flex()
                    .h(px(56.))
                    .flex_none()
                    .px_4()
                    .bg(p.sidebar_header_bg)
                    .text_color(p.sidebar_header_text)
                    .text_size(px(16.))
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
                    .child(Self::sidebar_button("logout", "Déconnexion", &p, logout)),
            )
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
        // Messages successifs du même auteur (< 5 min, même jour) : en-tête masqué.
        let compact = !new_day
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

        let header = h_flex()
            .gap_2()
            .items_baseline()
            .child(div().text_size(px(15.)).font_semibold().child(self.user_name(&post.user_id)))
            .child(div().text_xs().text_color(p.muted()).child(format_time(post.create_at)));

        let body: AnyElement = if post.is_system() {
            div()
                .italic()
                .text_size(px(BODY_TEXT))
                .text_color(p.muted())
                .child(markdown::plain_text(&post.message))
                .into_any_element()
        } else {
            div()
                .text_size(px(BODY_TEXT))
                .child(
                    TextView::markdown(ElementId::Name(key("body")), view.md.clone()).on_link_click(|url, _, _, cx| {
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
                .child(div().text_size(px(14.)).child(emoji::lookup(&name)))
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
                    .when(!post.metadata.reactions.is_empty(), |el| el.child(reaction_row))
                    .children(replies),
            )
            .children(toolbar);

        if new_day {
            v_flex().w_full().child(date_separator(post.create_at, &p)).child(row).into_any_element()
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

        let mut bar = h_flex()
            .absolute()
            .top(-px(14.))
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
            bar = bar.child(Button::new(id("reply")).label("Répondre").xsmall().ghost().on_click(
                move |_, window, cx| entity.update(cx, |this, cx| this.open_thread(pid.clone(), window, cx)),
            ));
        }
        if mine {
            let (e1, e2) = (entity.clone(), entity.clone());
            let (p1, p2) = (post_id.clone(), post_id.clone());
            bar = bar
                .child(Button::new(id("edit")).label("Modifier").xsmall().ghost().on_click(move |_, window, cx| {
                    e1.update(cx, |this, cx| this.start_edit(pane, p1.clone(), window, cx))
                }))
                .child(Button::new(id("delete")).label("Supprimer").xsmall().ghost().on_click(move |_, _, cx| {
                    e2.update(cx, |this, cx| {
                        this.confirm_delete = Some(p2.clone());
                        cx.notify();
                    })
                }));
        }
        bar
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

        v_flex()
            .flex_none()
            .px_5()
            .pb_1()
            .pt_1()
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
                    .child(div().px_1().pt_1().child(Textarea::new(&state).appearance(false)))
                    .child(
                        h_flex()
                            .px_3()
                            .pb_2()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(11.))
                                    .text_color(p.muted())
                                    .child("Entrée pour envoyer · Maj+Entrée pour aller à la ligne"),
                            )
                            .child(
                                Button::new(SharedString::from(format!("send-{pane:?}")))
                                    .label(if editing { "Enregistrer" } else { "Envoyer" })
                                    .small()
                                    .primary()
                                    .on_click(cx.listener(move |this, _, window, cx| this.send_from(pane, window, cx))),
                            ),
                    ),
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

    fn render_channel_header(&self) -> impl IntoElement {
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
        let icon = state.map(|c| match c.channel.kind.as_str() {
            "O" => "#",
            "P" => "🔒",
            _ => "",
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
                            .when_some(icon.filter(|i| !i.is_empty()), |el, icon| {
                                el.child(div().text_color(p.muted()).child(icon))
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
                        .label("✕")
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

        let main = v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(p.center_bg)
            .text_color(p.center_color)
            .child(self.render_channel_header())
            .children(error_banner)
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
                        .flex_none()
                        .px_5()
                        .gap_2()
                        .border_b_1()
                        .border_color(p.border())
                        .child(div().text_size(px(16.)).font_bold().child("Fil"))
                        .child(div().flex_1().truncate().text_size(px(12.)).text_color(p.muted()).child(channel))
                        .child(
                            Button::new("close-thread")
                                .label("✕")
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

        h_flex()
            .size_full()
            .bg(p.center_bg)
            .children(self.render_team_bar(cx))
            .child(self.render_sidebar(cx))
            .child(main)
            .children(thread_panel)
    }
}

impl Render for ChatApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
