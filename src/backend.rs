//! Partie « réseau » du client : elle tourne sur son propre thread avec un
//! runtime tokio, et dialogue avec l'interface par deux canaux :
//! l'interface envoie des [`Command`], le backend répond par des [`Event`].
//!
//! Ce découpage évite de mélanger l'exécuteur de GPUI et celui de tokio.
//! Quand l'interface abandonne le backend (déconnexion), le canal des
//! commandes se ferme, `run` se termine et le runtime est détruit avec
//! toutes ses tâches, WebSocket compris.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::{Mutex, mpsc};
use tokio_tungstenite::tungstenite::Message;

use crate::api::{Channel, ChannelMember, Client, Post, Preference, Reaction, SidebarCategory, Team, User};

/// Taille d'une page de messages.
pub const PAGE_SIZE: usize = 60;

#[derive(Debug)]
pub enum Command {
    LoadTeam { team_id: String },
    LoadCategories { team_id: String },
    LoadPreferences,
    SetCategoryCollapsed { team_id: String, category: SidebarCategory },
    LoadChannel { channel_id: String },
    LoadOlder { channel_id: String, before: String },
    LoadThread { root_id: String },
    Send { channel_id: String, root_id: String, message: String },
    Edit { post_id: String, message: String },
    Delete { post_id: String },
    React { post_id: String, emoji: String, add: bool },
    Typing { channel_id: String, parent_id: String },
    MarkRead { channel_id: String },
    /// Charge un canal inconnu (ex. nouveau message direct reçu en temps réel).
    FetchChannel { channel_id: String },
    Notify { title: String, body: String },
}

#[derive(Debug, Clone)]
pub enum Event {
    Ready { me: User, teams: Vec<Team> },
    Users(Vec<User>),
    Avatar { user_id: String, bytes: Arc<Vec<u8>> },
    Statuses(Vec<(String, String)>),
    Channels { team_id: String, channels: Vec<Channel>, members: Vec<ChannelMember> },
    Categories { team_id: String, categories: Vec<SidebarCategory> },
    /// Préférences brutes : barre latérale, thème…
    Prefs(Vec<Preference>),
    ChannelAdded(Channel),
    ChannelPosts { channel_id: String, posts: Vec<Post> },
    OlderPosts { channel_id: String, posts: Vec<Post> },
    Thread { root_id: String, posts: Vec<Post> },
    Posted { post: Post, mentions_me: bool, channel_type: String },
    PostEdited(Post),
    PostDeleted(Post),
    ReactionAdded(Reaction, String),
    ReactionRemoved(Reaction, String),
    Typing { channel_id: String, parent_id: String, user_id: String },
    /// Les catégories de l'équipe ont changé côté serveur : à recharger.
    CategoriesChanged { team_id: String },
    PrefsChanged,
    Connection(bool),
    Error(String),
}

pub struct Backend {
    pub commands: async_channel::Sender<Command>,
    pub events: async_channel::Receiver<Event>,
}

/// Démarre le backend sur un thread dédié.
pub fn start(server_url: String, token: String) -> Backend {
    let (cmd_tx, cmd_rx) = async_channel::unbounded::<Command>();
    let (ev_tx, ev_rx) = async_channel::unbounded::<Event>();

    std::thread::Builder::new()
        .name("mm-backend".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("runtime tokio");
            rt.block_on(async move {
                if let Err(err) = run(server_url, token, cmd_rx, ev_tx.clone()).await {
                    let _ = ev_tx.send(Event::Error(format!("{err:#}"))).await;
                }
            });
        })
        .expect("thread backend");

    Backend {
        commands: cmd_tx,
        events: ev_rx,
    }
}

/// Contexte partagé par toutes les tâches du backend.
#[derive(Clone)]
struct Ctx {
    client: Client,
    me_id: String,
    ev: async_channel::Sender<Event>,
    /// Utilisateurs déjà demandés au serveur.
    known_users: Arc<Mutex<HashSet<String>>>,
    /// Messages à envoyer sur le WebSocket (ex. « en train d'écrire »).
    ws_out: mpsc::UnboundedSender<Value>,
}

