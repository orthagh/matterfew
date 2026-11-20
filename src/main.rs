// En version optimisée, pas de console noire à côté de la fenêtre sous Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod backend;
mod credentials;
mod demo;
mod emoji;
mod markdown;
mod notify;
mod sidebar;
mod sso;
mod theme;
mod ui;

use gpui_kit::*;

fn main() {
    // Fenêtre de connexion « via Mattermost » (processus séparé, voir sso.rs).
    let args: Vec<String> = std::env::args().collect();
    if let Some(ix) = args.iter().position(|a| a == "--sso-login") {
        let url = args.get(ix + 1).cloned().unwrap_or_default();
        sso::run_login_window(&url);
    }

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            // Thème clair ou sombre selon le réglage du système.
            gpui_kit::component::Theme::sync_system_appearance(None, cx);

            let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Mattermost — client GPUI".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };

            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| ui::ChatApp::new(window, cx))
            })
            .expect("impossible d'ouvrir la fenêtre");

            // Fermer la fenêtre quitte l'application.
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
        });
}
