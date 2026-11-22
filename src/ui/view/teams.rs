//! Équipes : en rejoindre une autre, quitter, paramètres de l'équipe
//! (nom, description, ouverte à tous) ; actions de la carte d'équipe.

use gpui_kit::component::input::InputState;
use gpui_kit::component::switch::Switch;

use super::dialogs::Dialog;
use super::*;
use crate::api::Team;
use crate::backend::Command;

impl ChatApp {
    pub(crate) fn open_join_team(&mut self, cx: &mut Context<Self>) {
        self.team_card = None;
        self.dialog = Some(Dialog::JoinTeam { teams: None, joining: None });
        self.send(Command::LoadJoinableTeams);
        cx.notify();
    }

    pub(crate) fn on_joinable_teams(&mut self, teams: Vec<Team>) {
        if let Some(Dialog::JoinTeam { teams: list, .. }) = self.dialog.as_mut() {
            let mine: Vec<&str> = self.teams.iter().map(|t| t.id.as_str()).collect();
            let mut others: Vec<Team> = teams.into_iter().filter(|t| !mine.contains(&t.id.as_str())).collect();
            others.sort_by_key(|t| t.display_name.to_lowercase());
            *list = Some(others);
        }
    }

    pub(crate) fn join_team(&mut self, team_id: String, cx: &mut Context<Self>) {
        if let Some(Dialog::JoinTeam { joining, .. }) = self.dialog.as_mut() {
            *joining = Some(team_id.clone());
        }
        self.send(Command::JoinTeam { team_id });
        cx.notify();
    }

    /// Équipe rejointe : ajoutée à la liste et ouverte.
    pub(crate) fn on_team_joined(&mut self, team: Team, cx: &mut Context<Self>) {
        if matches!(self.dialog, Some(Dialog::JoinTeam { .. })) {
            self.dialog = None;
        }
        let id = team.id.clone();
        if !self.teams.iter().any(|t| t.id == id) {
            self.teams.push(team);
        }
        self.select_team(id, cx);
        cx.notify();
    }

    pub(crate) fn confirm_leave_team(&mut self, team_id: String, cx: &mut Context<Self>) {
        self.team_card = None;
        self.dialog = Some(Dialog::LeaveTeam { team_id });
        cx.notify();
    }

    pub(crate) fn leave_team(&mut self, team_id: String, cx: &mut Context<Self>) {
        self.dialog = None;
        self.send(Command::LeaveTeam { team_id: team_id.clone() });
        self.teams.retain(|t| t.id != team_id);
        if self.team_id.as_deref() == Some(team_id.as_str())
            && let Some(next) = self.teams.first().map(|t| t.id.clone())
        {
            self.select_team(next, cx);
        }
        cx.notify();
    }

