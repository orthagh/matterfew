//! Mode démo (`mm-gpui --demo`) : un faux serveur qui répond aux mêmes
//! commandes que le vrai backend, avec des données fictives. Sert à
//! travailler l'interface et à faire des captures sans compte Mattermost.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use std::sync::Arc;

use crate::api::{
    Channel, ChannelMember, FileInfo, NotifyProps, Post, PostMetadata, Preference, Reaction, SidebarCategory, Team,
    TeamUnread, User,
};
use crate::backend::{Backend, Command, Event};

const ME: &str = "me";

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn user(id: &str, first: &str, last: &str) -> User {
    User {
        id: id.into(),
        username: format!("{}.{}", first.to_lowercase(), last.to_lowercase()),
        first_name: first.into(),
        last_name: last.into(),
        nickname: String::new(),
        last_picture_update: 0,
        notify_props: HashMap::from([("desktop".to_string(), "mention".to_string())]),
        email: format!("{}.{}@exemple.org", first.to_lowercase(), last.to_lowercase()),
        position: match first {
            "Lucas" => "Responsable support".into(),
            "Inès" => "Designer".into(),
            "Nathan" => "Développeur backend".into(),
            _ => String::new(),
        },
        is_bot: false,
        roles: if first == "Lucas" { "system_user system_admin".into() } else { "system_user".into() },
        timezone: serde_json::json!({
            "useAutomaticTimezone": "true",
            "automaticTimezone": if first == "Hugo" { "America/Montreal" } else { "Europe/Paris" },
            "manualTimezone": ""
        }),
    }
}

fn channel(id: &str, kind: &str, name: &str, display: &str, purpose: &str, recency: i64) -> Channel {
    Channel {
        id: id.into(),
        kind: kind.into(),
        name: name.into(),
        display_name: display.into(),
        purpose: purpose.into(),
        header: String::new(),
        total_msg_count: 10,
        total_msg_count_root: 10,
        last_post_at: recency,
        last_root_post_at: recency,
        create_at: 0,
        delete_at: 0,
    }
}

fn category(id: &str, kind: &str, name: &str, sorting: &str, ids: &[&str]) -> SidebarCategory {
    SidebarCategory {
        id: id.into(),
        kind: kind.into(),
        display_name: name.into(),
        sorting: sorting.into(),
        collapsed: false,
        muted: false,
        channel_ids: ids.iter().map(|s| s.to_string()).collect(),
        extra: Default::default(),
    }
}

struct Demo {
    users: Vec<User>,
    channels: Vec<Channel>,
    posts: HashMap<String, Vec<Post>>,
    replies: HashMap<String, Vec<Post>>,
    /// Fichiers envoyés pendant la démo, en attente d'être joints à un message.
    uploads: HashMap<String, FileInfo>,
    /// Messages enregistrés pendant la démo.
    saved: Vec<String>,
    next_id: u64,
}

