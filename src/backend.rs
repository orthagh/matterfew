//! Partie « réseau » du client : elle tourne sur son propre thread avec un
//! runtime tokio, et dialogue avec l'interface par deux canaux :
//! l'interface envoie des [`Command`], le backend répond par des [`Event`].
//!
//! Ce découpage évite de mélanger l'exécuteur de GPUI et celui de tokio.
//! Quand l'interface abandonne le backend (déconnexion), le canal des
//! commandes se ferme, `run` se termine et le runtime est détruit avec
//! toutes ses tâches, WebSocket compris.

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::{Mutex, mpsc};
use tokio_tungstenite::tungstenite::Message;

use crate::api::{
    Channel, ChannelMember, Client, FileInfo, Post, Preference, Reaction, SidebarCategory, Team, TeamUnread, User,
    UserThread,
};

/// Taille d'une page de messages.
pub const PAGE_SIZE: usize = 60;

/// Type d'autocomplétion selon le caractère tapé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcKind {
    /// `@` : personnes.
    User,
    /// `~` : canaux.
    Channel,
    /// `:` : emojis.
    Emoji,
    /// `/` en début de message : commandes.
    Command,
}

/// Contenu du panneau latéral (hors fil de discussion).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SideKind {
    Pinned,
    Saved,
    Search,
    /// Fils suivis (chargés par `LoadThreads`).
    Threads,
    /// Mentions récentes, toutes équipes confondues.
    Mentions,
    /// Fichiers et liens du canal (chargés par `LoadChannelFiles`).
    Files,
    /// Informations du canal (rien à charger).
    Channel,
}

/// Une proposition de l'autocomplétion.
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub label: String,
    pub detail: String,
    /// Texte inséré à la place du mot en cours.
    pub insert: String,
    /// Personne proposée (pour afficher son avatar).
    pub user_id: Option<String>,
    /// Emoji : caractère Unicode, ou nom d'emoji personnalisé.
    pub glyph: Option<String>,
}

