//! Mode démo (`mm-gpui --demo`) : un faux serveur qui répond aux mêmes
//! commandes que le vrai backend, avec des données fictives. Sert à
//! travailler l'interface et à faire des captures sans compte Mattermost.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::api::{Channel, ChannelMember, NotifyProps, Post, PostMetadata, Preference, Reaction, SidebarCategory, Team, User};
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
        ];
        let channels = vec![
            channel("general", "O", "town-square", "Général", "Annonces et discussions pour toute l'équipe", now - h),
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

        let mut demo = Self { users, channels, posts: HashMap::new(), replies: HashMap::new(), next_id: 1000 };
        let day = 24 * h;
        let m = 60_000;

        demo.add("general", "u1", now - day - 3 * h, "Bonjour à tous :wave: La démo client est **jeudi à 14h**, salle Turing.", &[("+1", &["u2", "u3", ME]), ("tada", &["u4"])], 3);
        demo.add("general", "u3", now - day - 2 * h, "Je prépare les slides, n'hésitez pas à m'envoyer vos captures d'ici mercredi.", &[], 0);
        demo.add("general", "u3", now - day - 2 * h + 2 * m, "Le modèle est dans le drive partagé > *Présentations*.", &[], 0);
        demo.add("general", ME, now - 5 * h, "Merci @chloe.durand ! J'ajoute la partie sur les performances.", &[("heart", &["u3"])], 0);
        demo.add("general", "u4", now - 2 * h, "Rappel : la mise en production de la **v2.4** est prévue ce soir.\n\n- gel des fusions à 17h\n- déploiement à 19h\n- vérifications à 19h30", &[("eyes", &["u1", "u2"])], 1);
        demo.add("general", "u2", now - h, "Pour info, la doc est à jour : [notes de version](https://example.org/notes) :white_check_mark:", &[], 0);

        demo.add("backend", "u2", now - day - 4 * h, "Quelqu'un a vu la lenteur sur `/api/tickets` ce matin ?", &[], 2);
        demo.add("backend", "u6", now - day - 4 * h + 5 * m, "Oui, un index manquait. Correctif :\n\n```sql\nCREATE INDEX idx_tickets_status\n    ON tickets (status, updated_at);\n```", &[("+1", &["u2", ME])], 0);
        demo.add("backend", ME, now - 3 * h, "Je relis la MR de @nathan.simon cet après-midi.", &[], 0);
        demo.add("backend", "u6", now - 2 * h, "Top, merci ! Les tests passent en local, la CI est encore en cours :hourglass:", &[], 0);
        for i in 0..70 {
            demo.add("backend", if i % 3 == 0 { "u2" } else { "u6" }, now - 10 * day - (70 - i) * h, &format!("Ancien message n°{} de l'historique (pour tester le défilement).", i + 1), &[], 0);
        }

        demo.add("design", "u5", now - 2 * day, "Nouvelle version de la page d'accueil :art: Vos avis ?", &[("heart", &["u1", "u3"])], 4);
        demo.add("design", "u1", now - 26 * h, "J'aime beaucoup la nouvelle typographie. Le contraste du bouton secondaire me semble un peu faible.", &[], 0);

        demo.add("support", "u4", now - 6 * h, "Escalade client : export CSV vide depuis la mise à jour. Ticket **#4821**.", &[], 0);
        demo.add("support", "u4", now - 5 * h, "@camille.martin tu peux regarder ? C'est lié à ton changement sur les filtres.", &[], 0);

        demo.add("veille", "u2", now - 3 * day, "Article intéressant sur GPUI, le framework d'interface de Zed :rocket:", &[("+1", &["u6"])], 0);
        demo.add("random", "u3", now - 30 * h, "Qui vient au repas d'équipe vendredi ? :pizza:", &[("raised_hand", &["u1", "u2", "u4", ME])], 0);

        demo.add("dm1", "u1", now - 40 * m, "Tu as deux minutes pour la démo de jeudi ?", &[], 0);
        demo.add("dm1", "u1", now - 20 * m, "Je voudrais caler l'ordre de passage :slightly_smiling_face:", &[], 0);
        demo.add("dm2", ME, now - 3 * h, "Je t'envoie le lien de la MR.", &[], 0);
        demo.add("dm2", "u2", now - 3 * h + m, "Merci !", &[], 0);
        demo.add("dm3", "u4", now - 50 * h, "Bon week-end :sunglasses:", &[], 0);
        demo.add("gm1", "u5", now - 8 * h, "On se cale un point design lundi ?", &[], 0);

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
            metadata: PostMetadata { reactions },
            ..Default::default()
        };
        let list = self.posts.entry(ch.into()).or_default();
        list.push(post);
        list.sort_by_key(|p| p.create_at);
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
                    Team { id: "t1".into(), display_name: "Équipe démo".into() },
                    Team { id: "t2".into(), display_name: "Projets clients".into() },
                ],
            });
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
            send(Event::Prefs(vec![Preference {
                category: "sidebar_settings".into(),
                name: "show_unread_section".into(),
                value: "true".into(),
            }]));
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
                                message: "Nouveau message en temps réel (démo) :sparkles:".into(),
                                ..Default::default()
                            };
                            send(Event::Posted { post, mentions_me: false, channel_type: "O".into() });
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
                    ChannelMember {
                        channel_id: c.id.clone(),
                        msg_count_root: read,
                        mention_count_root: mentions,
                        notify_props: NotifyProps { mark_unread: mark_unread.into() },
                    }
                })
                .collect();
            send(Event::Categories {
                team_id: team_id.clone(),
                categories: vec![
                    category("fav", "favorites", "Favoris", "", &["general"]),
                    category("ch", "channels", "Canaux", "alpha", &["backend", "design", "veille", "random"]),
                    category("clients", "custom", "Clients", "manual", &["support"]),
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
        Command::Send { channel_id, root_id, message } => {
            let id = demo.new_id();
            let post = Post { id, create_at: now_ms(), user_id: ME.into(), channel_id: channel_id.clone(), root_id: root_id.clone(), message, ..Default::default() };
            if root_id.is_empty() {
                demo.posts.entry(channel_id).or_default().push(post.clone());
            } else {
                demo.replies.entry(root_id.clone()).or_default().push(post.clone());
                if let Some(root) = demo.find_mut(&root_id) {
                    root.reply_count += 1;
                }
            }
            send(Event::Posted { post, mentions_me: false, channel_type: "O".into() });
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
        Command::LoadPreferences
        | Command::SetCategoryCollapsed { .. }
        | Command::Typing { .. }
        | Command::MarkRead { .. }
        | Command::FetchChannel { .. }
        | Command::Notify { .. } => {}
    }
}