impl Demo {
    fn new() -> Self {
        let now = now_ms();
        let h = 3_600_000;
        let users = vec![
            user(ME, "Camille", "Martin"),
            user("u1", "Léa", "Bernard"),
            user("u2", "Hugo", "Petit"),
            user("u3", "Chloé", "Durand"),
            user("u4", "Lucas", "Moreau"),
            user("u5", "Inès", "Laurent"),
            user("u6", "Nathan", "Simon"),
            {
                // Compte du plugin GitHub (bot).
                let mut bot = user("ugh", "GitHub", "Bot");
                bot.username = "github".into();
                bot.first_name = "GitHub".into();
                bot.last_name = String::new();
                bot.is_bot = true;
                bot
            },
        ];
        let channels = vec![
            {
                let mut c = channel("general", "O", "town-square", "Général", "Annonces et discussions pour toute l'équipe", now - h);
                c.header = "**Réunion** lundi 10 h · [Wiki de l'équipe](https://example.org/wiki) · [Planning](https://example.org/planning)".into();
                c
            },
            channel("backend", "O", "dev-backend", "dev-backend", "API, base de données et déploiements", now - 2 * h),
            channel("design", "O", "design", "design", "Maquettes et retours d'interface", now - 26 * h),
            channel("support", "P", "support-n2", "support-n2", "Escalades du support", now - 5 * h),
            channel("veille", "O", "veille-techno", "veille-techno", "Articles et outils à partager", now - 72 * h),
            channel("random", "O", "off-topic", "Hors sujet", "", now - 30 * h),
            channel("dm1", "D", "me__u1", "", "", now - 20 * 60_000),
            channel("dm2", "D", "me__u2", "", "", now - 3 * h),
            channel("dm3", "D", "me__u4", "", "", now - 50 * h),
            channel("gm1", "G", "gm", "Chloé Durand, Inès Laurent", "", now - 8 * h),
        ];

        // Beaucoup de canaux, comme sur un vrai serveur : la barre latérale
        // doit défiler sans écraser ses lignes.
        let mut channels = channels;
        for (i, name) in [
            "marketplace", "partners-day", "itil-process", "glpi-ai", "securite", "tech-marketing", "equipe-caen",
            "proservices", "cloud", "services-dev", "rd-edition", "matos-studio", "sportifs-du-jeudi", "green-it",
            "articles", "formation", "agent-inventory", "mobile", "documentation", "qualite", "support-n1",
            "commercial", "rh", "evenements", "infra", "recrutement", "traduction", "communaute",
        ]
        .iter()
        .enumerate()
        {
            let kind = if i % 3 == 0 { "O" } else { "P" };
            channels.push(channel(&format!("x{i}"), kind, name, name, "", now - (40 + i as i64) * h));
        }
        let mut demo = Self { users, channels, posts: HashMap::new(), replies: HashMap::new(), uploads: HashMap::new(), saved: Vec::new(), next_id: 1000 };
        let day = 24 * h;
        let m = 60_000;

        demo.add("general", "u1", now - day - 3 * h, "Bonjour à tous :wave: La démo client est **jeudi à 14h**, salle Turing.", &[("+1", &["u2", "u3", ME]), ("tada", &["u4"])], 3);
        demo.add("general", "u3", now - day - 2 * h, "Je prépare les slides, n'hésitez pas à m'envoyer vos captures d'ici mercredi.", &[], 0);
        demo.add("general", "u3", now - day - 2 * h + 2 * m, "Le modèle est dans le drive partagé > *Présentations*.", &[], 0);
        demo.add("general", ME, now - 5 * h, "Merci @chloe.durand ! J'ajoute la partie sur les performances.", &[("heart", &["u3"])], 0);
        demo.add("general", "u4", now - 2 * h, "Rappel : la mise en production de la **v2.4** est prévue ce soir.\n\n- gel des fusions à 17h\n- déploiement à 19h\n- vérifications à 19h30", &[("eyes", &["u1", "u2"])], 1);
        // Réponse éphémère d'une commande « /github subscriptions list ».
        demo.add(
            "general",
            "ugh",
            now - 2 * m,
            "##### Subscriptions in this channel
* glpi-project/doc - pulls,issues,creates,deletes --render-style collapsed
* glpi-project/glpi - pulls,issues,creates,deletes --render-style collapsed
* glpi-project/roadmap - pulls,issues --render-style collapsed",
            &[],
            0,
        );
        if let Some(post) = demo.posts.get_mut("general").and_then(|l| l.iter_mut().find(|p| p.create_at == now - 2 * m)) {
            post.kind = "system_ephemeral".into();
        }
        demo.add("general", "u1", now - h - 5 * m, "Ordre du jour de lundi :
1. point sur la version 12
2. recette du client GPUI
Merci de compléter avant vendredi.", &[], 0);
        demo.add("general", "u2", now - h, "Pour info, la doc est à jour : [notes de version](https://example.org/notes) :white_check_mark: #release-12 #documentation", &[], 0);

        demo.add("backend", "u2", now - day - 4 * h, "Quelqu'un a vu la lenteur sur `/api/tickets` ce matin ?", &[], 2);
        demo.add("backend", "u6", now - day - 4 * h + 5 * m, "Oui, un index manquait. Correctif :\n\n```sql\nCREATE INDEX idx_tickets_status\n    ON tickets (status, updated_at);\n```", &[("+1", &["u2", ME])], 0);
        demo.add("backend", ME, now - 3 * h, "Je relis la MR de @nathan.simon cet après-midi.", &[], 0);
        demo.add("backend", "u6", now - 2 * h, "Top, merci ! Les tests passent en local, la CI est encore en cours :hourglass:", &[], 0);
        for i in 0..70 {
            demo.add("backend", if i % 3 == 0 { "u2" } else { "u6" }, now - 10 * day - (70 - i) * h, &format!("Ancien message n°{} de l'historique (pour tester le défilement).", i + 1), &[], 0);
        }

        demo.add("design", "u5", now - 2 * day, "Nouvelle version de la page d'accueil :art: Vos avis ?", &[("heart", &["u1", "u3"])], 4);
        demo.add("design", "u1", now - 26 * h, "J'aime beaucoup la nouvelle typographie. Le contraste du bouton secondaire me semble un peu faible.", &[], 0);

        demo.add("support", "u4", now - 6 * h, "Escalade client : export CSV vide depuis la mise à jour. Ticket **#4821**, support !4821.", &[], 0);
        demo.add("support", "u4", now - 5 * h, "@camille.martin tu peux regarder ? C'est lié à ton changement sur les filtres.", &[], 0);

        demo.add("veille", "u2", now - 3 * day, "Article intéressant sur GPUI, le framework d'interface de Zed :rocket:", &[("+1", &["u6"])], 0);
        demo.add("random", "u3", now - 30 * h, "Qui vient au repas d'équipe vendredi ? :pizza:", &[("raised_hand", &["u1", "u2", "u4", ME])], 0);
        demo.add("random", "u5", now - 29 * h, "Moi ! Et vive :glpi: :tada:", &[("glpi", &["u1", "u3"])], 0);
        if let Some(post) = demo.posts.get_mut("random").and_then(|l| l.iter_mut().find(|p| p.create_at == now - 29 * h)) {
            post.metadata.emojis = vec![crate::api::CustomEmoji { id: "e-glpi".into(), name: "glpi".into() }];
        }

        demo.add("dm1", "u1", now - 40 * m, "Tu as deux minutes pour la démo de jeudi ?", &[], 0);
        demo.add("dm1", "u1", now - 20 * m, "Je voudrais caler l'ordre de passage :slightly_smiling_face:", &[], 0);
        demo.add("dm2", ME, now - 3 * h, "Je t'envoie le lien de la MR.", &[], 0);
        demo.add("dm2", "u2", now - 3 * h + m, "Merci !", &[], 0);
        demo.add("dm3", "u4", now - 50 * h, "Bon week-end :sunglasses:", &[], 0);
        demo.add("gm1", "u5", now - 8 * h, "On se cale un point design lundi ?", &[], 0);

        // Messages avec pièces jointes.
        demo.add_with_files(
            "design",
            "u5",
            now - 20 * h,
            "Les captures de la revue d'interface :camera:",
            vec![
                file("f-d1", "accueil-large.png", "image/png", 820_000, 1600, 900),
                file("f-d2", "mobile-portrait.png", "image/png", 410_000, 750, 1334),
                file("f-d3", "icone.png", "image/png", 90_000, 1024, 1024),
                file("f-d4", "tableau-de-bord.png", "image/png", 990_000, 1440, 960),
                file("f-d5", "bandeau.jpg", "image/jpeg", 300_000, 2400, 800),
                file("f-d6", "carte.png", "image/png", 520_000, 1200, 1500),
                file("f-d7", "pastille.png", "image/png", 12_000, 240, 160),
            ],
        );
        demo.add_with_files(
            "general",
            "u5",
            now - 30 * m,
            "Voici la maquette finale de la page d'accueil :point_down:",
            vec![file("f-img1", "accueil-v2.png", "image/png", 1_240_000, 1600, 900)],
        );
        demo.add_with_files(
            "general",
            "u6",
            now - 10 * m,
            "Le rapport de performances et les mesures brutes :",
            vec![
                file("f-pdf1", "rapport-performances.pdf", "application/pdf", 245_000, 0, 0),
                file("f-csv1", "mesures.csv", "text/csv", 18_300, 0, 0),
            ],
        );

        // Intégration (GitLab) et aperçu de lien.
        demo.add("backend", "u6", now - 90 * m, "", &[], 0);
        if let Some(post) = demo.posts.get_mut("backend").and_then(|l| l.iter_mut().find(|p| p.create_at == now - 90 * m)) {
            post.props = serde_json::json!({
                "from_webhook": "true",
                "override_username": "GitLab",
                "attachments": [{
                    "color": "#fc6d26",
                    "pretext": "Nathan Simon a fusionné une demande de fusion",
                    "author_name": "glpi / glpi",
                    "title": "!1842 Correction de l'export CSV des tickets",
                    "title_link": "https://example.org/glpi/glpi/-/merge_requests/1842",
                    "text": "Corrige les filtres perdus lors de l'export. Ajoute des tests sur `Search::export`.",
                    "fields": [
                        {"title": "Branche", "value": "fix/csv-export", "short": true},
                        {"title": "Pipeline", "value": "✅ réussi", "short": true}
                    ],
                    "footer": "GitLab",
                    "actions": [{"id": "deploy", "name": "Déployer en recette", "type": "button", "style": "primary"}]
                }]
            });
        }
        // Bot GitHub : long message, réduit à l'affichage.
        demo.add("backend", "u6", now - 80 * m, "", &[], 0);
        if let Some(post) = demo.posts.get_mut("backend").and_then(|l| l.iter_mut().find(|p| p.create_at == now - 80 * m)) {
            post.props = serde_json::json!({
                "from_bot": "true",
                "override_username": "github",
                "attachments": [{
                    "color": "#24292f",
                    "pretext": "[glpi-project/glpi] Nouvelle pull request ouverte par nathan-simon",
                    "title": "#18342 Refonte du moteur de recherche des tickets",
                    "title_link": "https://example.org/glpi-project/glpi/pull/18342",
                    "text": "## Résumé
- remplace l'ancien moteur de requêtes
- ajoute un cache des critères
- corrige le tri sur les colonnes calculées

## Tests
- 214 tests unitaires ajoutés
- scénarios de bout en bout sur l'export CSV

## Notes
La migration des recherches enregistrées est automatique.",
                    "fields": [
                        {"title": "Relecteurs", "value": "lea-bernard, hugo-petit", "short": true},
                        {"title": "Étiquettes", "value": "search, performance", "short": true}
                    ],
                    "footer": "GitHub"
                }]
            });
        }
        demo.add("backend", "u6", now - 79 * m, "", &[], 0);
        if let Some(post) = demo.posts.get_mut("backend").and_then(|l| l.iter_mut().find(|p| p.create_at == now - 79 * m)) {
            post.props = serde_json::json!({
                "from_bot": "true",
                "override_username": "github",
                "attachments": [{
                    "color": "#2da44e",
                    "pretext": "[glpi-project/glpi] Vérifications réussies sur #18342",
                    "text": "Toutes les vérifications sont passées (42 tâches).",
                    "footer": "GitHub Actions"
                }]
            });
        }
        demo.add("backend", "u6", now - 78 * m, "", &[], 0);
        if let Some(post) = demo.posts.get_mut("backend").and_then(|l| l.iter_mut().find(|p| p.create_at == now - 78 * m)) {
            post.props = serde_json::json!({
                "from_bot": "true",
                "override_username": "github",
                "attachments": [{
                    "color": "#8250df",
                    "pretext": "[glpi-project/glpi] lea-bernard a approuvé #18342",
                    "text": "Relu, rien à redire. Merci pour les tests de bout en bout sur l'export.
On peut fusionner après la passe de traduction.
Pense à la note de version.",
                    "footer": "GitHub"
                }]
            });
        }
        // Plugin GitHub : messages Markdown postés par son bot, sans pièce jointe.
        for (k, text) in [
            (77, "#### Corriger l'export des tickets fusionnés\n##### [glpi-project/glpi#18350](https://github.com/glpi-project/glpi/pull/18350) nouvelle pull request de [nathan-simon](https://github.com/nathan-simon)\n\nCorrige la fusion des colonnes calculées dans l'export CSV."),
            (76, "[glpi-project/glpi] Nouveau commentaire de [lea-bernard](https://github.com/lea-bernard) sur [#18350 Corriger l'export des tickets fusionnés](https://github.com/glpi-project/glpi/pull/18350) :\n> Ajoute un test sur les colonnes vides, sinon parfait."),
        ] {
            demo.add("backend", "ugh", now - k * m, text, &[], 0);
            if let Some(post) = demo.posts.get_mut("backend").and_then(|l| l.iter_mut().find(|p| p.create_at == now - k * m)) {
                post.props = serde_json::json!({ "from_plugin": "true" });
                post.kind = "custom_git_pr".into();
            }
        }
        demo.add("veille", "u2", now - 2 * day, "À lire : https://example.org/gpui-zed", &[], 0);
        if let Some(post) = demo.posts.get_mut("veille").and_then(|l| l.iter_mut().find(|p| p.create_at == now - 2 * day)) {
            post.metadata.embeds = serde_json::json!([{
                "type": "opengraph",
                "url": "https://example.org/gpui-zed",
                "data": {
                    "site_name": "Le blog tech",
                    "title": "GPUI : le moteur d'interface de Zed",
                    "description": "Comment Zed dessine son interface directement sur la carte graphique, et pourquoi Discord l'a choisi pour sa réécriture.",
                    "images": [{"secure_url": "demo://og-gpui"}]
                }
            }]);
        }

        // Réponses de quelques fils.
        let root = demo.posts["general"][0].id.clone();
        for (who, text) in [("u2", "Je serai là."), ("u4", "Moi aussi, je présente la partie API."), (ME, "Parfait, merci à vous deux !")] {
            demo.reply(&root, "general", who, now - day - 3 * h + 10 * m, text);
        }
        let root = demo.posts["design"][0].id.clone();
        for (who, text) in [("u1", "Très réussi !"), ("u3", "Le header est plus lisible."), ("u6", "On peut tester en sombre ?"), ("u5", "Oui, je la publie demain.")] {
            demo.reply(&root, "design", who, now - 2 * day + 20 * m, text);
        }
        demo
    }

    fn new_id(&mut self) -> String {
        self.next_id += 1;
        format!("p{}", self.next_id)
    }

    fn add(&mut self, ch: &str, who: &str, at: i64, text: &str, reactions: &[(&str, &[&str])], reply_count: i64) {
        let id = self.new_id();
        let reactions = reactions
            .iter()
            .flat_map(|(emoji, users)| {
                users.iter().map(|u| Reaction { user_id: u.to_string(), emoji_name: emoji.to_string() })
            })
            .collect();
        let post = Post {
            id,
            create_at: at,
            user_id: who.into(),
            channel_id: ch.into(),
            message: text.into(),
            reply_count,
            metadata: PostMetadata { reactions, ..Default::default() },
            ..Default::default()
        };
        let list = self.posts.entry(ch.into()).or_default();
        list.push(post);
        list.sort_by_key(|p| p.create_at);
    }

    fn add_with_files(&mut self, ch: &str, who: &str, at: i64, text: &str, files: Vec<FileInfo>) {
        self.add(ch, who, at, text, &[], 0);
        if let Some(post) = self.posts.get_mut(ch).and_then(|l| l.iter_mut().find(|p| p.create_at == at)) {
            post.metadata.files = files;
        }
    }

    fn reply(&mut self, root: &str, ch: &str, who: &str, at: i64, text: &str) {
        let id = self.new_id();
        let post = Post { id, create_at: at, user_id: who.into(), channel_id: ch.into(), root_id: root.into(), message: text.into(), ..Default::default() };
        self.replies.entry(root.into()).or_default().push(post);
    }

    fn find_mut(&mut self, post_id: &str) -> Option<&mut Post> {
        self.posts.values_mut().chain(self.replies.values_mut()).flatten().find(|p| p.id == post_id)
    }
}

pub fn start() -> Backend {
    let (cmd_tx, cmd_rx) = async_channel::unbounded::<Command>();
    let (ev_tx, ev_rx) = async_channel::unbounded::<Event>();

    std::thread::Builder::new()
        .name("mm-demo".into())
        .spawn(move || {
            let mut demo = Demo::new();
            let send = |e: Event| {
                let _ = ev_tx.send_blocking(e);
            };
            send(Event::Ready {
                me: demo.users[0].clone(),
                teams: vec![
                    Team {
                        id: "t1".into(),
                        name: "demo".into(),
                        display_name: "Équipe démo".into(),
                        last_team_icon_update: 1,
                        description: "Espace de démonstration : support, design et projets internes.".into(),
                        team_type: "I".into(),
                        create_at: now_ms() - 400 * 24 * 3_600_000,
                    },
                    Team {
                        id: "t2".into(),
                        name: "clients".into(),
                        display_name: "Projets clients".into(),
                        last_team_icon_update: 0,
                        team_type: "O".into(),
                        ..Default::default()
                    },
                ],
                config: HashMap::from([("CollapsedThreads".to_string(), "default_on".to_string())]),
                prefs: demo_preferences(),
            });
            send(Event::TeamIcon { team_id: "t1".into(), bytes: Arc::new(bmp(64, 64, 0x2f6fd8, 0x30c4a0)) });
            send(Event::TeamUnreads(vec![TeamUnread {
                team_id: "t2".into(),
                msg_count_root: 3,
                mention_count_root: 1,
                msg_count: 3,
                mention_count: 1,
            }]));
            send(Event::Users(demo.users.clone()));
            send(Event::Statuses(vec![
                (ME.into(), "online".into()),
                ("u1".into(), "online".into()),
                ("u2".into(), "away".into()),
                ("u3".into(), "dnd".into()),
                ("u4".into(), "offline".into()),
                ("u5".into(), "online".into()),
                ("u6".into(), "online".into()),
            ]));
            send(Event::Connection(true));

            let mut tick = 0u32;
            loop {
                match cmd_rx.recv_blocking_timeout(Duration::from_secs(4)) {
                    Ok(Some(cmd)) => handle(cmd, &mut demo, &send),
                    Ok(None) => {
                        // Un peu de vie : quelqu'un écrit, puis publie.
                        tick += 1;
                        if tick % 3 == 1 {
                            send(Event::Typing { channel_id: "general".into(), parent_id: String::new(), user_id: "u1".into() });
                        }
                        if tick == 8 {
                            let id = demo.new_id();
                            let post = Post {
                                id,
                                create_at: now_ms(),
                                user_id: "u5".into(),
                                channel_id: "random".into(),
                                message: "@camille.martin nouveau message en temps réel (démo) :sparkles:".into(),
                                ..Default::default()
                            };
                            demo.posts.entry("random".into()).or_default().push(post.clone());
                            send(Event::Posted { post, mentions_me: true, followed: false, channel_type: "O".into(), team_id: "t1".into() });
                        }
                    }
                    Err(()) => return,
                }
            }
        })
        .expect("thread démo");

    Backend { commands: cmd_tx, events: ev_rx }
}

trait RecvTimeout<T> {
    /// `Ok(Some)` : commande reçue ; `Ok(None)` : délai écoulé ; `Err` : fermé.
    fn recv_blocking_timeout(&self, timeout: Duration) -> Result<Option<T>, ()>;
}

impl<T> RecvTimeout<T> for async_channel::Receiver<T> {
    fn recv_blocking_timeout(&self, timeout: Duration) -> Result<Option<T>, ()> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            match self.try_recv() {
                Ok(v) => return Ok(Some(v)),
                Err(async_channel::TryRecvError::Closed) => return Err(()),
                Err(async_channel::TryRecvError::Empty) => {
                    if std::time::Instant::now() >= deadline {
                        return Ok(None);
                    }
                    std::thread::sleep(Duration::from_millis(15));
                }
            }
        }
    }
}