    pub(crate) fn open_team_settings(&mut self, team_id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(team) = self.teams.iter().find(|t| t.id == team_id).cloned() else { return };
        self.team_card = None;
        self.dialog = Some(Dialog::TeamSettings { team_id, open: team.allow_open_invite || team.team_type == "O" });
        self.dialog_comment.update(cx, |s: &mut InputState, cx| {
            s.set_placeholder(t!("team.description_placeholder"), window, cx);
            s.set_value(team.description.clone(), window, cx);
        });
        self.dialog_input.update(cx, |s: &mut InputState, cx| {
            s.set_placeholder(t!("teams.name_placeholder"), window, cx);
            s.set_value(team.display_name.clone(), window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    pub(crate) fn save_team_settings(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::TeamSettings { team_id, open }) = self.dialog.as_ref() else { return };
        let (team_id, open) = (team_id.clone(), *open);
        let display_name = self.dialog_input.read(cx).value().trim().to_string();
        if display_name.chars().count() < 2 {
            self.flash(t!("teams.name_invalid").to_string(), cx);
            return;
        }
        let description = self.dialog_comment.read(cx).value().trim().to_string();
        self.send(Command::SaveTeamSettings { team_id, display_name, description, open });
        self.dialog = None;
        cx.notify();
    }

    /// Actions au bas de la carte d'équipe.
    pub(crate) fn render_team_actions(&self, team_id: &str, admin: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let row = |id: &'static str, icon: IconName, label: SharedString, danger: bool| {
            h_flex()
                .id(id)
                .gap_2p5()
                .px_5()
                .py_2()
                .cursor_pointer()
                .text_sm()
                .role(gpui_kit::Role::Button)
                .aria_label(label.clone())
                .when(danger, |el| el.text_color(p.error))
                .hover(|s| s.bg(p.hover()))
                .child(Icon::new(icon).size(px(16.)))
                .child(label)
        };
        let id = team_id.to_string();
        v_flex()
            .py_1()
            .border_t_1()
            .border_color(p.border())
            .when(admin, |el| {
                let id = id.clone();
                el.child(
                    row("team-settings", IconName::Settings, t!("teams.settings").into(), false).on_click(
                        cx.listener(move |this, _, window, cx| this.open_team_settings(id.clone(), window, cx)),
                    ),
                )
            })
            .child(
                row("team-join", IconName::LogIn, t!("teams.join").into(), false)
                    .on_click(cx.listener(|this, _, _, cx| this.open_join_team(cx))),
            )
            .when(self.teams.len() > 1, |el| {
                el.child(
                    row("team-leave", IconName::LogOut, t!("teams.leave").into(), true)
                        .on_click(cx.listener(move |this, _, _, cx| this.confirm_leave_team(id.clone(), cx))),
                )
            })
            .into_any_element()
    }

    pub(crate) fn render_join_team(
        &self,
        teams: Option<&Vec<Team>>,
        joining: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let body: AnyElement = match teams {
            None => div().px_4().pb_4().text_sm().text_color(p.muted()).child(t!("common.loading")).into_any_element(),
            Some(list) if list.is_empty() => v_flex()
                .px_4()
                .pb_5()
                .gap_1()
                .child(div().text_sm().font_semibold().child(t!("teams.none")))
                .child(div().text_sm().text_color(p.muted()).child(t!("teams.none_hint")))
                .into_any_element(),
            Some(list) => v_flex()
                .id("join-team-list")
                .max_h(px(360.))
                .overflow_y_scroll()
                .pb_2()
                .children(list.iter().map(|team| {
                    let busy = joining == Some(team.id.as_str());
                    let initials: String = team
                        .display_name
                        .split_whitespace()
                        .filter_map(|w| w.chars().next().filter(|c| c.is_alphanumeric()))
                        .take(2)
                        .collect::<String>()
                        .to_uppercase();
                    let id = team.id.clone();
                    h_flex()
                        .px_4()
                        .py_2()
                        .gap_3()
                        .child(
                            div()
                                .flex_none()
                                .size(px(36.))
                                .rounded(px(9.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(p.button_bg.opacity(0.14))
                                .text_color(p.button_bg)
                                .font_bold()
                                .child(initials),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(div().truncate().text_sm().font_semibold().child(team.display_name.clone()))
                                .child(div().truncate().text_xs().text_color(p.muted()).child(
                                    if team.description.is_empty() {
                                        t!("common.no_description").to_string()
                                    } else {
                                        team.description.clone()
                                    },
                                )),
                        )
                        .child(
                            Button::new(SharedString::from(format!("join-{}", team.id)))
                                .label(if busy { t!("teams.joining") } else { t!("teams.join_button") })
                                .small()
                                .primary()
                                .disabled(joining.is_some())
                                .on_click(cx.listener(move |this, _, _, cx| this.join_team(id.clone(), cx))),
                        )
                }))
                .into_any_element(),
        };
        self.dialog_shell(IconName::LogIn, t!("teams.join_title").into(), 480., body, None, cx)
    }

    pub(crate) fn render_leave_team(&self, team_id: &str, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let name = self.teams.iter().find(|t| t.id == team_id).map(|t| t.display_name.clone()).unwrap_or_default();
        let content = div()
            .px_4()
            .pb_4()
            .text_sm()
            .text_color(p.muted())
            .child(t!("teams.leave_confirm", team = name))
            .into_any_element();
        let id = team_id.to_string();
        let footer = h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(div().flex_1())
            .child(
                Button::new("leave-team-cancel")
                    .label(t!("common.cancel"))
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.close_dialog(cx))),
            )
            .child(
                Button::new("leave-team-ok")
                    .label(t!("teams.leave_button"))
                    .small()
                    .danger()
                    .on_click(cx.listener(move |this, _, _, cx| this.leave_team(id.clone(), cx))),
            )
            .into_any_element();
        self.dialog_shell(IconName::LogOut, t!("teams.leave_title").into(), 420., content, Some(footer), cx)
    }

    pub(crate) fn render_team_settings(&self, open: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let label = |text: SharedString| div().text_xs().font_semibold().text_color(p.muted()).child(text);
        let content = v_flex()
            .px_4()
            .pb_4()
            .gap_2()
            .child(label(t!("teams.name").into()))
            .child(Input::new(&self.dialog_input))
            .child(div().h(px(4.)))
            .child(label(t!("common.description").into()))
            .child(Input::new(&self.dialog_comment))
            .child(div().h(px(4.)))
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().text_sm().child(t!("teams.open")))
                            .child(div().text_xs().text_color(p.muted()).child(t!("teams.open_hint"))),
                    )
                    .child(Switch::new("team-open").checked(open).on_click(cx.listener(
                        |this, checked: &bool, _, cx| {
                            if let Some(Dialog::TeamSettings { open, .. }) = this.dialog.as_mut() {
                                *open = *checked;
                            }
                            cx.notify();
                        },
                    ))),
            )
            .into_any_element();
        let footer = h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(div().flex_1())
            .child(
                Button::new("team-settings-cancel")
                    .label(t!("common.cancel"))
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.close_dialog(cx))),
            )
            .child(
                Button::new("team-settings-save")
                    .label(t!("common.save"))
                    .small()
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| this.save_team_settings(cx))),
            )
            .into_any_element();
        self.dialog_shell(IconName::Settings, t!("teams.settings_title").into(), 460., content, Some(footer), cx)
    }
}
