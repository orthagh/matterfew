//! Tests de bout en bout de l'interface, sans écran : la vraie `ChatApp`
//! dans une fenêtre de test GPUI, branchée sur le serveur de démonstration
//! (jamais sur un vrai compte), pilotée au clavier comme par un utilisateur.
//!
//! `cargo test e2e` pour ne lancer qu'eux.

use std::time::{Duration, Instant};

use gpui_kit::{Focusable as _, TestApp, TestAppWindow, point, px};

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

use super::view::dialogs::Dialog;

#[test]
fn e2e_parcourir_rejoindre_et_archives() {
    let mut w = demo_app();
    w.1.update(|a, window, cx| a.open_browse_dialog(window, cx));
    wait_for(
        &w,
        "canaux publics chargés",
        |a| matches!(&a.dialog, Some(Dialog::Browse(b)) if b.public.is_some() && b.archived.is_some()),
    );
    w.1.simulate_input("hackathon");
    w.1.draw();
    let channel = w.1.read(|a, _| match &a.dialog {
        Some(Dialog::Browse(b)) => b.public.as_ref().unwrap().iter().find(|c| c.id == "b2").cloned().unwrap(),
        _ => panic!("fenêtre fermée"),
    });
    w.1.update(|a, window, cx| a.browse_open(channel, window, cx));
    wait_for(&w, "canal rejoint et ouvert", |a| a.channel_id.as_deref() == Some("b2") && a.dialog.is_none());
    w.1.read(|a, _| assert!(a.categories.iter().any(|c| c.channel_ids.iter().any(|id| id == "b2"))));
    // Canal archivé : ouvert en lecture seule, sans zone de saisie.
    w.1.update(|a, window, cx| a.open_browse_dialog(window, cx));
    wait_for(&w, "archivés chargés", |a| matches!(&a.dialog, Some(Dialog::Browse(b)) if b.archived.is_some()));
    let archived = w.1.read(|a, _| match &a.dialog {
        Some(Dialog::Browse(b)) => b.archived.as_ref().unwrap()[0].clone(),
        _ => unreachable!(),
    });
    let id = archived.id.clone();
    w.1.update(|a, window, cx| a.browse_open(archived, window, cx));
    wait_for(&w, "archivé ouvert", |a| a.channel_id.as_deref() == Some(id.as_str()));
    w.1.read(|a, _| assert!(a.is_archived(&id)));
    w.1.draw();
}

#[test]
fn e2e_message_de_groupe_au_clavier() {
    let mut w = demo_app();
    w.1.simulate_keystroke("ctrl-shift-k");
    wait_for(&w, "fenêtre ouverte", |a| matches!(a.dialog, Some(Dialog::Direct(_))));
    w.1.simulate_input("chlo");
    wait_for(
        &w,
        "recherche",
        |a| matches!(&a.dialog, Some(Dialog::Direct(d)) if d.results.as_ref().is_some_and(|r| r.first().map(String::as_str) == Some("u3"))),
    );
    w.1.simulate_keystroke("enter");
    w.1.simulate_input("nathan");
    wait_for(
        &w,
        "seconde recherche",
        |a| matches!(&a.dialog, Some(Dialog::Direct(d)) if d.results.as_ref().is_some_and(|r| r.first().map(String::as_str) == Some("u6"))),
    );
    w.1.simulate_keystroke("enter");
    w.1.read(|a, _| match &a.dialog {
        Some(Dialog::Direct(d)) => assert_eq!(d.selected, ["u3", "u6"]),
        _ => panic!("fenêtre fermée"),
    });
    w.1.draw();
    // Champ vide et deux personnes choisies : Entrée ouvre la conversation.
    w.1.simulate_keystroke("enter");
    wait_for(&w, "groupe ouvert", |a| {
        a.dialog.is_none()
            && a.channel_id.as_ref().and_then(|id| a.channels.get(id)).is_some_and(|c| c.channel.is_group())
    });
}

#[test]
fn e2e_statut_personnalise() {
    let mut w = demo_app();
    w.1.read(|a, _| {
        // Statuts de la démo : affichés à côté des noms.
        assert!(a.users.get("u2").and_then(crate::api::User::custom_status).is_some());
        assert!(a.my_custom_status().is_none());
    });
    w.1.update(|a, window, cx| {
        a.open_status_dialog(window, cx);
        a.apply_preset(0, window, cx);
    });
    w.1.draw();
    w.1.update(|a, _, cx| a.save_status(cx));
    let status = w.1.read(|a, _| a.my_custom_status()).expect("statut enregistré");
    assert_eq!(status.emoji, "calendar");
    assert_eq!(status.duration, "one_hour");
    assert!(!status.expires_at.is_empty());
    w.1.draw();
    w.1.update(|a, window, cx| {
        a.open_status_dialog(window, cx);
        a.clear_status(cx);
    });
    // Le serveur confirme ensuite (dans l'ordre) : le statut reste effacé.
    wait_for(&w, "statut effacé", |a| a.my_custom_status().is_none());
}

