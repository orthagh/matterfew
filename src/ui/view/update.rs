//! Mise à jour de l'application : vérification au lancement puis toutes
//! les 6 heures, bandeau « Matterfew x.y.z est disponible », téléchargement
//! avec progression, installation et redémarrage sur un clic.

use std::time::Duration;

use super::*;
use crate::updater::{self, Available, Install};

/// Délai avant la première vérification, puis intervalle.
const FIRST_CHECK: Duration = Duration::from_secs(30);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);

/// Où en est la mise à jour proposée.
pub(crate) enum UpdateState {
    Available(Available),
    Downloading { version: String, received: u64, total: Option<u64> },
    Installing { version: String },
    Failed { version: String, error: String },
}

impl UpdateState {
    fn version(&self) -> &str {
        match self {
            UpdateState::Available(a) => &a.version,
            UpdateState::Downloading { version, .. }
            | UpdateState::Installing { version }
            | UpdateState::Failed { version, .. } => version,
        }
    }
}

impl ChatApp {
    /// Vérifications périodiques (si le réglage le permet).
    pub(crate) fn start_update_checks(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FIRST_CHECK).await;
            loop {
                let go = this.update(cx, |this, cx| {
                    if this.settings.check_updates && !this.demo {
                        this.check_for_update(false, cx);
                    }
                });
                if go.is_err() {
                    break;
                }
                cx.background_executor().timer(CHECK_EVERY).await;
            }
        })
        .detach();
    }

    /// Cherche une nouvelle version ; `manual` : dire aussi « à jour » ou l'erreur.
    pub(crate) fn check_for_update(&mut self, manual: bool, cx: &mut Context<Self>) {
        if self.demo {
            // Démo : une version fictive, installable.
            self.on_update_checked(
                Ok(Some(Available { version: "9.9.9".into(), notes: String::new(), installable: true })),
                manual,
                cx,
            );
            return;
        }
        let result = super::super::in_background(updater::check);
        cx.spawn(async move |this, cx| {
            if let Ok(result) = result.recv().await {
                let _ = this.update(cx, |this, cx| this.on_update_checked(result, manual, cx));
            }
        })
        .detach();
    }

    fn on_update_checked(&mut self, result: anyhow::Result<Option<Available>>, manual: bool, cx: &mut Context<Self>) {
        // Téléchargement ou installation en cours : on ne l'interrompt pas.
        if matches!(self.update, Some(UpdateState::Downloading { .. } | UpdateState::Installing { .. })) {
            return;
        }
        match result {
            Ok(Some(found)) => {
                if manual || self.update_dismissed.as_deref() != Some(found.version.as_str()) {
                    self.update = Some(UpdateState::Available(found));
                }
            }
            Ok(None) if manual => {
                self.flash(t!("update.up_to_date", version = env!("CARGO_PKG_VERSION")).to_string(), cx)
            }
            Err(err) if manual => self.flash(t!("update.check_failed", error = format!("{err:#}")).to_string(), cx),
            _ => {}
        }
        cx.notify();
    }

    fn dismiss_update(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.update.take() {
            self.update_dismissed = Some(state.version().to_string());
        }
        cx.notify();
    }

    /// Télécharge, vérifie, installe, puis redémarre l'application.
    pub(crate) fn install_update(&mut self, cx: &mut Context<Self>) {
        let Some(UpdateState::Available(found)) = &self.update else { return };
        let version = found.version.clone();
        self.update = Some(UpdateState::Downloading { version: version.clone(), received: 0, total: None });
        cx.notify();
        if self.demo {
            self.simulate_update(version, cx);
            return;
        }
        let (tx, progress) = async_channel::unbounded::<(u64, Option<u64>)>();
        let download = super::super::in_background(move || {
            let last = std::cell::Cell::new(0u64);
            updater::download(|received, total| {
                // Une nouvelle valeur par point de pourcentage, pas par bloc lu.
                let step = total.map_or(256 << 10, |t| (t / 100).max(1));
                if received / step != last.get() / step {
                    last.set(received);
                    let _ = tx.try_send((received, total));
                }
            })
        });
        cx.spawn(async move |this, cx| {
            while let Ok((received, total)) = progress.recv().await {
                let _ = this.update(cx, |this, cx| {
                    if let Some(UpdateState::Downloading { received: r, total: t, .. }) = this.update.as_mut() {
                        (*r, *t) = (received, total);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let Ok(result) = download.recv().await else { return };
            let prepared = match result {
                Ok(prepared) => prepared,
                Err(err) => {
                    let _ = this.update(cx, |this, cx| this.update_failed(version, err, cx));
                    return;
                }
            };
            // Avant de quitter : ce qui doit survivre au redémarrage.
            let _ = this.update(cx, |this, cx| {
                this.store_cache();
                this.settings.save();
                this.update = Some(UpdateState::Installing { version: version.clone() });
                cx.notify();
            });
            let applied = super::super::in_background(move || updater::apply(prepared));
            let Ok(result) = applied.recv().await else { return };
            let _ = this.update(cx, |this, cx| match result {
                // La nouvelle version est lancée : celle-ci s'arrête.
                Ok(()) => cx.quit(),
                Err(err) => this.update_failed(version, err, cx),
            });
        })
        .detach();
    }

    fn update_failed(&mut self, version: String, err: anyhow::Error, cx: &mut Context<Self>) {
        self.update = Some(UpdateState::Failed { version, error: format!("{err:#}") });
        cx.notify();
    }

    /// Démo : progression simulée (sur un fil à part, comme le vrai
    /// téléchargement), sans rien installer.
    fn simulate_update(&mut self, version: String, cx: &mut Context<Self>) {
        let (tx, progress) = async_channel::unbounded::<(u64, Option<u64>)>();
        std::thread::spawn(move || {
            let total: u64 = 42 << 20;
            for step in 1..=20u64 {
                std::thread::sleep(Duration::from_millis(60));
                let _ = tx.send_blocking((total * step / 20, Some(total)));
            }
        });
        cx.spawn(async move |this, cx| {
            while let Ok((received, total)) = progress.recv().await {
                let _ = this.update(cx, |this, cx| {
                    if let Some(UpdateState::Downloading { received: r, total: t, .. }) = this.update.as_mut() {
                        (*r, *t) = (received, total);
                        cx.notify();
                    }
                });
            }
            let _ = this.update(cx, |this, cx| {
                this.update = Some(UpdateState::Installing { version });
                this.flash(t!("update.demo_done").to_string(), cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Bandeau au-dessus des messages.
    pub(crate) fn render_update_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.update.as_ref()?;
        let p = self.palette;
        let version = state.version().to_string();
        let install = self.install_kind;
        let notes_button = |id: &'static str| {
            let version = version.clone();
            Button::new(id)
                .label(t!("update.notes"))
                .xsmall()
                .ghost()
                .on_click(move |_, _, cx| open_link(&updater::release_page(&version), cx))
        };
        let later = Button::new("update-later")
            .icon(Icon::new(IconName::X))
            .xsmall()
            .ghost()
            .accessibility_label(t!("update.later"))
            .tooltip(t!("update.later"))
            .on_click(cx.listener(|this, _, _, cx| this.dismiss_update(cx)));
        let row = h_flex()
            .id("update-banner")
            .flex_none()
            .px_5()
            .py_1p5()
            .gap_2()
            .bg(p.button_bg.opacity(0.12))
            .secondary_size()
            .role(Role::Status)
            .child(Icon::new(IconName::Download).size(px(15.)).text_color(p.button_bg));
        let row = match state {
            UpdateState::Available(found) => {
                let (text, action): (SharedString, Option<AnyElement>) = if found.installable {
                    (
                        t!("update.available", version = version).into(),
                        Some(
                            Button::new("update-install")
                                .label(t!("update.install"))
                                .xsmall()
                                .primary()
                                .on_click(cx.listener(|this, _, _, cx| this.install_update(cx)))
                                .into_any_element(),
                        ),
                    )
                } else if install == Install::Flatpak {
                    (t!("update.available_flatpak", version = version).into(), None)
                } else {
                    let version = version.clone();
                    (
                        t!("update.available", version = version).into(),
                        Some(
                            Button::new("update-download")
                                .label(t!("update.download"))
                                .xsmall()
                                .primary()
                                .on_click(move |_, _, cx| open_link(&updater::release_page(&version), cx))
                                .into_any_element(),
                        ),
                    )
                };
                row.child(div().flex_1().min_w_0().truncate().child(text))
                    .child(notes_button("update-notes"))
                    .children(action)
                    .child(later)
            }
            UpdateState::Downloading { received, total, .. } => {
                let fraction = total.map(|t| (*received as f32 / t.max(1) as f32).clamp(0., 1.));
                let label = match total {
                    Some(t) => t!(
                        "update.downloading",
                        version = version,
                        done = crate::api::human_size(*received as i64),
                        total = crate::api::human_size(*t as i64)
                    ),
                    None => t!("update.downloading_unknown", version = version),
                };
                row.child(div().flex_none().child(label)).child(
                    div()
                        .flex_1()
                        .h(px(6.))
                        .rounded_full()
                        .bg(p.button_bg.opacity(0.18))
                        .child(div().h_full().rounded_full().bg(p.button_bg).w(relative(fraction.unwrap_or(0.05)))),
                )
            }
            UpdateState::Installing { .. } => {
                row.child(div().flex_1().child(t!("update.installing", version = version)))
            }
            UpdateState::Failed { error, .. } => row
                .bg(p.error.opacity(0.12))
                .child(div().flex_1().min_w_0().truncate().child(t!("update.failed", error = error)))
                .child(notes_button("update-notes-failed"))
                .child(later),
        };
        Some(row.into_any_element())
    }
}