impl Ctx {
    async fn emit(&self, event: Event) {
        let _ = self.ev.send(event).await;
    }

    async fn report<T>(&self, result: Result<T>) -> Option<T> {
        match result {
            Ok(v) => Some(v),
            Err(err) => {
                self.emit(Event::Error(format!("{err:#}"))).await;
                None
            }
        }
    }

    /// Récupère les utilisateurs inconnus (profil, présence, avatar) et
    /// publie leurs profils avant que l'appelant ne publie ses données.
    async fn ensure_users(&self, ids: impl IntoIterator<Item = String>) {
        let missing: Vec<String> = {
            let mut known = self.known_users.lock().await;
            let missing: Vec<String> = ids
                .into_iter()
                .filter(|id| !id.is_empty() && !known.contains(id))
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            known.extend(missing.iter().cloned());
            missing
        };
        if missing.is_empty() {
            return;
        }
        let Some(users) = self.report(self.client.users_by_ids(&missing).await).await else {
            return;
        };

        // Présence et avatars : en arrière-plan, sans bloquer l'affichage.
        let ctx = self.clone();
        let ids = missing.clone();
        tokio::spawn(async move {
            if let Ok(statuses) = ctx.client.statuses(&ids).await {
                ctx.emit(Event::Statuses(statuses.into_iter().map(|s| (s.user_id, s.status)).collect()))
                    .await;
            }
        });
        for user in &users {
            let ctx = self.clone();
            let (id, update) = (user.id.clone(), user.last_picture_update);
            tokio::spawn(async move {
                if let Ok(bytes) = ctx.client.profile_image(&id, update).await {
                    ctx.emit(Event::Avatar { user_id: id, bytes: Arc::new(bytes) }).await;
                }
            });
        }
        self.emit(Event::Users(users)).await;
    }
}

async fn run(
    server_url: String,
    token: String,
    commands: async_channel::Receiver<Command>,
    ev: async_channel::Sender<Event>,
) -> Result<()> {
    let client = Client::new(&server_url, &token)?;
    let me = client.me().await?;
    let teams = client.my_teams().await?;
    let (ws_out, ws_out_rx) = mpsc::unbounded_channel();
    let ctx = Ctx {
        client,
        me_id: me.id.clone(),
        ev,
        known_users: Arc::default(),
        ws_out,
    };
    ctx.ensure_users([me.id.clone()]).await;
    ctx.emit(Event::Ready { me, teams }).await;
    if let Some(prefs) = ctx.report(ctx.client.preferences().await).await {
        ctx.emit(Event::Prefs(prefs)).await;
    }

    // Temps réel : boucle WebSocket indépendante, avec reconnexion.
    tokio::spawn(websocket_loop(ctx.clone(), ws_out_rx));

    while let Ok(cmd) = commands.recv().await {
        let ctx = ctx.clone();
        // Chaque commande est traitée en parallèle : un chargement lent
        // ne bloque pas l'envoi d'un message.
        tokio::spawn(async move {
            let result = handle(cmd, &ctx).await;
            ctx.report(result).await;
        });
    }
    Ok(())
}