#[test]
fn e2e_preferences_de_notification_du_canal() {
    let mut w = demo_app();
    w.1.update(|a, _, cx| a.open_notify_dialog("design".into(), cx));
    w.1.draw();
    w.1.update(|a, _, cx| {
        if let Some(Dialog::Notify(d)) = a.dialog.as_mut() {
            d.desktop = "all".into();
            d.ignore_mentions = true;
            d.muted = true;
        }
        a.save_notify(cx);
    });
    w.1.read(|a, _| {
        let state = &a.channels["design"];
        assert_eq!(state.notify.desktop, "all");
        assert_eq!(state.notify.ignore_channel_mentions, "on");
        assert!(state.muted);
        assert!(a.dialog.is_none());
    });
    // Échap ferme la fenêtre sans rien changer.
    w.1.update(|a, _, cx| a.open_notify_dialog("design".into(), cx));
    w.1.simulate_keystroke("escape");
    wait_for(&w, "fenêtre fermée", |a| a.dialog.is_none());
}

#[test]
fn e2e_vue_fils_de_l_equipe() {
    let mut w = demo_app();
    wait_for(&w, "fils chargés pour la pastille", |a| a.threads.is_some());
    w.1.read(|a, _| assert!(a.unread_threads().0 > 0, "fils non lus dans la démo"));
    w.1.update(|a, _, cx| a.open_threads_panel(true, cx));
    w.1.update(|a, _, cx| {
        a.threads_unread = true;
        cx.notify();
    });
    w.1.draw();
    w.1.update(|a, _, cx| a.mark_all_threads_read(cx));
    wait_for(&w, "tout lu", |a| a.unread_threads() == (0, 0));
    w.1.draw();
}

#[test]
fn e2e_envoi_echoue_puis_reessaye() {
    let mut w = demo_app();
    w.1.update(|a, _, cx| {
        a.outgoing.push(super::view::messaging::OutgoingPost {
            pending_id: "p-1".into(),
            pane: Pane::Channel,
            channel_id: "general".into(),
            root_id: String::new(),
            message: "Message parti sans réseau".into(),
            file_ids: Vec::new(),
            failed: Some("réseau indisponible".into()),
            created_at: 1,
        });
        a.show_pending("p-1", cx);
    });
    w.1.draw();
    w.1.update(|a, _, cx| a.retry_send("p-1", cx));
    wait_for(&w, "renvoyé puis confirmé", |a| a.outgoing.is_empty());
    wait_for(&w, "message reçu", |a| a.posts.last().is_some_and(|v| v.post.message == "Message parti sans réseau"));
    w.1.read(|a, _| {
        let copies = a.posts.iter().filter(|v| v.post.message == "Message parti sans réseau").count();
        assert_eq!(copies, 1, "le provisoire est remplacé, pas doublé");
    });
}

#[test]
fn e2e_ctrl_entree_et_confirmation_channel() {
    let mut w = demo_app();
    wait_for(&w, "membres chargés", |a| a.members.get("general").is_some_and(|m| m.1 > 5));
    // Envoi avec Ctrl+Entrée : Entrée va à la ligne.
    w.1.update(|a, _, cx| {
        a.raw_prefs.push(crate::api::Preference {
            user_id: "me".into(),
            category: "advanced_settings".into(),
            name: "send_on_ctrl_enter".into(),
            value: "true".into(),
        });
        a.refresh_preferences(cx);
    });
    w.1.update(|a, window, cx| a.composer.update(cx, |s, cx| s.focus(window, cx)));
    w.1.simulate_input("ligne 1");
    w.1.simulate_keystroke("enter");
    w.1.simulate_input("ligne 2");
    w.1.read(|a, cx| assert_eq!(a.composer.read(cx).value(), "ligne 1\nligne 2"));
    w.1.simulate_keystroke("secondary-enter");
    wait_for(&w, "envoyé", |a| a.posts.last().is_some_and(|v| v.post.message == "ligne 1\nligne 2"));
    // @channel dans un canal de plus de 5 personnes : confirmation.
    w.1.update(|a, window, cx| a.composer.update(cx, |s, cx| s.set_value("@channel réunion à 14 h", window, cx)));
    w.1.simulate_keystroke("secondary-enter");
    wait_for(&w, "confirmation", |a| matches!(a.dialog, Some(Dialog::ConfirmNotify { .. })));
    w.1.draw();
    w.1.update(|a, window, cx| a.confirm_notify_send(Pane::Channel, window, cx));
    wait_for(&w, "envoyé après confirmation", |a| {
        a.dialog.is_none() && a.posts.last().is_some_and(|v| v.post.message == "@channel réunion à 14 h")
    });
}