#[derive(Debug)]
pub enum Command {
    LoadTeam {
        team_id: String,
    },
    LoadCategories {
        team_id: String,
    },
    LoadPreferences,
    SetCategoryCollapsed {
        team_id: String,
        category: SidebarCategory,
    },
    LoadChannel {
        channel_id: String,
    },
    LoadOlder {
        channel_id: String,
        before: String,
    },
    /// Rattrape les messages créés, modifiés ou supprimés depuis `since`.
    SyncChannel {
        channel_id: String,
        since: i64,
    },
    /// Fils réduits actifs ou non (décidé par l'interface d'après la
    /// configuration du serveur et les préférences).
    SetCollapsedThreads(bool),
    LoadThread {
        root_id: String,
    },
    Send {
        channel_id: String,
        root_id: String,
        message: String,
        file_ids: Vec<String>,
        /// Identifiant local : signale l'envoi réussi ou échoué (`Sent`, `SendFailed`).
        pending_id: String,
    },
    /// Enregistre des préférences (emojis récents…).
    SavePreferences(Vec<Preference>),
    /// Préférences de notification du compte (carte complète).
    SaveNotifyProps(HashMap<String, String>),
    /// Paramètres du canal : nom affiché et nom dans les liens.
    RenameChannel {
        channel_id: String,
        display_name: String,
        name: String,
    },
    MakeChannelPrivate {
        channel_id: String,
    },
    /// Archive (`archive`) ou désarchive un canal.
    ArchiveChannel {
        channel_id: String,
        archive: bool,
    },
    SetChannelAdmin {
        channel_id: String,
        user_id: String,
        admin: bool,
    },
    RemoveChannelMember {
        channel_id: String,
        user_id: String,
    },
    DeleteCategory {
        team_id: String,
        category_id: String,
    },
    LoadBookmarks {
        channel_id: String,
    },
    /// Pages d'un PDF joint, rendues en images (aperçu).
    LoadPdf {
        file_id: String,
    },
    AddBookmark {
        channel_id: String,
        title: String,
        url: String,
    },
    DeleteBookmark {
        channel_id: String,
        bookmark_id: String,
    },
    LoadTeamUnreads,
    /// Aperçu d'une image jointe.
    LoadPreview {
        file_id: String,
    },
    /// Image complète, pour la visionneuse.
    LoadFullImage {
        file_id: String,
    },
    /// Image d'un aperçu de lien ou d'une intégration (URL complète).
    LoadExternalImage {
        url: String,
    },
    /// Bouton d'un message interactif.
    PostAction {
        post_id: String,
        action_id: String,
    },
    /// Propositions pour le mot en cours de saisie (`seq` : numéro de la demande).
    Autocomplete {
        seq: u64,
        kind: AcKind,
        term: String,
        team_id: String,
        channel_id: String,
        root_id: String,
    },
    /// Image d'un emoji personnalisé.
    LoadEmoji {
        id: String,
        name: String,
    },
    /// Messages épinglés (`channel_id`), enregistrés, ou résultats de `terms`.
    LoadSide {
        kind: SideKind,
        team_id: String,
        channel_id: String,
        terms: String,
    },
    LoadMembers {
        channel_id: String,
    },
    /// Fichiers d'un canal (`terms` : la recherche `in:` correspondante).
    LoadChannelFiles {
        team_id: String,
        channel_id: String,
        terms: String,
    },
    /// Carte de l'équipe : nombre de membres, droit de modifier.
    LoadTeamInfo {
        team_id: String,
    },
    SaveTeamDescription {
        team_id: String,
        description: String,
    },
    /// Équipes ouvertes que l'on peut rejoindre.
    LoadJoinableTeams,
    JoinTeam {
        team_id: String,
    },
    LeaveTeam {
        team_id: String,
    },
    /// Paramètres d'une équipe : nom, description, ouverte à tous.
    SaveTeamSettings {
        team_id: String,
        display_name: String,
        description: String,
        open: bool,
    },
    PatchChannel {
        channel_id: String,
        purpose: String,
        header: String,
    },
    /// Lien vers un message (`…/pl/<id>`) : le retrouver et l'ouvrir.
    OpenPermalink {
        post_id: String,
    },
    /// Lien vers un canal (`…/<équipe>/channels/<nom>`).
    OpenChannelLink {
        team_name: String,
        channel_name: String,
    },
    FollowThread {
        team_id: String,
        root_id: String,
        follow: bool,
    },
    Remind {
        post_id: String,
        at: i64,
    },
    CreateChannel {
        team_id: String,
        name: String,
        display_name: String,
        purpose: String,
        private: bool,
    },
    CreateCategory {
        team_id: String,
        name: String,
    },
    InviteByEmail {
        team_id: String,
        emails: Vec<String>,
    },
    /// Personnes à ajouter au canal (recherche dans l'équipe).
    SearchUsersForChannel {
        team_id: String,
        channel_id: String,
        term: String,
    },
    AddChannelMember {
        channel_id: String,
        user_id: String,
    },
    /// Profil d'après le nom d'utilisateur (clic sur une @mention).
    LoadUserByName {
        username: String,
    },
    LoadThreads {
        team_id: String,
    },
    MarkAllThreadsRead {
        team_id: String,
    },
    /// Préférences de notification d'un canal (seuls les champs donnés changent).
    SetChannelNotify {
        channel_id: String,
        props: Vec<(&'static str, String)>,
    },
    /// Canaux publics (ou archivés) de l'équipe, pour « Parcourir les canaux ».
    LoadBrowseChannels {
        team_id: String,
        archived: bool,
    },
    JoinChannel {
        channel_id: String,
    },
    /// Personnes du serveur (nouveau message direct).
    SearchUsers {
        term: String,
    },
    /// Ouvre (ou crée) un message de groupe avec ces personnes.
    OpenGroup {
        user_ids: Vec<String>,
    },
    /// Statut personnalisé ; `None` l'efface.
    SetCustomStatus(Option<crate::api::CustomStatus>),
    MarkThreadRead {
        team_id: String,
        root_id: String,
    },
    /// Ouvre (ou crée) le message direct avec quelqu'un.
    OpenDirect {
        user_id: String,
    },
    MarkUnread {
        post_id: String,
    },
    SavePost {
        post_id: String,
        save: bool,
    },
    PinPost {
        post_id: String,
        pin: bool,
    },
    UpdateCategories {
        team_id: String,
        categories: Vec<SidebarCategory>,
    },
    SetMuted {
        channel_id: String,
        muted: bool,
    },
    LeaveChannel {
        channel_id: String,
    },
    MarkChannelsRead {
        channel_ids: Vec<String>,
    },
    SetStatus {
        status: String,
        /// Fin de « Ne pas déranger » (secondes Unix), 0 sans échéance.
        dnd_end_time: i64,
    },
    /// Commande « / » envoyée depuis la zone de saisie.
    Slash {
        channel_id: String,
        team_id: String,
        root_id: String,
        command: String,
    },
    /// Télécharge un fichier joint ; `open` : l'ouvrir ensuite (sinon,
    /// l'afficher dans l'explorateur).
    Download {
        file: FileInfo,
        open: bool,
    },
    /// Envoie un fichier local ; `local_id` identifie la pièce jointe en attente.
    /// Arrête un envoi de fichier en cours.
    CancelUpload {
        local_id: u64,
    },
    Upload {
        local_id: u64,
        channel_id: String,
        path: PathBuf,
    },
    Edit {
        post_id: String,
        message: String,
    },
    Delete {
        post_id: String,
    },
    React {
        post_id: String,
        emoji: String,
        add: bool,
    },
    Typing {
        channel_id: String,
        parent_id: String,
    },
    MarkRead {
        channel_id: String,
    },
    /// Charge un canal inconnu (ex. nouveau message direct reçu en temps réel).
    FetchChannel {
        channel_id: String,
    },
    Notify {
        title: String,
        body: String,
        target: Option<crate::desktop::ClickTarget>,
    },
}

#[derive(Debug, Clone)]
pub enum Event {
    /// Connexion établie. `config` : configuration publique du serveur ;
    /// `prefs` : préférences de l'utilisateur (thème, barre latérale, fils…).
    Ready {
        me: User,
        teams: Vec<Team>,
        config: HashMap<String, String>,
        prefs: Vec<Preference>,
    },
    ChannelSince {
        channel_id: String,
        posts: Vec<Post>,
    },
    /// Des événements temps réel ont été perdus : tout resynchroniser.
    Resync,
    TeamIcon {
        team_id: String,
        bytes: Arc<Vec<u8>>,
    },
    TeamUnreads(Vec<TeamUnread>),
    FilePreview {
        file_id: String,
        bytes: Arc<Vec<u8>>,
    },
    FullImage {
        file_id: String,
        result: Result<Arc<Vec<u8>>, String>,
    },
    ExternalImage {
        url: String,
        bytes: Arc<Vec<u8>>,
    },
    Suggestions {
        seq: u64,
        kind: AcKind,
        items: Vec<Suggestion>,
    },
    SidePosts {
        kind: SideKind,
        posts: Vec<Post>,
    },
    Members {
        channel_id: String,
        user_ids: Vec<String>,
        admins: Vec<String>,
        total: i64,
    },
    Threads(Vec<UserThread>),
    Sent {
        pending_id: String,
    },
    Bookmarks {
        channel_id: String,
        bookmarks: Vec<crate::api::Bookmark>,
    },
    /// Pages PNG d'un PDF et nombre total de pages, ou l'erreur.
    PdfPages {
        file_id: String,
        result: Result<PdfPages, String>,
    },
    /// Signets d'un canal modifiés ailleurs : à recharger.
    BookmarksChanged {
        channel_id: String,
    },
    SendFailed {
        pending_id: String,
        error: String,
    },
    /// Un fil suivi a changé (nouvelle réponse, lecture…).
    ThreadUpdated {
        team_id: String,
        thread: UserThread,
    },
    ThreadRead {
        thread_id: String,
        unread_replies: i64,
        unread_mentions: i64,
    },
    ThreadFollow {
        thread_id: String,
        following: bool,
    },
    /// Préférences de notification d'un canal modifiées (ici ou ailleurs).
    MemberUpdated(ChannelMember),
    BrowseChannels {
        team_id: String,
        archived: bool,
        channels: Vec<Channel>,
    },
    UserSearch {
        term: String,
        users: Vec<User>,
    },
    ChannelFiles {
        channel_id: String,
        files: Vec<FileInfo>,
    },
    TeamInfo {
        team_id: String,
        total: i64,
        active: i64,
        admin: bool,
    },
    TeamUpdated(Team),
    /// Canal modifié (ici ou ailleurs) : nom, description, en-tête.
    ChannelUpdated(Channel),
    /// Canal créé par l'utilisateur : à ouvrir.
    ChannelCreated(Channel),
    /// Message à ouvrir (lien permanent).
    JumpTo(Post),
    /// Canal à ouvrir, éventuellement dans une autre équipe.
    OpenChannel {
        team_id: String,
        channel_id: String,
    },
    /// Information à afficher brièvement (invitations envoyées…).
    Notice(String),
    /// Règles du plugin Autolink lues sur le serveur.
    AutolinkRules(Vec<crate::autolink::RuleDef>),
    UserCandidates {
        channel_id: String,
        term: String,
        users: Vec<User>,
    },
    /// Aucun aperçu possible pour cette image : afficher une carte de fichier.
    PreviewFailed {
        file_id: String,
    },
    /// Équipe de canaux hors de l'équipe affichée (mentions récentes).
    ChannelTeams(Vec<(String, String)>),
    DirectOpened(Channel),
    /// Quelqu'un a rejoint ou quitté un canal : membres à recharger.
    MembersChanged {
        channel_id: String,
    },
    CustomEmoji {
        name: String,
        bytes: Arc<Vec<u8>>,
    },
    Downloaded {
        path: PathBuf,
        open: bool,
    },
    /// Octets d'un fichier déjà envoyés.
    UploadProgress {
        local_id: u64,
        sent: u64,
    },
    Uploaded {
        local_id: u64,
        file: FileInfo,
    },
    UploadFailed {
        local_id: u64,
        error: String,
    },
    Users(Vec<User>),
    Avatar {
        user_id: String,
        bytes: Arc<Vec<u8>>,
    },
    Statuses(Vec<(String, String)>),
    /// Fin de son propre « Ne pas déranger » (secondes Unix), 0 sans échéance.
    DndEnd(i64),
    /// Équipes visibles (ouvertes), pour « Rejoindre une autre équipe ».
    JoinableTeams(Vec<Team>),
    /// Équipe rejointe.
    TeamJoined(Team),
    Channels {
        team_id: String,
        channels: Vec<Channel>,
        members: Vec<ChannelMember>,
    },
    Categories {
        team_id: String,
        categories: Vec<SidebarCategory>,
    },
    /// Préférences brutes : barre latérale, thème…
    Prefs(Vec<Preference>),
    ChannelAdded(Channel),
    ChannelPosts {
        channel_id: String,
        posts: Vec<Post>,
    },
    OlderPosts {
        channel_id: String,
        posts: Vec<Post>,
    },
    Thread {
        root_id: String,
        posts: Vec<Post>,
    },
    /// `followed` : l'utilisateur suit le fil de cette réponse.
    Posted {
        post: Post,
        mentions_me: bool,
        followed: bool,
        channel_type: String,
        team_id: String,
    },
    PostEdited(Post),
    PostDeleted(Post),
    ReactionAdded(Reaction, String),
    ReactionRemoved(Reaction, String),
    Typing {
        channel_id: String,
        parent_id: String,
        user_id: String,
    },
    /// Les catégories de l'équipe ont changé côté serveur : à recharger.
    CategoriesChanged {
        team_id: String,
    },
    PrefsChanged,
    Connection(bool),
    /// Erreur à afficher ; `expired` : le serveur a refusé le jeton.
    Error {
        message: String,
        expired: bool,
    },
}

impl Event {
    pub fn error(err: &anyhow::Error) -> Self {
        let expired = err.chain().any(|e| e.is::<crate::api::Unauthorized>());
        Event::Error { message: format!("{err:#}"), expired }
    }
}

/// Pages PNG d'un PDF et nombre total de pages.
pub type PdfPages = (Vec<Arc<Vec<u8>>>, u32);

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
                    let _ = ev_tx.send(Event::error(&err)).await;
                }
            });
        })
        .expect("thread backend");

    Backend { commands: cmd_tx, events: ev_rx }
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
    /// Fils réduits actifs (paramètre des requêtes de messages).
    crt: Arc<AtomicBool>,
    me_username: String,
    me_admin: bool,
    /// Équipes de l'utilisateur (recherche des mentions dans toutes).
    team_ids: Vec<String>,
    /// Envois de fichiers en cours, annulables.
    uploads: Arc<std::sync::Mutex<HashMap<u64, tokio::task::AbortHandle>>>,
}