async fn handle(cmd: Command, ctx: &Ctx) -> Result<()> {
    let client = &ctx.client;
    match cmd {
        Command::LoadTeam { team_id } => {
            let (channels, members, categories) = tokio::try_join!(
                client.my_channels(&team_id),
                client.my_channel_members(&team_id),
                client.categories(&team_id),
            )?;
            // Noms des correspondants des messages directs.
            let partners = channels.iter().filter_map(|c| c.direct_partner(&ctx.me_id));
            ctx.ensure_users(partners).await;
            ctx.emit(Event::Categories { team_id: team_id.clone(), categories }).await;
            ctx.emit(Event::Channels { team_id, channels, members }).await;
        }
        Command::LoadCategories { team_id } => {
            let categories = client.categories(&team_id).await?;
            ctx.emit(Event::Categories { team_id, categories }).await;
        }
        Command::LoadPreferences => {
            let prefs = client.preferences().await?;
            ctx.emit(Event::Prefs(prefs)).await;
        }
        Command::SetCategoryCollapsed { team_id, category } => {
            client.update_category(&team_id, &category).await?;
        }
        Command::LoadChannel { channel_id } => {
            let posts = client.channel_posts(&channel_id, PAGE_SIZE as u32).await?;
            ctx.ensure_users(posts.iter().map(|p| p.user_id.clone())).await;
            ctx.emit(Event::ChannelPosts { channel_id, posts }).await;
        }
        Command::LoadOlder { channel_id, before } => {
            let posts = client.channel_posts_before(&channel_id, &before, PAGE_SIZE as u32).await?;
            ctx.ensure_users(posts.iter().map(|p| p.user_id.clone())).await;
            ctx.emit(Event::OlderPosts { channel_id, posts }).await;
        }
        Command::LoadThread { root_id } => {
            let posts = client.thread(&root_id).await?;
            ctx.ensure_users(posts.iter().map(|p| p.user_id.clone())).await;
            ctx.emit(Event::Thread { root_id, posts }).await;
        }
        // Les résultats des quatre commandes suivantes reviennent par le
        // WebSocket (posted, post_edited, post_deleted, reaction_*) :
        // l'interface les applique à ce moment-là, sans doublon.
        Command::Send { channel_id, root_id, message } => {
            client.create_post(&channel_id, &root_id, &message).await?;
        }
        Command::Edit { post_id, message } => client.patch_post(&post_id, &message).await?,
        Command::Delete { post_id } => client.delete_post(&post_id).await?,
        Command::React { post_id, emoji, add } => {
            if add {
                client.add_reaction(&ctx.me_id, &post_id, &emoji).await?;
            } else {
                client.remove_reaction(&ctx.me_id, &post_id, &emoji).await?;
            }
        }
        Command::Typing { channel_id, parent_id } => {
            let _ = ctx.ws_out.send(serde_json::json!({
                "action": "user_typing",
                "data": { "channel_id": channel_id, "parent_id": parent_id }
            }));
        }
        Command::MarkRead { channel_id } => client.view_channel(&channel_id).await?,
        Command::FetchChannel { channel_id } => {
            let channel = client.channel(&channel_id).await?;
            if let Some(partner) = channel.direct_partner(&ctx.me_id) {
                ctx.ensure_users([partner]).await;
            }
            ctx.emit(Event::ChannelAdded(channel)).await;
        }
        Command::Notify { title, body } => {
            tokio::task::spawn_blocking(move || crate::notify::show(&title, &body)).await?;
        }
    }
    Ok(())
}

async fn websocket_loop(ctx: Ctx, mut outgoing: mpsc::UnboundedReceiver<Value>) {
    let mut delay = Duration::from_secs(1);
    loop {
        match websocket_session(&ctx, &mut outgoing).await {
            Ok(()) => delay = Duration::from_secs(1),
            Err(err) => ctx.emit(Event::Error(format!("temps réel : {err:#}"))).await,
        }
        ctx.emit(Event::Connection(false)).await;
        if ctx.ev.is_closed() {
            return;
        }
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(Duration::from_secs(30));
    }
}

async fn websocket_session(ctx: &Ctx, outgoing: &mut mpsc::UnboundedReceiver<Value>) -> Result<()> {
    let (mut socket, _) = tokio_tungstenite::connect_async(ctx.client.websocket_url()).await?;

    // Authentification par « challenge » : le jeton part dans le premier
    // message, jamais dans l'URL.
    let mut seq = 1u64;
    let auth = serde_json::json!({
        "seq": seq,
        "action": "authentication_challenge",
        "data": { "token": ctx.client.token() }
    });
    socket.send(Message::Text(auth.to_string().into())).await?;

    let mut ping = tokio::time::interval(Duration::from_secs(30));
    ping.tick().await;

    loop {
        tokio::select! {
            _ = ping.tick() => {
                socket.send(Message::Ping(Vec::new().into())).await?;
            }
            Some(mut out) = outgoing.recv() => {
                seq += 1;
                out["seq"] = seq.into();
                socket.send(Message::Text(out.to_string().into())).await?;
            }
            msg = socket.next() => {
                let Some(msg) = msg else { return Ok(()) };
                match msg? {
                    Message::Text(text) => {
                        if let Ok(value) = serde_json::from_str::<Value>(&text) {
                            on_ws_event(value, ctx).await;
                        }
                    }
                    Message::Close(_) => return Ok(()),
                    _ => {}
                }
            }
        }
    }
}

