//! Tests de bout en bout de l'interface, sans écran : la vraie `ChatApp`
//! dans une fenêtre de test GPUI, branchée sur le serveur de démonstration
//! (jamais sur un vrai compte), pilotée au clavier comme par un utilisateur.
//!
//! `cargo test e2e` pour ne lancer qu'eux.

use std::time::{Duration, Instant};

use gpui_kit::{TestApp, TestAppWindow, point, px};

use super::{ChatApp, Launch, Pane, Screen};

/// Application de test avec la démo connectée et le canal « Général » ouvert.
fn demo_app() -> (TestApp, TestAppWindow<ChatApp>) {
    let mut app = TestApp::new();
    app.update(gpui_kit::init);
    let window = app.open_window(|window, cx| ChatApp::with_launch(Launch::Demo, window, cx));
    let w = (app, window);
    wait_for(&w, "connexion à la démo", |a| a.screen == Screen::Chat && !a.posts.is_empty());
    w
}

/// Attend qu'une condition soit vraie (la démo répond depuis un autre fil).
fn wait_for(w: &(TestApp, TestAppWindow<ChatApp>), what: &str, cond: impl Fn(&ChatApp) -> bool) {
    let start = Instant::now();
    loop {
        w.0.run_until_parked();
        if w.1.read(|a, _| cond(a)) {
            return;
        }
        assert!(start.elapsed() < Duration::from_secs(15), "délai dépassé : {what}");
        std::thread::sleep(Duration::from_millis(15));
    }
}

#[test]
fn e2e_connexion_et_affichage() {
    let mut w = demo_app();
    w.1.read(|a, _| {
        assert_eq!(a.channel_id.as_deref(), Some("general"));
        assert!(a.channels.len() > 10, "canaux de la démo chargés");
        assert!(!a.categories.is_empty());
    });
    // Dessiner toute l'interface (barre latérale, messages, membres) sans erreur.
    w.1.draw();
}

#[test]
fn e2e_envoyer_un_message_au_clavier() {
    let mut w = demo_app();
    w.1.update(|a, window, cx| {
        a.composer.update(cx, |s, cx| {
            s.set_value("Bonjour depuis le test e2e", window, cx);
            s.focus(window, cx);
        });
    });
    w.1.simulate_keystroke("enter");
    wait_for(&w, "message envoyé puis reçu", |a| {
        a.posts.last().is_some_and(|v| v.post.message == "Bonjour depuis le test e2e")
    });
    // La zone de saisie est vidée.
    w.0.run_until_parked();
    w.1.read(|a, cx| assert!(a.composer.read(cx).value().is_empty()));
}

#[test]
fn e2e_ctrl_k_change_de_canal() {
    let mut w = demo_app();
    w.1.simulate_keystroke("ctrl-k");
    wait_for(&w, "sélecteur ouvert", |a| a.switcher.is_some());
    w.1.simulate_input("design");
    w.1.simulate_keystroke("enter");
    wait_for(&w, "canal design ouvert", |a| a.channel_id.as_deref() == Some("design") && a.switcher.is_none());
    w.1.draw();
}

#[test]
fn e2e_cartes_bascule_et_echap() {
    let mut w = demo_app();
    // Carte de l'équipe : un second clic sur le nom la referme.
    w.1.update(|a, _, cx| a.open_team_card(cx));
    w.1.read(|a, _| assert!(a.team_card.is_some()));
    w.1.update(|a, _, cx| a.open_team_card(cx));
    w.1.read(|a, _| assert!(a.team_card.is_none()));
    // Fermée par le clic à côté (qui tombe sur le nom) : le clic ne la rouvre pas.
    w.1.update(|a, _, cx| {
        a.open_team_card(cx);
        a.close_team_card(cx);
        a.open_team_card(cx);
    });
    w.1.read(|a, _| assert!(a.team_card.is_none()));

    // Panneau du canal : s'ouvre à droite, un second clic le referme.
    let channel_panel = |a: &ChatApp| a.side.as_ref().is_some_and(|s| s.kind == super::SideKind::Channel);
    w.1.update(|a, window, cx| a.open_channel_card(point(px(500.), px(40.)), window, cx));
    w.1.read(|a, _| assert!(channel_panel(a)));
    w.1.draw();
    w.1.update(|a, window, cx| a.open_channel_card(point(px(500.), px(40.)), window, cx));
    w.1.read(|a, _| assert!(!channel_panel(a)));
}

#[test]
fn e2e_modifier_la_description_du_canal() {
    let mut w = demo_app();
    w.1.update(|a, window, cx| {
        a.open_channel_card(point(px(500.), px(40.)), window, cx);
        a.edit_channel_card(window, cx);
        a.channel_purpose_input.update(cx, |s, cx| s.set_value("Nouvelle description (test e2e)", window, cx));
        a.save_channel_card(cx);
    });
    wait_for(&w, "description enregistrée", |a| {
        a.channels.get("general").is_some_and(|c| c.channel.purpose == "Nouvelle description (test e2e)")
            && !a.channel_editing
    });
}