fn handle(cmd: Command, demo: &mut Demo, send: &impl Fn(Event)) {
    match cmd {
        Command::LoadTeam { team_id } | Command::LoadCategories { team_id } => {
            let members = demo
                .channels
                .iter()
                .map(|c| {
                    let (read, mentions) = match c.id.as_str() {
                        "support" => (8, 1),
                        "dm1" => (8, 2),
                        "random" | "design" => (8, 0),
                        _ => (10, 0),
                    };
                    let mark_unread = if c.id == "veille" { "mention" } else { "all" };
                    // Lu jusqu'à… : les messages plus récents sont « nouveaux ».
                    let last_viewed_at = match c.id.as_str() {
                        "dm1" => now_ms() - 3_600_000,
                        "support" => now_ms() - 5 * 3_600_000 - 1_800_000,
                        "design" => now_ms() - 30 * 3_600_000,
                        _ => now_ms(),
                    };
                    ChannelMember {
                        channel_id: c.id.clone(),
                        last_viewed_at,
                        msg_count: read,
                        msg_count_root: read,
                        mention_count: mentions,
                        mention_count_root: mentions,
                        notify_props: NotifyProps { mark_unread: mark_unread.into(), desktop: "default".into() },
                    }
                })
                .collect();
            let mut channel_ids = vec!["design", "random"];
            let extra: Vec<String> = demo.channels.iter().filter(|c| c.id.starts_with('x')).map(|c| c.id.clone()).collect();
            channel_ids.extend(extra.iter().map(String::as_str));
            send(Event::Categories {
                team_id: team_id.clone(),
                categories: vec![
                    category("fav", "favorites", "Favoris", "", &["general"]),
                    category("ch", "channels", "Canaux", "alpha", &channel_ids),
                    category("clients", "custom", "Clients", "manual", &["support", "veille", "backend"]),
                    category("dm", "direct_messages", "Messages directs", "recent", &["dm1", "dm2", "dm3", "gm1"]),
                ],
            });
            send(Event::Channels { team_id, channels: demo.channels.clone(), members });
        }
        Command::LoadChannel { channel_id } => {
            let posts = demo.posts.get(&channel_id).cloned().unwrap_or_default();
            let start = posts.len().saturating_sub(crate::backend::PAGE_SIZE);
            send(Event::ChannelPosts { channel_id, posts: posts[start..].to_vec() });
        }
        Command::LoadOlder { channel_id, before } => {
            let posts = demo.posts.get(&channel_id).cloned().unwrap_or_default();
            let end = posts.iter().position(|p| p.id == before).unwrap_or(0);
            let start = end.saturating_sub(crate::backend::PAGE_SIZE);
            send(Event::OlderPosts { channel_id, posts: posts[start..end].to_vec() });
        }
        Command::LoadThread { root_id } => {
            let root = demo.posts.values().flatten().find(|p| p.id == root_id).cloned();
            let mut posts: Vec<Post> = root.into_iter().collect();
            posts.extend(demo.replies.get(&root_id).cloned().unwrap_or_default());
            send(Event::Thread { root_id, posts });
        }
        Command::Send { channel_id, root_id, message, file_ids } => {
            let id = demo.new_id();
            let files = file_ids.iter().filter_map(|id| demo.uploads.remove(id)).collect();
            let post = Post {
                id,
                create_at: now_ms(),
                user_id: ME.into(),
                channel_id: channel_id.clone(),
                root_id: root_id.clone(),
                message,
                metadata: PostMetadata { files, ..Default::default() },
                ..Default::default()
            };
            if root_id.is_empty() {
                demo.posts.entry(channel_id).or_default().push(post.clone());
            } else {
                demo.replies.entry(root_id.clone()).or_default().push(post.clone());
                if let Some(root) = demo.find_mut(&root_id) {
                    root.reply_count += 1;
                }
            }
            send(Event::Posted { post, mentions_me: false, followed: false, channel_type: "O".into(), team_id: "t1".into() });
        }
        Command::Edit { post_id, message } => {
            if let Some(post) = demo.find_mut(&post_id) {
                post.message = message;
                post.edit_at = now_ms();
                send(Event::PostEdited(post.clone()));
            }
        }
        Command::Delete { post_id } => {
            if let Some(post) = demo.find_mut(&post_id) {
                post.delete_at = now_ms();
                let post = post.clone();
                for list in demo.posts.values_mut().chain(demo.replies.values_mut()) {
                    list.retain(|p| p.id != post_id);
                }
                send(Event::PostDeleted(post));
            }
        }
        Command::React { post_id, emoji, add } => {
            let reaction = Reaction { user_id: ME.into(), emoji_name: emoji };
            if let Some(post) = demo.find_mut(&post_id) {
                if add {
                    post.metadata.reactions.push(reaction.clone());
                    send(Event::ReactionAdded(reaction, post_id));
                } else {
                    post.metadata.reactions.retain(|r| !(r.user_id == ME && r.emoji_name == reaction.emoji_name));
                    send(Event::ReactionRemoved(reaction, post_id));
                }
            }
        }
        Command::LoadTeamUnreads => {}
        Command::LoadPreview { file_id } => {
            let seed = file_id.bytes().fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
            // Aperçu aux proportions du fichier, comme celui du serveur.
            let (w, h) = demo
                .posts
                .values()
                .flatten()
                .flat_map(|p| p.metadata.files.iter())
                .find(|f| f.id == file_id && f.width > 0 && f.height > 0)
                .map(|f| (480u32, (480 * f.height / f.width).clamp(60, 960) as u32))
                .unwrap_or((480, 270));
            let bytes = bmp(w, h, 0x1c58d9 ^ (seed & 0x3f3f3f), 0xffbc1f ^ (seed & 0x1f1f00));
            send(Event::FilePreview { file_id, bytes: Arc::new(bytes) });
        }
        Command::LoadFullImage { file_id } => {
            let bytes = bmp(1600, 900, 0x1c58d9, 0xffbc1f);
            send(Event::FullImage { file_id, result: Ok(Arc::new(bytes)) });
        }
        Command::Download { file, open } => {
            let dir = std::env::temp_dir().join("mm-gpui-demo");
            let _ = std::fs::create_dir_all(&dir);
            let path = dir.join(&file.name);
            let _ = std::fs::write(&path, format!("Fichier de démonstration : {}\n", file.name));
            send(Event::Downloaded { path, open });
        }
        Command::Upload { local_id, path, .. } => {
            std::thread::sleep(Duration::from_millis(600));
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let size = std::fs::metadata(&path).map(|m| m.len() as i64).unwrap_or(0);
            let extension = name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
            let mime = match extension.as_str() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                _ => "application/octet-stream",
            };
            let id = demo.new_id();
            let info = FileInfo { id: id.clone(), name, extension, size, mime_type: mime.into(), width: 800, height: 450, has_preview_image: mime.starts_with("image/"), ..Default::default() };
            demo.uploads.insert(id, info.clone());
            send(Event::Uploaded { local_id, file: info });
        }
        Command::SyncChannel { channel_id, .. } => send(Event::ChannelSince { channel_id, posts: Vec::new() }),
        Command::LoadExternalImage { url } => {
            if url.starts_with("demo://") {
                send(Event::ExternalImage { url, bytes: Arc::new(bmp(200, 200, 0x7b2ff7, 0x1c58d9)) });
            }
        }
        Command::PostAction { .. }
        | Command::Slash { .. }
        | Command::MarkUnread { .. }
        | Command::UpdateCategories { .. }
        | Command::SetMuted { .. }
        | Command::LeaveChannel { .. }
        | Command::MarkChannelsRead { .. } => {}
        Command::SetStatus { status } => send(Event::Statuses(vec![(ME.into(), status)])),
        Command::LoadChannelFiles { channel_id, .. } => {
            // Démo : fichiers des messages connus du canal.
            let files = demo
                .posts
                .get(&channel_id)
                .into_iter()
                .flatten()
                .flat_map(|p| {
                    p.metadata.files.iter().cloned().map(move |mut f| {
                        f.post_id = p.id.clone();
                        f.channel_id = p.channel_id.clone();
                        f.user_id = p.user_id.clone();
                        f.create_at = p.create_at;
                        f
                    })
                })
                .collect();
            send(Event::ChannelFiles { channel_id, files });
        }
        Command::LoadUserByName { username } => {
            if let Some(u) = demo.users.iter().find(|u| u.username == username) {
                send(Event::Users(vec![u.clone()]));
            }
        }
        Command::LoadMembers { channel_id } => {
            // Démo : tout le monde dans les canaux publics, deux personnes dans les autres.
            let ids: Vec<String> = match channel_id.as_str() {
                "general" | "backend" | "design" | "random" | "veille" => demo.users.iter().map(|u| u.id.clone()).collect(),
                _ => vec![ME.to_string(), "u1".to_string(), "u4".to_string()],
            };
            let total = ids.len() as i64;
            send(Event::Members { channel_id, user_ids: ids, admins: vec!["u4".into()], total });
        }
        Command::LoadThreads { .. } => {
            let mut threads: Vec<crate::api::UserThread> = demo
                .replies
                .iter()
                .filter_map(|(root, replies)| {
                    let post = demo.posts.values().flatten().find(|p| &p.id == root)?.clone();
                    let mut participants: Vec<User> = Vec::new();
                    for r in replies {
                        if let Some(u) = demo.users.iter().find(|u| u.id == r.user_id)
                            && !participants.iter().any(|p| p.id == u.id) {
                                participants.push(u.clone());
                            }
                    }
                    Some(crate::api::UserThread {
                        id: root.clone(),
                        reply_count: replies.len() as i64,
                        last_reply_at: replies.iter().map(|r| r.create_at).max().unwrap_or(post.create_at),
                        unread_replies: if post.channel_id == "design" { 2 } else { 0 },
                        unread_mentions: 0,
                        participants,
                        post,
                    })
                })
                .collect();
            threads.sort_by_key(|x| std::cmp::Reverse(x.last_reply_at));
            send(Event::Threads(threads));
        }
        Command::MarkThreadRead { .. } => {}
        Command::LoadTeamInfo { team_id } => send(Event::TeamInfo { team_id, total: 7, active: 5, admin: true }),
        Command::SaveTeamDescription { team_id, description } => {
            send(Event::TeamUpdated(Team {
                id: team_id,
                name: "demo".into(),
                display_name: "Équipe démo".into(),
                last_team_icon_update: 1,
                description,
                team_type: "I".into(),
                ..Default::default()
            }))
        }
        Command::CreateChannel { name, display_name, purpose, private, .. } => {
            let mut c = channel(&format!("new-{name}"), if private { "P" } else { "O" }, &name, &display_name, &purpose, now_ms());
            c.create_at = now_ms();
            demo.channels.push(c.clone());
            send(Event::ChannelCreated(c));
        }
        Command::CreateCategory { team_id, name } => {
            send(Event::Notice(format!("Catégorie « {name} » créée (démo, équipe {team_id}).")));
        }
        Command::InviteByEmail { emails, .. } => {
            send(Event::Notice(format!("{} invitation(s) envoyée(s) (démo).", emails.len())));
        }
        Command::PatchChannel { channel_id, purpose, header } => {
            if let Some(c) = demo.channels.iter_mut().find(|c| c.id == channel_id) {
                c.purpose = purpose;
                c.header = header;
                send(Event::ChannelUpdated(c.clone()));
            }
        }
        Command::SearchUsersForChannel { channel_id, term, .. } => {
            // Démo : les canaux privés n'ont que trois membres.
            let inside = ["me", "u1", "u4"];
            let needle = crate::fuzzy::fold(&term);
            let users = demo
                .users
                .iter()
                .filter(|u| !inside.contains(&u.id.as_str()))
                .filter(|u| crate::fuzzy::fold(&format!("{} {} {}", u.username, u.first_name, u.last_name)).contains(&needle))
                .cloned()
                .collect();
            send(Event::UserCandidates { channel_id, term, users });
        }
        Command::AddChannelMember { .. } => {}
        Command::OpenDirect { user_id } => {
            let existing = demo.channels.iter().find(|c| c.is_direct() && c.name.split("__").any(|id| id == user_id));
            let channel = match existing {
                Some(c) => c.clone(),
                None => {
                    let c = channel(&format!("dm-{user_id}"), "D", &format!("{ME}__{user_id}"), "", "", now_ms());
                    demo.channels.push(c.clone());
                    c
                }
            };
            send(Event::DirectOpened(channel));
        }
        Command::SavePost { post_id, save } => {
            demo.saved.retain(|id| *id != post_id);
            if save {
                demo.saved.push(post_id);
            }
        }
        Command::LoadSide { kind, channel_id, terms, .. } => {
            use crate::backend::SideKind;
            let terms = terms.to_lowercase();
            let mut posts: Vec<Post> = demo
                .posts
                .values()
                .chain(demo.replies.values())
                .flatten()
                .filter(|p| match kind {
                    SideKind::Pinned => p.channel_id == channel_id && p.is_pinned,
                    SideKind::Saved => demo.saved.contains(&p.id),
                    SideKind::Threads | SideKind::Files | SideKind::Channel => false,
                    SideKind::Mentions => p.message.contains("@camille.martin"),
                    SideKind::Search => {
                        !terms.is_empty()
                            && terms.split_whitespace().all(|t| p.message.to_lowercase().contains(t))
                    }
                })
                .cloned()
                .collect();
            posts.sort_by_key(|x| std::cmp::Reverse(x.create_at));
            send(Event::SidePosts { kind, posts });
        }
        Command::PinPost { post_id, pin } => {
            if let Some(post) = demo.find_mut(&post_id) {
                post.is_pinned = pin;
                send(Event::PostEdited(post.clone()));
            }
        }
        Command::LoadEmoji { name, .. } => {
            send(Event::CustomEmoji { name, bytes: Arc::new(bmp(32, 32, 0xfec400, 0xe5007d)) });
        }
        Command::Autocomplete { seq, kind, term, .. } => {
            use crate::backend::{AcKind, Suggestion};
            let term = term.to_lowercase();
            let items: Vec<Suggestion> = match kind {
                AcKind::User => demo
                    .users
                    .iter()
                    .filter(|u| u.username.starts_with(&term) || u.first_name.to_lowercase().starts_with(&term))
                    .map(|u| Suggestion {
                        label: format!("@{}", u.username),
                        detail: u.display_name(),
                        insert: format!("@{} ", u.username),
                        user_id: Some(u.id.clone()),
                        glyph: None,
                    })
                    .collect(),
                AcKind::Channel => demo
                    .channels
                    .iter()
                    .filter(|c| (c.kind == "O" || c.kind == "P") && c.name.starts_with(&term))
                    .take(10)
                    .map(|c| Suggestion {
                        label: format!("~{}", c.name),
                        detail: c.display_name.clone(),
                        insert: format!("~{} ", c.name),
                        user_id: None,
                        glyph: None,
                    })
                    .collect(),
                AcKind::Emoji => ["glpi", "teclib"]
                    .iter()
                    .filter(|n| n.starts_with(&term))
                    .map(|n| Suggestion {
                        label: format!(":{n}:"),
                        detail: "emoji personnalisé".into(),
                        insert: format!(":{n}: "),
                        user_id: None,
                        glyph: Some(n.to_string()),
                    })
                    .collect(),
                AcKind::Command => [
                    ("/away", "Passer en « Absent »"),
                    ("/dnd", "Ne pas déranger"),
                    ("/online", "Passer en ligne"),
                    ("/me", "Message à la troisième personne"),
                    ("/shrug", "¯\\_(ツ)_/¯"),
                ]
                .iter()
                .filter(|(c, _)| c.starts_with(&term))
                .map(|(c, d)| Suggestion {
                    label: c.to_string(),
                    detail: d.to_string(),
                    insert: format!("{c} "),
                    user_id: None,
                    glyph: None,
                })
                .collect(),
            };
            for s in &items {
                if let (AcKind::Emoji, Some(name)) = (kind, &s.glyph) {
                    send(Event::CustomEmoji { name: name.clone(), bytes: Arc::new(bmp(32, 32, 0xfec400, 0xe5007d)) });
                }
            }
            send(Event::Suggestions { seq, kind, items });
        }
        Command::LoadPreferences
        | Command::SetCollapsedThreads(_)
        | Command::SetCategoryCollapsed { .. }
        | Command::Typing { .. }
        | Command::MarkRead { .. }
        | Command::FetchChannel { .. }
        => {}
        Command::Notify { title, body, target } => crate::notify::show(&title, &body, target),
    }
}