#[test]
fn e2e_transferer_reactions_et_emojis_recents() {
    let mut w = demo_app();
    let (post_id, reacted) = w.1.read(|a, _| {
        let v = a.posts.iter().find(|v| !v.post.metadata.reactions.is_empty()).expect("réactions dans la démo");
        (v.post.id.clone(), v.post.metadata.reactions[0].emoji_name.clone())
    });
    w.1.read(|a, _| {
        let tip = a.who_reacted(&a.posts.iter().find(|v| v.post.id == post_id).unwrap().post, &reacted);
        assert!(tip.contains(&format!(":{reacted}:")), "{tip}");
    });
    // Emoji utilisé : en tête des récents et des réactions rapides.
    w.1.update(|a, _, _| a.toggle_reaction(post_id.clone(), "tada".into()));
    w.1.read(|a, _| {
        assert_eq!(a.recent_emojis.first().map(String::as_str), Some("tada"));
        assert_eq!(a.quick_reactions()[0], "tada");
    });
    // Transfert vers « design ».
    w.1.update(|a, window, cx| a.open_forward(post_id.clone(), window, cx));
    w.1.simulate_input("design");
    w.1.simulate_keystroke("enter");
    w.1.read(|a, _| assert!(matches!(&a.dialog, Some(Dialog::Forward(f)) if f.target.as_deref() == Some("design"))));
    w.1.draw();
    w.1.update(|a, _, cx| a.send_forward(cx));
    wait_for(&w, "transféré", |a| a.dialog.is_none() && a.outgoing.is_empty() && a.info.is_some());
}

#[test]
fn e2e_parametres_du_canal_et_archivage() {
    let mut w = demo_app();
    w.1.update(|a, window, cx| {
        a.open_channel_settings("design".into(), window, cx);
        a.dialog_input.update(cx, |s, cx| s.set_value("Design produit", window, cx));
        a.dialog_comment.update(cx, |s, cx| s.set_value("Design Produit", window, cx));
    });
    w.1.draw();
    w.1.update(|a, _, cx| a.save_channel_settings(cx));
    wait_for(&w, "canal renommé", |a| {
        let c = &a.channels["design"].channel;
        c.display_name == "Design produit" && c.name == "design-produit"
    });
    w.1.update(|a, window, cx| {
        a.open_channel_settings("design".into(), window, cx);
        if let Some(Dialog::ChannelSettings { confirm, .. }) = a.dialog.as_mut() {
            *confirm = Some(super::view::manage::ChannelDanger::Archive);
        }
    });
    w.1.draw();
    w.1.update(|a, _, cx| a.run_channel_danger(cx));
    wait_for(&w, "canal archivé", |a| a.is_archived("design"));
}

#[test]
fn e2e_categories_raccourcis_et_tout_lu() {
    let mut w = demo_app();
    w.1.update(|a, window, cx| {
        a.open_category_dialog("clients".into(), false, window, cx);
        a.dialog_input.update(cx, |s, cx| s.set_value("Clients VIP", window, cx));
        a.rename_category(cx);
    });
    w.1.read(|a, _| assert!(a.categories.iter().any(|c| c.id == "clients" && c.display_name == "Clients VIP")));
    w.1.update(|a, window, cx| {
        a.open_category_dialog("clients".into(), true, window, cx);
        a.delete_category(cx);
    });
    w.1.read(|a, _| {
        assert!(!a.categories.iter().any(|c| c.id == "clients"));
        let channels = a.categories.iter().find(|c| c.kind == "channels").unwrap();
        assert!(channels.channel_ids.iter().any(|id| id == "support"));
    });
    // F1 : aide des raccourcis ; Échap la ferme.
    w.1.simulate_keystroke("f1");
    wait_for(&w, "aide ouverte", |a| matches!(a.dialog, Some(Dialog::Shortcuts)));
    w.1.draw();
    w.1.simulate_keystroke("escape");
    wait_for(&w, "aide fermée", |a| a.dialog.is_none());
    // Maj+Échap : tout marquer comme lu.
    w.1.read(|a, _| assert!(a.channels.values().any(|c| c.shows_unread())));
    w.1.simulate_keystroke("shift-escape");
    wait_for(&w, "tout lu", |a| !a.channels.values().any(|c| c.shows_unread()));
}

#[test]
fn e2e_retirer_un_membre() {
    let mut w = demo_app();
    wait_for(&w, "membres", |a| a.members.contains_key("general"));
    let before = w.1.read(|a, _| a.members["general"].0.len());
    w.1.update(|a, _, cx| a.remove_member("general".into(), "u6".into(), cx));
    w.1.read(|a, _| {
        assert_eq!(a.members["general"].0.len(), before - 1);
        assert!(!a.members["general"].0.iter().any(|id| id == "u6"));
    });
}

#[test]
fn e2e_historique_des_canaux() {
    let mut w = demo_app();
    w.1.update(|a, _, cx| a.select_channel("design".into(), cx));
    w.1.update(|a, _, cx| a.select_channel("backend".into(), cx));
    let (back, forward) = if cfg!(target_os = "macos") { ("cmd-[", "cmd-]") } else { ("alt-left", "alt-right") };
    w.1.simulate_keystroke(back);
    wait_for(&w, "retour à design", |a| a.channel_id.as_deref() == Some("design"));
    w.1.simulate_keystroke(back);
    wait_for(&w, "retour à général", |a| a.channel_id.as_deref() == Some("general"));
    w.1.simulate_keystroke(forward);
    wait_for(&w, "avance à design", |a| a.channel_id.as_deref() == Some("design"));
}