#[test]
fn e2e_recherche_ne_se_rouvre_pas_au_retour_du_focus() {
    let mut w = demo_app();
    // Le focus rendu au champ (par un menu qui se ferme, par ex.) : rien ne s'ouvre.
    w.1.update(|a, window, cx| a.search_input.update(cx, |s, cx| s.focus(window, cx)));
    w.0.run_until_parked();
    w.1.read(|a, _| assert!(!a.search_open));
    // Taper dans le champ : les propositions s'ouvrent.
    w.1.simulate_input("from:");
    w.0.run_until_parked();
    w.1.read(|a, _| assert!(a.search_open));
    w.1.draw();
}

#[test]
fn e2e_bots_regroupes_et_reduits() {
    let mut w = demo_app();
    w.1.update(|a, _, cx| a.select_channel("backend".into(), cx));
    wait_for(&w, "canal backend chargé", |a| {
        a.channel_id.as_deref() == Some("backend") && a.posts.iter().any(|v| v.post.is_bot_post())
    });
    // Dessin du canal avec des messages de bots (bloc, réduction).
    w.1.draw();
    let id = w.1.read(|a, _| a.posts.iter().find(|v| v.post.is_bot_post()).map(|v| v.post.id.clone()).unwrap());
    w.1.update(|a, _, cx| a.toggle_bot_post(id.clone(), cx));
    w.1.read(|a, _| assert!(a.expanded_bots.contains(&id)));
    w.1.draw();
}

#[test]
fn e2e_profil_au_clic_puis_echap() {
    let mut w = demo_app();
    w.1.update(|a, window, cx| a.open_profile("u2".into(), point(px(400.), px(200.)), window, cx));
    w.1.read(|a, _| assert!(a.profile.is_some()));
    w.1.draw();
    w.1.simulate_keystroke("escape");
    w.0.run_until_parked();
    w.1.read(|a, _| assert!(a.profile.is_none()));
}

#[test]
fn e2e_brouillon_garde_par_canal() {
    let mut w = demo_app();
    w.1.update(|a, window, cx| {
        a.composer.update(cx, |s, cx| s.set_value("brouillon de général", window, cx));
        a.select_channel("design".into(), cx);
    });
    w.0.run_until_parked();
    w.1.update(|a, _, cx| a.select_channel("general".into(), cx));
    w.0.run_until_parked();
    w.1.draw();
    w.1.read(|a, cx| assert_eq!(a.composer.read(cx).value().as_ref(), "brouillon de général"));
    let _ = Pane::Channel;
}

#[test]
fn e2e_fleche_haut_modifie_son_dernier_message() {
    let mut w = demo_app();
    w.1.update(|a, _, cx| a.select_channel("backend".into(), cx));
    wait_for(&w, "canal backend", |a| a.channel_id.as_deref() == Some("backend") && !a.posts.is_empty());
    let mine = w.1.read(|a, _| {
        let me = a.me_id().to_string();
        a.posts.iter().rev().find(|v| v.post.user_id == me && !v.post.is_system()).map(|v| v.post.message.clone())
    });
    let Some(mine) = mine else { return };
    w.1.update(|a, window, cx| a.composer.update(cx, |s, cx| s.focus(window, cx)));
    w.1.simulate_keystroke("up");
    w.0.run_until_parked();
    w.1.read(|a, cx| {
        assert!(a.editing.is_some(), "modification commencée");
        assert_eq!(a.composer.read(cx).value().to_string(), mine);
    });
}

#[test]
fn e2e_alt_bas_change_de_canal() {
    let mut w = demo_app();
    let before = w.1.read(|a, _| a.channel_id.clone());
    w.1.simulate_keystroke("alt-down");
    w.0.run_until_parked();
    let after = w.1.read(|a, _| a.channel_id.clone());
    assert_ne!(before, after, "Alt+↓ ouvre le canal suivant");
    w.1.simulate_keystroke("alt-up");
    w.0.run_until_parked();
    w.1.read(|a, _| assert_eq!(a.channel_id, before, "Alt+↑ revient au précédent"));
}

#[test]
fn e2e_lien_de_canal_ouvert_dans_l_application() {
    let mut w = demo_app();
    let handled = w.1.update(|a, window, cx| a.open_internal_link("mm-channel:design", window, cx));
    assert!(handled);
    wait_for(&w, "canal design", |a| a.channel_id.as_deref() == Some("design"));
    // Une adresse d'un autre site reste pour le navigateur.
    let other = w.1.update(|a, window, cx| a.open_internal_link("https://example.org/x/pl/abc", window, cx));
    assert!(!other);
}

#[test]
fn e2e_coller_une_image_la_joint() {
    let mut w = demo_app();
    let png = {
        let mut out = Vec::new();
        image::RgbaImage::new(4, 4).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    };
    w.0.write_to_clipboard(gpui_kit::ClipboardItem::new_image(&gpui_kit::Image::from_bytes(
        gpui_kit::ImageFormat::Png,
        png,
    )));
    let handled = w.1.update(|a, _, cx| a.paste_attachments(super::Pane::Channel, cx));
    assert!(handled);
    w.1.read(|a, _| assert_eq!(a.attachments.len(), 1));
    // Du texte se colle normalement.
    w.0.write_to_clipboard(gpui_kit::ClipboardItem::new_string("bonjour".into()));
    let handled = w.1.update(|a, _, cx| a.paste_attachments(super::Pane::Channel, cx));
    assert!(!handled);
}