fn file(id: &str, name: &str, mime: &str, size: i64, width: i64, height: i64) -> FileInfo {
    FileInfo {
        id: id.into(),
        name: name.into(),
        extension: name.rsplit_once('.').map(|(_, e)| e.to_string()).unwrap_or_default(),
        size,
        mime_type: mime.into(),
        width,
        height,
        has_preview_image: mime.starts_with("image/"),
        ..Default::default()
    }
}

/// Petite image BMP en dégradé (aperçus et logos de la démo, sans
/// dépendre d'un encodeur d'images).
fn bmp(width: u32, height: u32, from: u32, to: u32) -> Vec<u8> {
    let row = (width * 3).div_ceil(4) * 4;
    let size = 54 + row * height;
    let mut out = Vec::with_capacity(size as usize);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(width as i32).to_le_bytes());
    out.extend_from_slice(&(height as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    let channel = |c: u32, shift: u32| ((c >> shift) & 0xff) as f32;
    for y in 0..height {
        for x in 0..width {
            let t = (x + y) as f32 / (width + height) as f32;
            let mix = |shift| (channel(from, shift) * (1.0 - t) + channel(to, shift) * t) as u8;
            out.extend_from_slice(&[mix(0), mix(8), mix(16)]);
        }
        out.extend(std::iter::repeat_n(0u8, (row - width * 3) as usize));
    }
    out
}

/// Préférences de la démo. `--theme fichier.json` applique un thème
/// Mattermost (le JSON copié depuis Paramètres › Affichage › Thème ›
/// « Copier le thème » du client web) pour vérifier le rendu avec ses couleurs.
fn demo_preferences() -> Vec<Preference> {
    let mut prefs = vec![Preference {
        user_id: String::new(),
        category: "sidebar_settings".into(),
        name: "show_unread_section".into(),
        value: "true".into(),
    }];
    let args: Vec<String> = std::env::args().collect();
    if let Some(path) = args.iter().position(|a| a == "--theme").and_then(|i| args.get(i + 1)) {
        match std::fs::read_to_string(path) {
            Ok(value) => prefs.push(Preference { user_id: String::new(), category: "theme".into(), name: String::new(), value }),
            Err(err) => eprintln!("thème illisible ({path}) : {err}"),
        }
    }
    prefs
}