#[test]
fn e2e_parametres_notifications_et_barre_laterale() {
    use super::view::settings::SettingsTab;
    let mut w = demo_app();
    w.1.update(|a, window, cx| a.open_settings(window, cx));
    for tab in [SettingsTab::Notifications, SettingsTab::Display, SettingsTab::Sidebar, SettingsTab::Advanced] {
        w.1.update(|a, _, _| {
            if let Some(Dialog::Settings(d)) = a.dialog.as_mut() {
                d.tab = tab;
            }
        });
        w.1.draw();
    }
    w.1.update(|a, window, cx| {
        let Some(Dialog::Settings(d)) = a.dialog.as_mut() else { panic!("paramètres ouverts") };
        d.desktop = "all".into();
        d.sound = "Ripple";
        d.dm_limit = 10;
        d.highlight_keys.update(cx, |s, cx| s.set_value("zorglub, quux", window, cx));
        a.save_settings(cx);
    });
    wait_for(&w, "préférences de notification enregistrées", |a| {
        let props = &a.me.as_ref().unwrap().notify_props;
        props.get("highlight_keys").map(String::as_str) == Some("zorglub,quux")
            && props.get("desktop").map(String::as_str) == Some("all")
            && props.get("desktop_notification_sound").map(String::as_str) == Some("Ripple")
    });
    w.1.read(|a, _| {
        assert!(a.dialog.is_none());
        // Les mots-clés surlignés sont globaux : d'autres tests en parallèle les changent.
        assert_eq!(a.prefs.dm_limit, 10);
    });
}

#[test]
fn e2e_ne_pas_deranger_avec_duree() {
    let mut w = demo_app();
    // Une heure : statut et échéance.
    w.1.update(|a, _, cx| {
        let end = super::view::presence::dnd_end("one_hour", chrono::Local::now());
        a.set_status_until("dnd", end, cx);
    });
    wait_for(&w, "ne pas déranger", |a| a.statuses.get(a.me_id()).map(String::as_str) == Some("dnd") && a.dnd_end > 0);
    w.1.read(|a, _| assert!(a.my_status_line().contains('·'), "{}", a.my_status_line()));
    // Date choisie : demain 8 h.
    w.1.update(|a, window, cx| {
        a.open_dnd_dialog(window, cx);
        if let Some(Dialog::DndCustom { day }) = a.dialog.as_mut() {
            *day = 1;
        }
        a.dialog_input.update(cx, |s, cx| s.set_value("8h", window, cx));
    });
    w.1.draw();
    w.1.update(|a, _, cx| a.save_dnd_custom(cx));
    wait_for(&w, "échéance demain", |a| {
        a.dialog.is_none()
            && chrono::TimeZone::timestamp_opt(&chrono::Local, a.dnd_end, 0)
                .single()
                .is_some_and(|t| chrono::Timelike::hour(&t) == 8)
    });
    // Retour en ligne : plus d'échéance.
    w.1.update(|a, _, cx| a.set_status_until("online", 0, cx));
    wait_for(&w, "en ligne", |a| a.statuses.get(a.me_id()).map(String::as_str) == Some("online") && a.dnd_end == 0);
}

#[test]
fn e2e_non_lus_seulement_et_conversation_fermee() {
    let mut w = demo_app();
    let all = w.1.read(|a, _| a.sidebar_sections().iter().map(|s| s.channel_ids.len()).sum::<usize>());
    w.1.simulate_keystroke("ctrl-shift-u");
    w.1.read(|a, _| {
        assert!(a.unread_filter);
        let ids: Vec<String> = a.sidebar_sections().into_iter().flat_map(|s| s.channel_ids).collect();
        assert!(ids.len() < all, "{} sur {all}", ids.len());
        assert!(ids.iter().all(|id| a.channels[id].shows_unread() || a.channel_id.as_deref() == Some(id.as_str())));
    });
    w.1.draw();
    w.1.simulate_keystroke("ctrl-shift-u");
    // Fermer une conversation lue : elle quitte la barre latérale, puis revient à l'ouverture.
    let dm = w.1.read(|a, _| {
        a.channels.values().find(|s| s.channel.is_direct() && !s.shows_unread()).map(|s| s.channel.id.clone()).unwrap()
    });
    w.1.update(|a, _, cx| a.close_dm(dm.clone(), cx));
    let listed = |a: &ChatApp, id: &str| a.sidebar_sections().iter().any(|s| s.channel_ids.iter().any(|c| c == id));
    w.1.read(|a, _| assert!(!listed(a, &dm)));
    w.1.update(|a, _, cx| a.select_channel(dm.clone(), cx));
    w.1.read(|a, _| assert!(listed(a, &dm)));
}