impl Ctx {
    fn crt(&self) -> bool {
        self.crt.load(Ordering::Relaxed)
    }
}

impl Ctx {
    async fn emit(&self, event: Event) {
        let _ = self.ev.send(event).await;
    }

    async fn report<T>(&self, result: Result<T>) -> Option<T> {
        match result {
            Ok(v) => Some(v),
            Err(err) => {
                self.emit(Event::error(&err)).await;
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
                ctx.emit(Event::Statuses(statuses.into_iter().map(|s| (s.user_id, s.status)).collect())).await;
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
    let (me, teams, config, prefs) =
        tokio::try_join!(client.me(), client.my_teams(), client.client_config(), client.preferences(),)?;
    let (ws_out, ws_out_rx) = mpsc::unbounded_channel();
    let ctx = Ctx {
        client,
        me_id: me.id.clone(),
        ev,
        known_users: Arc::default(),
        ws_out,
        crt: Arc::new(AtomicBool::new(true)),
        me_username: me.username.clone(),
        me_admin: me.is_system_admin(),
        team_ids: teams.iter().map(|t| t.id.clone()).collect(),
        uploads: Arc::default(),
    };
    ctx.ensure_users([me.id.clone()]).await;
    // Logos des équipes, en arrière-plan.
    for team in teams.iter().filter(|t| t.last_team_icon_update > 0) {
        let ctx = ctx.clone();
        let (id, update) = (team.id.clone(), team.last_team_icon_update);
        tokio::spawn(async move {
            if let Ok(bytes) = ctx.client.team_image(&id, update).await {
                // Réduit ici (filtre de qualité) : affiché en 40 px, net même à 200 %.
                let bytes =
                    tokio::task::spawn_blocking(move || crate::thumbs::downscale_png(&bytes, 96).unwrap_or(bytes))
                        .await
                        .unwrap_or_default();
                ctx.emit(Event::TeamIcon { team_id: id, bytes: Arc::new(bytes) }).await;
            }
        });
    }
    let my_id = me.id.clone();
    ctx.emit(Event::Ready { me, teams, config, prefs }).await;
    // Échéance de « Ne pas déranger » (absente des événements de statut).
    {
        let ctx = ctx.clone();
        tokio::spawn(async move {
            if let Ok(s) = ctx.client.status(&my_id).await {
                ctx.emit(Event::DndEnd(if s.status == "dnd" { s.dnd_end_time } else { 0 })).await;
            }
        });
    }
    // Liens automatiques du plugin Autolink : la configuration n'est lisible
    // que par un administrateur ; sinon, rien (les règles de ui.json restent).
    if let Ok(config) = ctx.client.admin_config().await {
        let rules = crate::autolink::from_server_config(&config);
        if !rules.is_empty() {
            ctx.emit(Event::AutolinkRules(rules)).await;
        }
    }
    if let Some(unreads) = ctx.report(ctx.client.team_unreads().await).await {
        ctx.emit(Event::TeamUnreads(unreads)).await;
    }

    // Temps réel : boucle WebSocket indépendante, avec reconnexion.
    tokio::spawn(websocket_loop(ctx.clone(), ws_out_rx));
    // Présence : rafraîchie chaque minute pour les utilisateurs connus.
    tokio::spawn(status_polling(ctx.clone()));

    while let Ok(cmd) = commands.recv().await {
        // Réglage immédiat, avant les commandes qui en dépendent.
        if let Command::SetCollapsedThreads(on) = cmd {
            ctx.crt.store(on, Ordering::Relaxed);
            continue;
        }
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
            let posts = client.channel_posts(&channel_id, PAGE_SIZE as u32, ctx.crt()).await?;
            ctx.ensure_users(posts.iter().map(|p| p.user_id.clone())).await;
            ctx.emit(Event::ChannelPosts { channel_id, posts }).await;
        }
        Command::LoadOlder { channel_id, before } => {
            let posts = client.channel_posts_before(&channel_id, &before, PAGE_SIZE as u32, ctx.crt()).await?;
            ctx.ensure_users(posts.iter().map(|p| p.user_id.clone())).await;
            ctx.emit(Event::OlderPosts { channel_id, posts }).await;
        }
        Command::SyncChannel { channel_id, since } => {
            let posts = client.channel_posts_since(&channel_id, since, ctx.crt()).await?;
            ctx.ensure_users(posts.iter().map(|p| p.user_id.clone())).await;
            ctx.emit(Event::ChannelSince { channel_id, posts }).await;
        }
        Command::SetCollapsedThreads(_) => {}
        Command::LoadThread { root_id } => {
            let posts = client.thread(&root_id).await?;
            ctx.ensure_users(posts.iter().map(|p| p.user_id.clone())).await;
            ctx.emit(Event::Thread { root_id, posts }).await;
        }
        // Les résultats des quatre commandes suivantes reviennent par le
        // WebSocket (posted, post_edited, post_deleted, reaction_*) :
        // l'interface les applique à ce moment-là, sans doublon.
        Command::Send { channel_id, root_id, message, file_ids, pending_id } => {
            // Échec : l'interface propose de réessayer (pas d'erreur générale).
            match client.create_post(&channel_id, &root_id, &message, &file_ids, &pending_id).await {
                Ok(_) => ctx.emit(Event::Sent { pending_id }).await,
                Err(err) => ctx.emit(Event::SendFailed { pending_id, error: format!("{err:#}") }).await,
            }
        }
        Command::SavePreferences(prefs) => client.save_preferences(&ctx.me_id, &prefs).await?,
        Command::SaveNotifyProps(props) => {
            let user = client.set_notify_props(&ctx.me_id, &props).await?;
            ctx.emit(Event::Users(vec![user])).await;
        }
        Command::RenameChannel { channel_id, display_name, name } => {
            let fields = serde_json::json!({ "display_name": display_name, "name": name });
            let channel = client.patch_channel_fields(&channel_id, &fields).await?;
            ctx.emit(Event::ChannelUpdated(channel)).await;
        }
        Command::MakeChannelPrivate { channel_id } => {
            let channel = client.make_channel_private(&channel_id).await?;
            ctx.emit(Event::ChannelUpdated(channel)).await;
        }
        Command::ArchiveChannel { channel_id, archive } => {
            let channel = if archive {
                client.archive_channel(&channel_id).await?;
                client.channel(&channel_id).await?
            } else {
                client.unarchive_channel(&channel_id).await?
            };
            ctx.emit(Event::ChannelUpdated(channel)).await;
        }
        Command::SetChannelAdmin { channel_id, user_id, admin } => {
            client.set_channel_admin(&channel_id, &user_id, admin).await?;
            ctx.emit(Event::MembersChanged { channel_id }).await;
        }
        Command::RemoveChannelMember { channel_id, user_id } => {
            client.leave_channel(&channel_id, &user_id).await?;
            ctx.emit(Event::MembersChanged { channel_id }).await;
        }
        Command::LoadPdf { file_id } => {
            let result = match client.file_content(&file_id).await {
                Ok(bytes) => tokio::task::spawn_blocking(move || crate::pdf::render_pages(&bytes, 1400))
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.map_err(|e| format!("{e:#}")))
                    .map(|(pages, total)| (pages.into_iter().map(Arc::new).collect(), total)),
                Err(err) => Err(format!("{err:#}")),
            };
            ctx.emit(Event::PdfPages { file_id, result }).await;
        }
        // Signets désactivés ou serveur trop ancien : simplement aucun signet.
        Command::LoadBookmarks { channel_id } => {
            let bookmarks = client.bookmarks(&channel_id).await.unwrap_or_default();
            ctx.emit(Event::Bookmarks { channel_id, bookmarks }).await;
        }
        Command::AddBookmark { channel_id, title, url } => {
            client.add_bookmark(&channel_id, &title, &url).await?;
            let bookmarks = client.bookmarks(&channel_id).await?;
            ctx.emit(Event::Bookmarks { channel_id, bookmarks }).await;
        }
        Command::DeleteBookmark { channel_id, bookmark_id } => {
            client.delete_bookmark(&channel_id, &bookmark_id).await?;
            let bookmarks = client.bookmarks(&channel_id).await?;
            ctx.emit(Event::Bookmarks { channel_id, bookmarks }).await;
        }
        Command::DeleteCategory { team_id, category_id } => {
            client.delete_category(&team_id, &category_id).await?;
            ctx.emit(Event::CategoriesChanged { team_id }).await;
        }
        Command::LoadTeamUnreads => {
            let unreads = client.team_unreads().await?;
            ctx.emit(Event::TeamUnreads(unreads)).await;
        }
        Command::LoadPreview { file_id } => {
            // Certaines images n'ont pas d'aperçu (GIF, SVG, très petites, anciennes) :
            // miniature, puis fichier complet. Jamais d'erreur affichée pour un aperçu.
            let bytes = match client.file_preview(&file_id).await {
                Ok(b) => Ok(b),
                Err(_) => match client.file_thumbnail(&file_id).await {
                    Ok(b) => Ok(b),
                    Err(_) => client.file_content(&file_id).await,
                },
            };
            match bytes {
                Ok(bytes) => ctx.emit(Event::FilePreview { file_id, bytes: Arc::new(bytes) }).await,
                Err(_) => ctx.emit(Event::PreviewFailed { file_id }).await,
            }
        }
        Command::LoadChannelFiles { team_id, channel_id, terms } => {
            // Recherche refusée ou vide : l'interface complète avec les messages connus.
            let files: Vec<FileInfo> = client
                .search_files(&team_id, &terms)
                .await
                .unwrap_or_default()
                .into_iter()
                .filter(|f| f.channel_id.is_empty() || f.channel_id == channel_id)
                .collect();
            ctx.ensure_users(files.iter().map(|f| f.user_id.clone()).filter(|id| !id.is_empty())).await;
            ctx.emit(Event::ChannelFiles { channel_id, files }).await;
        }
        Command::LoadFullImage { file_id } => {
            let result = client.file_content(&file_id).await.map(Arc::new).map_err(|e| format!("{e:#}"));
            ctx.emit(Event::FullImage { file_id, result }).await;
        }
        Command::LoadExternalImage { url } => {
            // Échec silencieux : un aperçu manquant n'est pas une erreur à afficher.
            if let Ok(bytes) = client.image_at(&url).await {
                ctx.emit(Event::ExternalImage { url, bytes: Arc::new(bytes) }).await;
            }
        }
        Command::PostAction { post_id, action_id } => client.post_action(&post_id, &action_id).await?,
        Command::Autocomplete { seq, kind, term, team_id, channel_id, root_id } => {
            let items = match kind {
                AcKind::User => client
                    .autocomplete_users(&team_id, &channel_id, &term)
                    .await?
                    .into_iter()
                    .take(10)
                    .map(|u| Suggestion {
                        label: format!("@{}", u.username),
                        detail: u.display_name(),
                        insert: format!("@{} ", u.username),
                        user_id: Some(u.id.clone()),
                        glyph: None,
                    })
                    .collect(),
                AcKind::Channel => client
                    .autocomplete_channels(&team_id, &term)
                    .await?
                    .into_iter()
                    .take(10)
                    .map(|c| Suggestion {
                        label: format!("~{}", c.name),
                        detail: c.display_name.clone(),
                        insert: format!("~{} ", c.name),
                        user_id: None,
                        glyph: None,
                    })
                    .collect(),
                AcKind::Emoji => {
                    let found = client.autocomplete_emoji(&term).await?;
                    for e in &found {
                        load_emoji(ctx, e.id.clone(), e.name.clone());
                    }
                    found
                        .into_iter()
                        .take(10)
                        .map(|e| Suggestion {
                            label: format!(":{}:", e.name),
                            detail: t!("autocomplete.custom_emoji").into(),
                            insert: format!(":{}: ", e.name),
                            user_id: None,
                            glyph: Some(e.name),
                        })
                        .collect()
                }
                AcKind::Command => client
                    .command_suggestions(&team_id, &channel_id, &root_id, &term)
                    .await?
                    .into_iter()
                    .take(12)
                    .map(|c| {
                        // Le serveur renvoie la commande sans son « / » (« jira create ») :
                        // on le remet. Le premier niveau s'affiche avec le « / ».
                        let complete = c.complete.trim_start_matches('/');
                        let top = !term.trim_start_matches('/').contains(' ');
                        let name = if top {
                            format!("/{}", c.suggestion.trim_start_matches('/'))
                        } else {
                            c.suggestion.clone()
                        };
                        Suggestion {
                            label: format!("{name} {}", c.hint).trim().to_string(),
                            detail: c.description,
                            insert: format!("/{complete} "),
                            user_id: None,
                            glyph: None,
                        }
                    })
                    .collect(),
            };
            ctx.emit(Event::Suggestions { seq, kind, items }).await;
        }
        Command::LoadEmoji { id, name } => {
            let bytes = client.emoji_image(&id).await?;
            ctx.emit(Event::CustomEmoji { name, bytes: Arc::new(bytes) }).await;
        }
        Command::LoadSide { kind, team_id, channel_id, terms } => {
            let posts = match kind {
                SideKind::Pinned => client.pinned_posts(&channel_id).await?,
                SideKind::Saved => client.saved_posts(&ctx.me_id).await?,
                SideKind::Search => client.search_posts(&team_id, &terms).await?,
                SideKind::Threads | SideKind::Files | SideKind::Channel => Vec::new(),
                SideKind::Mentions => {
                    // Toutes les équipes : une recherche « @nom » par équipe, fusionnées.
                    let terms = format!("@{}", ctx.me_username);
                    let mut all = Vec::new();
                    let mut teams = Vec::new();
                    for team in &ctx.team_ids {
                        if let Ok(mut found) = client.search_posts(team, &terms).await {
                            teams.extend(found.iter().map(|p| (p.channel_id.clone(), team.clone())));
                            all.append(&mut found);
                        }
                    }
                    ctx.emit(Event::ChannelTeams(teams)).await;
                    all.sort_by_key(|x| std::cmp::Reverse(x.create_at));
                    all.dedup_by(|a, b| a.id == b.id);
                    all.truncate(60);
                    all
                }
            };
            ctx.ensure_users(posts.iter().map(|p| p.user_id.clone())).await;
            ctx.emit(Event::SidePosts { kind, posts }).await;
        }
        Command::LoadMembers { channel_id } => {
            let (users, admins, total) = client.channel_users(&channel_id).await?;
            let ids: Vec<String> = users.iter().map(|u| u.id.clone()).collect();
            // Profils et avatars des inconnus, puis présence de tous.
            ctx.ensure_users(ids.clone()).await;
            if let Ok(statuses) = client.statuses(&ids).await {
                ctx.emit(Event::Statuses(statuses.into_iter().map(|s| (s.user_id, s.status)).collect())).await;
            }
            // Profils complets (rôles, fuseau…) même pour les personnes déjà connues.
            ctx.emit(Event::Users(users)).await;
            ctx.emit(Event::Members { channel_id, user_ids: ids, admins, total }).await;
        }
        Command::LoadUserByName { username } => {
            let user = client.user_by_username(&username).await?;
            ctx.ensure_users([user.id.clone()]).await;
            ctx.emit(Event::Users(vec![user])).await;
        }
        Command::LoadThreads { team_id } => {
            let threads = client.user_threads(&ctx.me_id, &team_id).await?;
            let mut ids: Vec<String> = Vec::new();
            for t in &threads {
                ids.push(t.post.user_id.clone());
                ids.extend(t.participants.iter().map(|u| u.id.clone()));
            }
            ctx.ensure_users(ids).await;
            ctx.emit(Event::Threads(threads)).await;
        }
        Command::MarkAllThreadsRead { team_id } => {
            client.mark_all_threads_read(&ctx.me_id, &team_id).await?;
            ctx.emit(Event::ThreadRead { thread_id: String::new(), unread_replies: 0, unread_mentions: 0 }).await;
        }
        Command::SetChannelNotify { channel_id, props } => {
            let map: HashMap<&str, String> = props.into_iter().collect();
            client.set_channel_notify_props(&channel_id, &ctx.me_id, &map).await?;
        }
        Command::LoadBrowseChannels { team_id, archived } => {
            let channels = client.team_channels(&team_id, archived).await?;
            ctx.emit(Event::BrowseChannels { team_id, archived, channels }).await;
        }
        Command::JoinChannel { channel_id } => {
            client.add_channel_member(&channel_id, &ctx.me_id).await?;
            let channel = client.channel(&channel_id).await?;
            ctx.emit(Event::ChannelCreated(channel)).await;
        }
        Command::SearchUsers { term } => {
            let users: Vec<User> = client
                .search_users(&term)
                .await?
                .into_iter()
                .filter(|u| u.id != ctx.me_id && u.delete_at == 0)
                .collect();
            ctx.ensure_users(users.iter().map(|u| u.id.clone())).await;
            ctx.emit(Event::UserSearch { term, users }).await;
        }
        Command::OpenGroup { user_ids } => {
            let channel = if user_ids.len() == 1 {
                client.direct_channel(&ctx.me_id, &user_ids[0]).await?
            } else {
                let mut ids = vec![ctx.me_id.clone()];
                ids.extend(user_ids.iter().cloned());
                client.group_channel(&ids).await?
            };
            ctx.ensure_users(user_ids).await;
            ctx.emit(Event::DirectOpened(channel)).await;
        }
        Command::SetCustomStatus(status) => match status {
            Some(status) => client.set_custom_status(&status).await?,
            None => client.clear_custom_status().await?,
        },
        Command::MarkThreadRead { team_id, root_id } => {
            let now = chrono::Utc::now().timestamp_millis();
            client.mark_thread_read(&ctx.me_id, &team_id, &root_id, now).await?;
        }
        Command::LoadTeamInfo { team_id } => {
            let ((total, active), admin) =
                tokio::try_join!(client.team_stats(&team_id), client.is_team_admin(&team_id))?;
            ctx.emit(Event::TeamInfo { team_id, total, active, admin: admin || ctx.me_admin }).await;
        }
        Command::LoadJoinableTeams => {
            let teams = client.listed_teams().await?;
            ctx.emit(Event::JoinableTeams(teams.into_iter().filter(|t| t.delete_at == 0).collect())).await;
        }
        Command::JoinTeam { team_id } => {
            client.join_team(&team_id, &ctx.me_id).await?;
            let team = client.team(&team_id).await?;
            ctx.emit(Event::TeamJoined(team)).await;
        }
        Command::LeaveTeam { team_id } => client.leave_team(&team_id, &ctx.me_id).await?,
        Command::SaveTeamSettings { team_id, display_name, description, open } => {
            let fields = serde_json::json!({
                "display_name": display_name,
                "description": description,
                "allow_open_invite": open,
            });
            let team = client.patch_team(&team_id, &fields).await?;
            ctx.emit(Event::TeamUpdated(team)).await;
        }
        Command::SaveTeamDescription { team_id, description } => {
            let team = client.patch_team_description(&team_id, &description).await?;
            ctx.emit(Event::TeamUpdated(team)).await;
        }
        Command::CreateChannel { team_id, name, display_name, purpose, private } => {
            let channel = client.create_channel(&team_id, &name, &display_name, &purpose, private).await?;
            ctx.emit(Event::ChannelCreated(channel)).await;
            let categories = client.categories(&team_id).await?;
            ctx.emit(Event::Categories { team_id, categories }).await;
        }
        Command::CreateCategory { team_id, name } => {
            client.create_category(&ctx.me_id, &team_id, &name).await?;
            let categories = client.categories(&team_id).await?;
            ctx.emit(Event::Categories { team_id, categories }).await;
        }
        Command::InviteByEmail { team_id, emails } => {
            let n = emails.len();
            client.invite_by_email(&team_id, &emails).await?;
            ctx.emit(Event::Notice(if n == 1 {
                t!("team.invitation_sent.one").into()
            } else {
                t!("team.invitation_sent.other", count = n).into()
            }))
            .await;
        }
        Command::OpenPermalink { post_id } => {
            let post = client.post_by_id(&post_id).await?;
            let team_id = client.channel_team(&post.channel_id).await?;
            if !team_id.is_empty() {
                ctx.emit(Event::ChannelTeams(vec![(post.channel_id.clone(), team_id)])).await;
            }
            ctx.ensure_users([post.user_id.clone()]).await;
            ctx.emit(Event::JumpTo(post)).await;
        }
        Command::OpenChannelLink { team_name, channel_name } => {
            let team = client.team_by_name(&team_name).await?;
            let channel = client.channel_by_name(&team.id, &channel_name).await?;
            ctx.emit(Event::OpenChannel { team_id: team.id, channel_id: channel.id }).await;
        }
        Command::FollowThread { team_id, root_id, follow } => {
            client.set_thread_following(&ctx.me_id, &team_id, &root_id, follow).await?;
        }
        Command::Remind { post_id, at } => {
            client.set_reminder(&ctx.me_id, &post_id, at).await?;
            ctx.emit(Event::Notice(t!("notice.reminder_set").to_string())).await;
        }
        Command::PatchChannel { channel_id, purpose, header } => {
            let channel = client.patch_channel(&channel_id, &purpose, &header).await?;
            ctx.emit(Event::ChannelUpdated(channel)).await;
        }
        Command::SearchUsersForChannel { team_id, channel_id, term } => {
            let users = client.users_not_in_channel(&team_id, &channel_id, &term).await?;
            ctx.ensure_users(users.iter().map(|u| u.id.clone())).await;
            ctx.emit(Event::UserCandidates { channel_id, term, users }).await;
        }
        Command::AddChannelMember { channel_id, user_id } => {
            client.add_channel_member(&channel_id, &user_id).await?;
            ctx.emit(Event::MembersChanged { channel_id }).await;
        }
        Command::OpenDirect { user_id } => {
            let channel = client.direct_channel(&ctx.me_id, &user_id).await?;
            ctx.ensure_users([user_id]).await;
            ctx.emit(Event::DirectOpened(channel)).await;
        }
        Command::MarkUnread { post_id } => client.set_unread(&ctx.me_id, &post_id).await?,
        Command::SavePost { post_id, save } => {
            let pref = [Preference {
                user_id: ctx.me_id.clone(),
                category: "flagged_post".into(),
                name: post_id,
                value: "true".into(),
            }];
            if save {
                client.save_preferences(&ctx.me_id, &pref).await?;
            } else {
                client.delete_preferences(&ctx.me_id, &pref).await?;
            }
        }
        Command::PinPost { post_id, pin } => client.set_pinned(&post_id, pin).await?,
        Command::UpdateCategories { team_id, categories } => client.update_categories(&team_id, &categories).await?,
        Command::SetMuted { channel_id, muted } => {
            let value = if muted { "mention" } else { "all" };
            client.set_channel_mark_unread(&channel_id, &ctx.me_id, value).await?;
        }
        Command::LeaveChannel { channel_id } => client.leave_channel(&channel_id, &ctx.me_id).await?,
        Command::MarkChannelsRead { channel_ids } => client.mark_channels_read(&channel_ids).await?,
        Command::SetStatus { status, dnd_end_time } => {
            client.set_status(&ctx.me_id, &status, dnd_end_time).await?;
            ctx.emit(Event::DndEnd(if status == "dnd" { dnd_end_time } else { 0 })).await;
        }
        Command::Slash { channel_id, team_id, root_id, command } => {
            client.execute_command(&channel_id, &team_id, &root_id, &command).await?;
        }
        Command::Download { file, open } => {
            let bytes = client.file_content(&file.id).await?;
            // Pour l'ouvrir : dossier temporaire ; sinon : Téléchargements.
            let dir = if open {
                std::env::temp_dir().join("matterfew").join(&file.id)
            } else {
                dirs::download_dir().unwrap_or_else(std::env::temp_dir)
            };
            tokio::fs::create_dir_all(&dir).await?;
            let path = unique_path(&dir, &file.name);
            tokio::fs::write(&path, bytes).await?;
            ctx.emit(Event::Downloaded { path, open }).await;
        }
        Command::Upload { local_id, channel_id, path } => {
            let sent = Arc::new(std::sync::atomic::AtomicU64::new(0));
            let task = {
                let (client, sent) = (client.clone(), sent.clone());
                tokio::spawn(async move { client.upload_file(&channel_id, &path, sent).await })
            };
            ctx.uploads.lock().unwrap_or_else(|e| e.into_inner()).insert(local_id, task.abort_handle());
            // Progression, quatre fois par seconde.
            let ticker = {
                let ctx = ctx.clone();
                tokio::spawn(async move {
                    let mut last = 0;
                    loop {
                        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                        let now = sent.load(Ordering::Relaxed);
                        if now != last {
                            last = now;
                            ctx.emit(Event::UploadProgress { local_id, sent: now }).await;
                        }
                    }
                })
            };
            let result = task.await;
            ticker.abort();
            ctx.uploads.lock().unwrap_or_else(|e| e.into_inner()).remove(&local_id);
            match result {
                Ok(Ok(file)) => ctx.emit(Event::Uploaded { local_id, file }).await,
                Ok(Err(err)) => ctx.emit(Event::UploadFailed { local_id, error: format!("{err:#}") }).await,
                // Annulé depuis l'interface : la pièce jointe est déjà retirée.
                Err(_) => {}
            }
        }
        Command::CancelUpload { local_id } => {
            if let Some(task) = ctx.uploads.lock().unwrap_or_else(|e| e.into_inner()).remove(&local_id) {
                task.abort();
            }
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
        Command::Notify { title, body, target } => {
            tokio::task::spawn_blocking(move || crate::notify::show(&title, &body, target)).await?;
        }
    }
    Ok(())
}

/// État conservé d'une connexion WebSocket à l'autre, pour la reprise.
#[derive(Default)]
struct WsState {
    /// Identifiant de connexion donné par le serveur (événement `hello`).
    connection_id: String,
    /// Numéro du prochain événement attendu.
    next_seq: u64,
    /// Au moins une connexion a abouti.
    connected_once: bool,
}

async fn websocket_loop(ctx: Ctx, mut outgoing: mpsc::UnboundedReceiver<Value>) {
    let mut delay = Duration::from_secs(1);
    let mut state = WsState::default();
    loop {
        match websocket_session(&ctx, &mut outgoing, &mut state).await {
            Ok(()) => delay = Duration::from_secs(1),
            Err(err) => {
                let mut event = Event::error(&err);
                if let Event::Error { message, .. } = &mut event {
                    *message = t!("errors.realtime", error = message).to_string();
                }
                ctx.emit(event).await
            }
        }
        ctx.emit(Event::Connection(false)).await;
        if ctx.ev.is_closed() {
            return;
        }
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(Duration::from_secs(30));
    }
}

/// Une connexion WebSocket. Chaque événement du serveur porte un numéro
/// (`seq`) : un trou dans la suite veut dire qu'on a perdu des événements,
/// on se reconnecte alors en redonnant `connection_id` + numéro attendu et le
/// serveur rejoue ce qui manque. S'il ne peut pas (nouvel identifiant dans
/// `hello`), on signale une resynchronisation complète.
async fn websocket_session(
    ctx: &Ctx,
    outgoing: &mut mpsc::UnboundedReceiver<Value>,
    state: &mut WsState,
) -> Result<()> {
    let url = ctx.client.websocket_resume_url(&state.connection_id, state.next_seq);
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await?;
    let mut last_activity = std::time::Instant::now();

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
                // Plus rien reçu (même pas les réponses aux pings) : connexion morte.
                if last_activity.elapsed() > Duration::from_secs(75) {
                    anyhow::bail!("{}", t!("errors.server_silent"));
                }
                socket.send(Message::Ping(Vec::new().into())).await?;
            }
            Some(mut out) = outgoing.recv() => {
                seq += 1;
                out["seq"] = seq.into();
                socket.send(Message::Text(out.to_string().into())).await?;
            }
            msg = socket.next() => {
                let Some(msg) = msg else { return Ok(()) };
                last_activity = std::time::Instant::now();
                match msg? {
                    Message::Text(text) => {
                        let Ok(value) = serde_json::from_str::<Value>(&text) else { continue };
                        // Réponses à nos propres requêtes : hors numérotation.
                        if value.get("seq_reply").is_some() {
                            continue;
                        }
                        let seq = value.get("seq").and_then(Value::as_u64).unwrap_or(state.next_seq);
                        if value.get("event").and_then(Value::as_str) == Some("hello") {
                            let id = str_field(&value, "/data/connection_id");
                            // Nouvel identifiant (ou serveur sans reprise) après une
                            // première connexion : des événements ont été perdus.
                            let lost = state.connected_once && (id.is_empty() || id != state.connection_id);
                            // Session reprise : la numérotation continue, on n'y touche pas.
                            if id.is_empty() || id != state.connection_id {
                                state.next_seq = seq + 1;
                            }
                            state.connection_id = id;
                            state.connected_once = true;
                            if lost {
                                ctx.emit(Event::Resync).await;
                            }
                        } else if seq != state.next_seq {
                            anyhow::bail!("{}", t!("errors.missed_events", expected = state.next_seq, got = seq));
                        } else {
                            state.next_seq = seq + 1;
                        }
                        on_ws_event(value, ctx).await;
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
    data.get(key).and_then(Value::as_str).and_then(|s| serde_json::from_str(s).ok())
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
            let followers: Vec<String> = nested(&data, "followers").unwrap_or_default();
            Event::Posted {
                post,
                mentions_me: mentions.contains(&ctx.me_id),
                followed: followers.contains(&ctx.me_id),
                channel_type: str_field(&data, "/channel_type"),
                team_id: str_field(&data, "/team_id"),
            }
        }),
        "ephemeral_message" => nested::<Post>(&data, "post").map(|post| Event::Posted {
            post,
            mentions_me: false,
            followed: false,
            channel_type: String::new(),
            team_id: String::new(),
        }),
        "post_edited" => nested::<Post>(&data, "post").map(Event::PostEdited),
        "post_deleted" => nested::<Post>(&data, "post").map(Event::PostDeleted),
        "reaction_added" => {
            nested::<Reaction>(&data, "reaction").map(|r| Event::ReactionAdded(r, reaction_post_id(&data)))
        }
        "reaction_removed" => {
            nested::<Reaction>(&data, "reaction").map(|r| Event::ReactionRemoved(r, reaction_post_id(&data)))
        }
        "typing" => Some(Event::Typing {
            channel_id: str_field(&value, "/broadcast/channel_id"),
            parent_id: str_field(&data, "/parent_id"),
            user_id: str_field(&data, "/user_id"),
        }),
        // Le canal est transmis en JSON dans une chaîne.
        "channel_updated" => data
            .get("channel")
            .and_then(Value::as_str)
            .and_then(|s| serde_json::from_str::<Channel>(s).ok())
            .map(Event::ChannelUpdated),
        "user_added" | "user_removed" => {
            let channel_id = match str_field(&value, "/broadcast/channel_id") {
                c if c.is_empty() => str_field(&data, "/channel_id"),
                c => c,
            };
            (!channel_id.is_empty()).then_some(Event::MembersChanged { channel_id })
        }
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
        // Profil modifié (statut personnalisé, nom, avatar…) : objet JSON direct.
        "user_updated" => {
            data.get("user").and_then(|u| serde_json::from_value::<User>(u.clone()).ok()).map(|u| Event::Users(vec![u]))
        }
        "thread_updated" => nested::<UserThread>(&data, "thread")
            .map(|thread| Event::ThreadUpdated { team_id: str_field(&value, "/broadcast/team_id"), thread }),
        "thread_read_changed" => {
            let thread_id = str_field(&data, "/thread_id");
            let count = |k: &str| data.get(k).and_then(Value::as_i64).unwrap_or(0);
            (!thread_id.is_empty()).then(|| Event::ThreadRead {
                thread_id,
                unread_replies: count("unread_replies"),
                unread_mentions: count("unread_mentions"),
            })
        }
        "thread_follow_changed" => {
            let thread_id = str_field(&data, "/thread_id");
            let following = data.get("state").and_then(Value::as_bool).unwrap_or(false);
            (!thread_id.is_empty()).then_some(Event::ThreadFollow { thread_id, following })
        }
        "channel_bookmark_created"
        | "channel_bookmark_updated"
        | "channel_bookmark_deleted"
        | "channel_bookmark_sorted" => {
            let channel_id = str_field(&value, "/broadcast/channel_id");
            (!channel_id.is_empty()).then_some(Event::BookmarksChanged { channel_id })
        }
        "channel_member_updated" => nested::<ChannelMember>(&data, "channelMember").map(Event::MemberUpdated),
        _ => None,
    };

    if let Some(event) = event {
        match &event {
            Event::Posted { post, .. } => ctx.ensure_users([post.user_id.clone()]).await,
            Event::Typing { user_id, .. } => ctx.ensure_users([user_id.clone()]).await,
            Event::ThreadUpdated { thread, .. } => {
                let mut ids = vec![thread.post.user_id.clone()];
                ids.extend(thread.participants.iter().map(|u| u.id.clone()));
                ctx.ensure_users(ids).await;
            }
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

/// Charge l'image d'un emoji personnalisé en arrière-plan.
fn load_emoji(ctx: &Ctx, id: String, name: String) {
    let ctx = ctx.clone();
    tokio::spawn(async move {
        if let Ok(bytes) = ctx.client.emoji_image(&id).await {
            ctx.emit(Event::CustomEmoji { name, bytes: Arc::new(bytes) }).await;
        }
    });
}

/// Rafraîchit la présence des utilisateurs connus chaque minute.
async fn status_polling(ctx: Ctx) {
    let mut tick = tokio::time::interval(Duration::from_secs(60));
    tick.tick().await;
    loop {
        tick.tick().await;
        if ctx.ev.is_closed() {
            return;
        }
        let ids: Vec<String> = ctx.known_users.lock().await.iter().cloned().collect();
        if let Ok(statuses) = ctx.client.statuses(&ids).await {
            ctx.emit(Event::Statuses(statuses.into_iter().map(|s| (s.user_id, s.status)).collect())).await;
        }
    }
}

/// `dossier/nom.ext`, ou `nom (1).ext`, `nom (2).ext`… si le nom est pris.
fn unique_path(dir: &Path, name: &str) -> PathBuf {
    // Pas de séparateurs de chemin venant du serveur dans le nom.
    let name: String = name.chars().map(|c| if matches!(c, '/' | '\\' | ':') { '_' } else { c }).collect();
    let name = if name.trim().trim_matches('.').is_empty() { t!("files.default_name").to_string() } else { name };
    let candidate = dir.join(&name);
    if !candidate.exists() {
        return candidate;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), format!(".{ext}")),
        _ => (name.clone(), String::new()),
    };
    (1..).map(|n| dir.join(format!("{stem} ({n}){ext}"))).find(|p| !p.exists()).unwrap_or(candidate)
}

#[cfg(test)]
mod tests {
    use super::unique_path;

    #[test]
    fn unique_path_stays_in_dir_and_avoids_collisions() {
        let dir = std::env::temp_dir().join(format!("matterfew-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(unique_path(&dir, ".."), dir.join(&*t!("files.default_name")));
        assert_eq!(unique_path(&dir, "../../x.txt"), dir.join(".._.._x.txt"));
        std::fs::write(dir.join("a.txt"), "").unwrap();
        assert_eq!(unique_path(&dir, "a.txt"), dir.join("a (1).txt"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