/// Certains champs (`data.post`, `data.mentions`, `data.reaction`) sont du
/// JSON encodé dans une chaîne : on les décode une seconde fois.
fn nested<T: for<'de> serde::Deserialize<'de>>(data: &Value, key: &str) -> Option<T> {
    data.get(key)
        .and_then(Value::as_str)
        .and_then(|s| serde_json::from_str(s).ok())
}

fn str_field(value: &Value, pointer: &str) -> String {
    value.pointer(pointer).and_then(Value::as_str).unwrap_or_default().to_string()
}

async fn on_ws_event(value: Value, ctx: &Ctx) {
    let Some(kind) = value.get("event").and_then(Value::as_str) else {
        return;
    };
    let data = value.get("data").cloned().unwrap_or(Value::Null);

    let event = match kind {
        "hello" => Some(Event::Connection(true)),
        "posted" => nested::<Post>(&data, "post").map(|post| {
            let mentions: Vec<String> = nested(&data, "mentions").unwrap_or_default();
            Event::Posted {
                post,
                mentions_me: mentions.iter().any(|m| *m == ctx.me_id),
                channel_type: str_field(&data, "/channel_type"),
            }
        }),
        "post_edited" => nested::<Post>(&data, "post").map(Event::PostEdited),
        "post_deleted" => nested::<Post>(&data, "post").map(Event::PostDeleted),
        "reaction_added" => nested::<Reaction>(&data, "reaction").map(|r| {
            Event::ReactionAdded(r, reaction_post_id(&data))
        }),
        "reaction_removed" => nested::<Reaction>(&data, "reaction").map(|r| {
            Event::ReactionRemoved(r, reaction_post_id(&data))
        }),
        "typing" => Some(Event::Typing {
            channel_id: str_field(&value, "/broadcast/channel_id"),
            parent_id: str_field(&data, "/parent_id"),
            user_id: str_field(&data, "/user_id"),
        }),
        "status_change" => {
            let (user_id, status) = (str_field(&data, "/user_id"), str_field(&data, "/status"));
            (!user_id.is_empty()).then(|| Event::Statuses(vec![(user_id, status)]))
        }
        // Le contenu exact de ces événements n'est pas documenté : on se
        // contente de signaler qu'il faut recharger.
        "sidebar_category_created"
        | "sidebar_category_updated"
        | "sidebar_category_deleted"
        | "sidebar_category_order_updated" => {
            let team_id = match str_field(&value, "/broadcast/team_id") {
                t if t.is_empty() => str_field(&data, "/team_id"),
                t => t,
            };
            Some(Event::CategoriesChanged { team_id })
        }
        "preference_changed" | "preferences_changed" | "preferences_deleted" => Some(Event::PrefsChanged),
        _ => None,
    };

    if let Some(event) = event {
        match &event {
            Event::Posted { post, .. } => ctx.ensure_users([post.user_id.clone()]).await,
            Event::Typing { user_id, .. } => ctx.ensure_users([user_id.clone()]).await,
            _ => {}
        }
        ctx.emit(event).await;
    }
}

fn reaction_post_id(data: &Value) -> String {
    data.get("reaction")
        .and_then(Value::as_str)
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .map(|v| str_field(&v, "/post_id"))
        .unwrap_or_default()
}

/// Regroupe les réactions d'un message par emoji, en indiquant si
/// l'utilisateur courant y a participé : [("+1", 3, true), ...].
pub fn group_reactions(reactions: &[Reaction], me: &str) -> Vec<(String, usize, bool)> {
    let mut out: Vec<(String, usize, bool)> = Vec::new();
    for r in reactions {
        match out.iter_mut().find(|(name, ..)| *name == r.emoji_name) {
            Some(entry) => {
                entry.1 += 1;
                entry.2 |= r.user_id == me;
            }
            None => out.push((r.emoji_name.clone(), 1, r.user_id == me)),
        }
    }
    out
}