#[test]
fn e2e_rejoindre_et_quitter_une_equipe() {
    let mut w = demo_app();
    w.1.update(|a, _, cx| a.open_join_team(cx));
    wait_for(
        &w,
        "équipes ouvertes",
        |a| matches!(&a.dialog, Some(Dialog::JoinTeam { teams: Some(t), .. }) if t.len() == 2),
    );
    w.1.draw();
    w.1.update(|a, _, cx| a.join_team("t3".into(), cx));
    wait_for(&w, "équipe rejointe", |a| {
        a.dialog.is_none() && a.team_id.as_deref() == Some("t3") && a.teams.len() == 3
    });
    w.1.update(|a, _, cx| a.confirm_leave_team("t3".into(), cx));
    w.1.draw();
    w.1.update(|a, _, cx| a.leave_team("t3".into(), cx));
    w.1.read(|a, _| {
        assert_eq!(a.teams.len(), 2);
        assert_ne!(a.team_id.as_deref(), Some("t3"));
    });
    // Paramètres de l'équipe : nom et ouverture.
    w.1.update(|a, window, cx| {
        a.open_team_settings("t1".into(), window, cx);
        a.dialog_input.update(cx, |s, cx| s.set_value("Équipe produit", window, cx));
        if let Some(Dialog::TeamSettings { open, .. }) = a.dialog.as_mut() {
            *open = true;
        }
    });
    w.1.draw();
    w.1.update(|a, _, cx| a.save_team_settings(cx));
    wait_for(&w, "équipe renommée", |a| {
        a.teams.iter().any(|t| t.id == "t1" && t.display_name == "Équipe produit" && t.team_type == "O")
    });
}

#[test]
fn e2e_limites_envoi_aide_et_panneau_large() {
    let mut w = demo_app();
    // Message trop long : bloqué avant l'envoi.
    let before = w.1.read(|a, _| a.posts.len());
    w.1.update(|a, window, cx| {
        a.server_config.insert("MaxPostSize".into(), "20".into());
        a.composer.update(cx, |s, cx| s.set_value("un message bien trop long pour la limite", window, cx));
    });
    w.1.draw();
    w.1.update(|a, window, cx| a.send_from(Pane::Channel, window, cx));
    w.1.read(|a, cx| {
        assert_eq!(a.posts.len(), before);
        assert!(!a.composer.read(cx).value().is_empty(), "le texte reste dans la zone de saisie");
    });
    // Fichier : progression, puis fin de l'envoi.
    let dir = std::env::temp_dir().join(format!("matterfew-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("rapport.txt");
    std::fs::write(&file, vec![b'x'; 4000]).unwrap();
    w.1.update(|a, _, cx| a.add_files(Pane::Channel, vec![file.clone()], cx));
    wait_for(&w, "progression", |a| a.attachments.iter().any(|x| x.sent > 0));
    w.1.draw();
    wait_for(&w, "envoyé", |a| a.attachments.iter().all(|x| x.file_id.is_some()));
    // Trop gros pour le serveur : refusé.
    w.1.update(|a, _, cx| {
        a.server_config.insert("MaxFileSize".into(), "1000".into());
        a.add_files(Pane::Channel, vec![file.clone()], cx);
    });
    w.1.read(|a, _| assert_eq!(a.attachments.len(), 1));
    let _ = std::fs::remove_dir_all(&dir);
    // Aide Markdown et panneau agrandi.
    w.1.update(|a, _, cx| a.open_markdown_help(cx));
    w.1.draw();
    w.1.update(|a, _, cx| a.close_dialog(cx));
    let narrow = w.1.read(|a, _| a.right_panel_width());
    w.1.simulate_keystroke("ctrl-shift-.");
    w.1.read(|a, _| assert!(a.panel_expanded && a.right_panel_width() >= narrow));
}

#[test]
fn e2e_editeur_riche() {
    let mut w = demo_app();
    w.1.update(|a, window, cx| {
        a.settings.rich_editor = true;
        a.composer.update(cx, |s, cx| s.set_value("un **gras** et `code`", window, cx));
        a.check_spelling(Pane::Channel, cx);
    });
    w.1.read(|a, cx| assert!(a.composer.read(cx).decorations().len() >= 4));
    w.1.draw();
    w.1.update(|a, _, cx| {
        a.settings.rich_editor = false;
        a.check_spelling(Pane::Channel, cx);
    });
    // Sans correcteur (cas des tests) : plus aucune décoration.
    w.1.read(|a, cx| assert!(a.spell.is_some() || a.composer.read(cx).decorations().is_empty()));
}

#[test]
fn e2e_mise_a_jour() {
    use super::view::update::UpdateState;
    let mut w = demo_app();
    w.1.update(|a, _, cx| a.check_for_update(true, cx));
    w.1.read(|a, _| assert!(matches!(&a.update, Some(UpdateState::Available(f)) if f.installable)));
    w.1.draw();
    w.1.update(|a, _, cx| a.install_update(cx));
    wait_for(
        &w,
        "téléchargement",
        |a| matches!(a.update, Some(UpdateState::Downloading { received, .. }) if received > 0),
    );
    w.1.draw();
    wait_for(&w, "installation (démo)", |a| matches!(a.update, Some(UpdateState::Installing { .. })));
    w.1.draw();
}

#[test]
fn e2e_signets_apercu_pdf_et_f6() {
    let mut w = demo_app();
    wait_for(&w, "signets chargés", |a| a.bookmarks.get("general").is_some_and(|b| b.len() == 3));
    w.1.draw();
    // Ajout d'un lien (adresse sans schéma : https ajouté).
    w.1.update(|a, window, cx| {
        a.open_bookmark_dialog("general".into(), window, cx);
        a.dialog_input.update(cx, |s, cx| s.set_value("example.org/agenda", window, cx));
        a.save_bookmark(cx);
    });
    wait_for(&w, "signet ajouté", |a| {
        a.bookmarks.get("general").is_some_and(|b| b.iter().any(|b| b.link_url == "https://example.org/agenda"))
    });
    // Aperçu d'un PDF joint.
    let pdf = w.1.read(|a, _| {
        a.posts.iter().flat_map(|v| v.post.metadata.files.iter()).find(|f| f.extension == "pdf").cloned().unwrap()
    });
    // Ailleurs que sous Windows, le PDF s'ouvrirait avec l'application du système.
    if crate::pdf::SUPPORTED {
        w.1.update(|a, window, cx| a.open_file(pdf, window, cx));
        wait_for(&w, "pages rendues", |a| {
            a.viewer.as_ref().and_then(|v| v.pdf.as_ref()).is_some_and(|(pages, total)| pages.len() == 2 && *total == 2)
        });
        w.1.draw();
        w.1.update(|a, _, cx| a.close_viewer(cx));
    }
    // F6 : de la zone de saisie à la recherche, puis retour.
    w.1.update(|a, window, cx| a.composer.update(cx, |s, cx| s.focus(window, cx)));
    w.1.simulate_keystroke("f6");
    w.1.update(|a, window, cx| assert!(a.search_input.read(cx).focus_handle(cx).is_focused(window)));
    w.1.simulate_keystroke("f6");
    w.1.update(|a, window, cx| assert!(a.composer.read(cx).focus_handle(cx).is_focused(window)));
}

#[test]
fn e2e_lu_ailleurs() {
    // Canal lu depuis le client web : il n'est plus non lu ici.
    let mut w = demo_app();
    let id = w.1.read(|a, _| {
        a.channels.values().find(|c| c.shows_unread()).map(|c| c.channel.id.clone()).expect("un canal non lu")
    });
    w.1.update(|a, window, cx| {
        a.on_event(crate::backend::Event::ChannelsViewed(vec![(id.clone(), 1)]), window, cx);
    });
    w.1.read(|a, _| assert!(!a.channels[&id].shows_unread()));
    // Puis marqué non lu là-bas : il le redevient, avec ses mentions.
    w.1.update(|a, window, cx| {
        let unread = crate::backend::Event::ChannelUnread {
            channel_id: id.clone(),
            last_viewed_at: 1,
            mention_count: 2,
            mention_count_root: 2,
        };
        a.on_event(unread, window, cx);
    });
    w.1.read(|a, _| {
        assert!(a.channels[&id].shows_unread());
        assert_eq!(a.channels[&id].mentions, 2);
    });
}

/// Linux: text selected with the mouse becomes the primary selection, and a
/// middle click pastes the one of any application.
#[cfg(target_os = "linux")]
#[test]
fn e2e_primary_selection_and_middle_click() {
    use gpui_kit::{ClipboardItem, MouseButton};

    let mut w = demo_app();
    w.1.update(|a, window, cx| {
        a.composer.update(cx, |s, cx| {
            s.set_value("bonjour", window, cx);
            s.focus(window, cx);
        });
    });
    w.1.simulate_keystroke("ctrl-a");
    // The test window only knows what is under the pointer once it has moved
    // and a frame was drawn.
    let (messages, composer) = w.1.update(|_, window, _| {
        let size = window.viewport_size();
        (point(size.width / 2., size.height / 2.), point(size.width / 2., size.height - px(60.)))
    });
    let primary =
        |w: &(TestApp, TestAppWindow<ChatApp>)| w.0.read(|cx| cx.read_from_primary().and_then(|item| item.text()));

    // Releasing the button publishes what is selected.
    w.1.simulate_mouse_move(messages);
    w.1.draw();
    w.1.simulate_mouse_up(messages, MouseButton::Left);
    w.0.run_until_parked();
    assert_eq!(primary(&w).as_deref(), Some("bonjour"));

    // Another application selects text; a middle click in the composer pastes it.
    w.0.update(|cx| cx.write_to_primary(ClipboardItem::new_string(" monde".into())));
    w.1.simulate_keystroke("end");
    w.1.simulate_mouse_move(composer);
    w.1.draw();
    w.1.simulate_mouse_down(composer, MouseButton::Middle);
    w.0.run_until_parked();
    w.1.read(|a, cx| assert_eq!(a.composer.read(cx).value().as_ref(), "bonjour monde"));

    // A click that selects nothing new leaves the other application's selection alone.
    w.1.simulate_mouse_up(composer, MouseButton::Left);
    w.0.run_until_parked();
    assert_eq!(primary(&w).as_deref(), Some(" monde"));
}

#[test]
fn e2e_erreur_passagere_effacee() {
    // Erreur au lancement (serveur qui redémarre) : effacée quand la
    // connexion revient, sans avoir à fermer le bandeau.
    use crate::backend::Event;
    let mut w = demo_app();
    w.1.update(|a, window, cx| {
        a.on_event(Event::Error { message: "HTTP 502".into(), expired: false }, window, cx);
    });
    w.1.read(|a, _| assert!(a.error.is_some()));
    w.1.update(|a, window, cx| a.on_event(Event::Connection(true), window, cx));
    w.1.read(|a, _| assert!(a.error.is_none()));
}

#[test]
fn e2e_clic_sur_l_heure() {
    // Depuis le fil, l'heure d'un message le montre dans le canal.
    let mut w = demo_app();
    let first = w.1.read(|a, _| a.posts[0].post.clone());
    w.1.update(|a, window, cx| a.on_time_click(super::Pane::Thread, first.clone(), window, cx));
    w.1.read(|a, _| {
        assert_eq!(a.revealed.as_deref(), Some(first.id.as_str()));
        assert!(a.pending_reveal.is_none());
    });
    w.1.draw();
    // Une réponse affichée dans le canal ouvre son fil.
    let reply = w.1.read(|a, _| a.posts.iter().find(|v| v.post.is_reply()).map(|v| v.post.clone()));
    if let Some(reply) = reply {
        w.1.update(|a, window, cx| a.on_time_click(super::Pane::Channel, reply.clone(), window, cx));
        w.1.read(|a, _| {
            assert_eq!(a.thread.as_ref().map(|t| t.root_id.as_str()), Some(reply.root_id.as_str()));
            assert_eq!(a.revealed.as_deref(), Some(reply.id.as_str()));
        });
    }
}

#[test]
fn e2e_molette_en_bas() {
    // Tout en bas, la molette vers le bas ne fait pas apparaître « Aller au
    // présent » ; vers le haut, la liste défile toujours.
    use gpui_kit::{ScrollDelta, ScrollEvent};
    let mut w = demo_app();
    w.1.draw();
    let at = point(px(600.), px(300.));
    let wheel = |y: f32| ScrollEvent { position: at, delta: ScrollDelta::Lines(point(0., y)), ..Default::default() };
    let scrolled_up = |w: &(TestApp, TestAppWindow<ChatApp>)| w.1.read(|a, cx| a.scroller.read(cx).is_scrolled_up());
    for _ in 0..3 {
        w.1.simulate_event(wheel(-3.));
        w.1.draw();
        assert!(!scrolled_up(&w));
    }
    w.1.simulate_event(wheel(3.));
    w.1.draw();
    assert!(scrolled_up(&w), "la molette vers le haut fait toujours défiler");
}

#[test]
fn e2e_menu_d_un_lien() {
    // Clic droit sur un lien web : menu pour le copier ; rien pour les autres adresses.
    let mut w = demo_app();
    w.1.update(|a, window, cx| a.open_link_menu("file:///etc/passwd".into(), point(px(100.), px(100.)), window, cx));
    w.1.read(|a, _| assert!(a.link_menu.is_none()));
    w.1.update(|a, window, cx| {
        a.open_link_menu("https://example.org/page".into(), point(px(100.), px(100.)), window, cx)
    });
    w.1.read(|a, _| assert!(a.link_menu.is_some()));
    w.1.draw();
    w.1.simulate_click(point(px(160.), px(145.)), gpui_kit::MouseButton::Left);
    wait_for(&w, "menu fermé", |a| a.link_menu.is_none());
    let copied = w.0.update(|cx| cx.read_from_clipboard().and_then(|c| c.text()));
    assert_eq!(copied.as_deref(), Some("https://example.org/page"));
}

#[test]
fn e2e_glisser_deposer_un_fichier() {
    // Un fichier glissé au-dessus du canal affiche la zone de dépôt ; déposé,
    // il est joint au message en cours.
    use gpui_kit::{ExternalPaths, FileDropEvent};
    let mut w = demo_app();
    w.1.draw();
    let file = std::env::temp_dir().join("matterfew-depot-e2e.txt");
    std::fs::write(&file, "bonjour").unwrap();
    let at = point(px(600.), px(300.));
    w.1.simulate_event(FileDropEvent::Entered { position: at, paths: ExternalPaths(vec![file.clone()].into()) });
    w.1.simulate_event(FileDropEvent::Pending { position: at });
    w.1.draw();
    w.1.read(|a, _| assert_eq!(a.file_drag, Some(super::Pane::Channel)));
    w.1.simulate_event(FileDropEvent::Submit { position: at });
    w.1.draw();
    w.1.read(|a, _| {
        assert_eq!(a.file_drag, None);
        assert_eq!(a.attachments.len(), 1);
        assert_eq!(a.attachments[0].name, "matterfew-depot-e2e.txt");
    });
    // Glissé puis ressorti de la fenêtre : la zone disparaît, rien n'est joint.
    w.1.simulate_event(FileDropEvent::Entered { position: at, paths: ExternalPaths(vec![file.clone()].into()) });
    w.1.simulate_event(FileDropEvent::Pending { position: at });
    w.1.draw();
    w.1.simulate_event(FileDropEvent::Exited);
    w.1.draw();
    w.1.read(|a, _| {
        assert_eq!(a.file_drag, None);
        assert_eq!(a.attachments.len(), 1);
    });
    let _ = std::fs::remove_file(file);
    // Fichier hors de portée (chemin de l'hôte vu d'un bac à sable) : signalé.
    let ghost = std::env::temp_dir().join("matterfew-introuvable").join("rapport.pdf");
    w.1.simulate_event(FileDropEvent::Entered { position: at, paths: ExternalPaths(vec![ghost].into()) });
    w.1.simulate_event(FileDropEvent::Submit { position: at });
    w.1.draw();
    w.1.read(|a, _| {
        assert_eq!(a.attachments.len(), 1);
        assert!(a.info.as_deref().is_some_and(|i| i.contains("rapport.pdf")), "{:?}", a.info);
    });
}

#[test]
fn e2e_maj_et_alt_entree_vont_a_la_ligne() {
    // Comme le client web : Maj+Entrée et Alt+Entrée vont à la ligne sans
    // envoyer, Entrée envoie, Ctrl+Entrée ferme un bloc de code ouvert.
    let mut w = demo_app();
    let sent_before = w.1.read(|a, _| a.posts.len());
    w.1.update(|a, window, cx| {
        a.composer.update(cx, |s, cx| s.focus(window, cx));
    });
    w.1.simulate_input("ligne 1");
    w.1.simulate_keystroke("shift-enter");
    w.1.simulate_input("ligne 2");
    w.1.simulate_keystroke("alt-enter");
    w.1.simulate_input("ligne 3");
    w.0.run_until_parked();
    w.1.read(|a, cx| {
        assert_eq!(a.posts.len(), sent_before, "rien n'est envoyé");
        assert_eq!(a.composer.read(cx).value().as_ref(), "ligne 1\nligne 2\nligne 3");
    });
    w.1.simulate_keystroke("enter");
    wait_for(&w, "message de trois lignes envoyé", |a| {
        a.posts.last().is_some_and(|v| v.post.message == "ligne 1\nligne 2\nligne 3")
    });

    w.1.update(|a, window, cx| a.composer.update(cx, |s, cx| s.set_value("```\nlet x = 1;", window, cx)));
    w.1.simulate_keystroke("secondary-enter");
    wait_for(&w, "bloc de code fermé puis envoyé", |a| {
        a.posts.last().is_some_and(|v| v.post.message == "```\nlet x = 1;\n```")
    });
}

#[test]
fn e2e_envoi_optimiste() {
    // Le message s'affiche dès l'envoi, puis le vrai le remplace sans doublon.
    let mut w = demo_app();
    w.1.update(|a, window, cx| {
        let channel = a.channel_id.clone().unwrap();
        a.post_message(super::Pane::Channel, channel, String::new(), "Envoi optimiste".into(), Vec::new(), cx);
        let last = &a.posts.last().unwrap().post;
        assert_eq!(last.message, "Envoi optimiste");
        assert!(a.outgoing.iter().any(|o| o.pending_id == last.id), "affiché en attente");
        let _ = window;
    });
    wait_for(&w, "remplacé par le vrai message", |a| {
        a.outgoing.is_empty() && a.posts.last().is_some_and(|v| v.post.message == "Envoi optimiste")
    });
    w.1.read(|a, _| {
        let copies = a.posts.iter().filter(|v| v.post.message == "Envoi optimiste").count();
        assert_eq!(copies, 1, "pas de doublon");
        assert!(!a.posts.last().unwrap().post.id.contains(':'), "identifiant du serveur");
    });
}

#[test]
fn e2e_echec_d_envoi_sur_le_message() {
    // Échec : le message reste dans la liste avec la raison ; « Abandonner » le retire.
    let mut w = demo_app();
    w.1.update(|a, _, cx| {
        a.outgoing.push(super::view::messaging::OutgoingPost {
            pending_id: "moi:1".into(),
            pane: super::Pane::Channel,
            channel_id: a.channel_id.clone().unwrap(),
            root_id: String::new(),
            message: "Pas parti".into(),
            file_ids: Vec::new(),
            failed: Some("réseau".into()),
            created_at: 1,
        });
        a.show_pending("moi:1", cx);
    });
    w.1.draw();
    w.1.read(|a, _| assert_eq!(a.posts.last().unwrap().post.id, "moi:1"));
    w.1.update(|a, _, cx| a.discard_failed("moi:1", cx));
    w.1.read(|a, _| {
        assert!(a.outgoing.is_empty());
        assert!(!a.posts.iter().any(|v| v.post.id == "moi:1"));
    });
}

#[test]
fn e2e_coller_un_lien_sur_une_selection() {
    // Adresse collée sur un texte sélectionné : lien Markdown.
    let mut w = demo_app();
    w.1.update(|a, window, cx| {
        a.composer.update(cx, |s, cx| {
            s.set_value("voir la doc ici", window, cx);
            s.focus(window, cx);
            s.set_selected_range(8..11, cx);
        });
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("https://example.org/guide".into()));
    });
    w.1.simulate_keystroke("secondary-v");
    w.1.read(|a, cx| {
        assert_eq!(a.composer.read(cx).value().as_ref(), "voir la [doc](https://example.org/guide) ici");
    });
    // Sans sélection, l'adresse se colle telle quelle.
    w.1.update(|a, window, cx| {
        a.composer.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
    });
    w.1.simulate_keystroke("secondary-v");
    w.1.read(|a, cx| assert_eq!(a.composer.read(cx).value().as_ref(), "https://example.org/guide"));
}
