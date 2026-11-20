//! Interface GPUI : état de l'application et réactions aux événements.
//! Le rendu (ce qui est dessiné à l'écran) est dans `view.rs`.

mod view;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::component::message_scroller::MessageScrollerState;
use gpui_kit::*;

use crate::api::{Channel, FileInfo, Post, Preference, Reaction, SidebarCategory, Team, User};
use crate::backend::{self, Command, Event, PAGE_SIZE};
use crate::sidebar::{ChannelState, SidebarPrefs};
use crate::theme::{MattermostTheme, Palette};
use crate::api::{LoginError, login_with_password};
use crate::credentials::AuthKind;
use crate::{credentials, demo, markdown, sso};

/// Un message et son Markdown déjà préparé (emojis, mentions).
struct PostView {
    post: Post,
    md: SharedString,
}

impl PostView {
    fn new(post: Post) -> Self {
        let md = markdown::prepare(&post.message).into();
        Self { post, md }
    }
}

/// Fichier en cours d'envoi ou prêt à être joint au prochain message.
struct Attachment {
    local_id: u64,
    pane: Pane,
    name: String,
    size: i64,
    /// Identifiant côté serveur, une fois l'envoi terminé.
    file_id: Option<String>,
    error: Option<String>,
}

struct ThreadView {
    root_id: String,
    posts: Vec<PostView>,
}

struct Typing {
    channel_id: String,
    parent_id: String,
    user_id: String,
    at: Instant,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Screen {
    Login,
    Connecting,
    Chat,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pane {
    Channel,
    Thread,
}

/// Délai entre deux signaux « en train d'écrire » envoyés au serveur.
const TYPING_SEND_INTERVAL: Duration = Duration::from_secs(3);
/// Durée d'affichage d'un « X est en train d'écrire » sans nouveau signal.
const TYPING_DISPLAY: Duration = Duration::from_secs(5);

pub struct ChatApp {
    screen: Screen,
    url_input: Entity<InputState>,
    token_input: Entity<InputState>,
    email_input: Entity<InputState>,
    password_input: Entity<InputState>,
    mfa_input: Entity<InputState>,
    /// Le compte demande un code de double authentification.
    show_mfa: bool,
    /// Formulaire « jeton d'accès personnel » déplié.
    show_token: bool,
    /// Étape de connexion en cours (navigateur, mot de passe…), pour l'affichage.
    login_busy: Option<&'static str>,
    commands: Option<async_channel::Sender<Command>>,
    /// Identifiants en attente d'enregistrement (après connexion réussie).
    pending_credentials: Option<(String, String, AuthKind)>,
    server_url: Option<String>,
    /// Jeton et type de la connexion active (pour fermer la session).
    active_auth: Option<(String, AuthKind)>,

    me: Option<User>,
    teams: Vec<Team>,
    team_id: Option<String>,
    channels: HashMap<String, ChannelState>,
    categories: Vec<SidebarCategory>,
    raw_prefs: Vec<Preference>,
    prefs: SidebarPrefs,
    palette: Palette,
    demo: bool,
    sticky_unread: Option<String>,
    users: HashMap<String, User>,
    avatars: HashMap<String, Arc<Image>>,
    team_icons: HashMap<String, Arc<Image>>,
    /// Équipe → (non lus, mentions), pour la barre des équipes.
    team_unreads: HashMap<String, (bool, i64)>,
    /// Aperçus des images jointes, et ceux déjà demandés.
    previews: HashMap<String, Arc<Image>>,
    requested_previews: HashSet<String>,
    attachments: Vec<Attachment>,
    next_local_id: u64,
    /// Message d'information temporaire (ex. fichier téléchargé).
    info: Option<String>,
    statuses: HashMap<String, String>,
    typing: Vec<Typing>,
    last_typing_sent: Option<Instant>,

    channel_id: Option<String>,
    posts: Vec<PostView>,
    loading_posts: bool,
    has_more: bool,
    loading_older: bool,
    scroller: Entity<MessageScrollerState>,
    composer: Entity<TextareaState>,

    thread: Option<ThreadView>,
    thread_scroller: Entity<MessageScrollerState>,
    thread_composer: Entity<TextareaState>,

    /// Message en cours de modification, et dans quel panneau.
    editing: Option<(Pane, String)>,
    /// Message dont la suppression attend confirmation.
    confirm_delete: Option<String>,

    connected: bool,
    was_connected: bool,
    error: Option<String>,

    _subscriptions: Vec<Subscription>,
    _event_task: Option<Task<()>>,
}

impl ChatApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Ordre de priorité : variables d'environnement, puis connexion mémorisée.
        let env_url = std::env::var("MM_URL").unwrap_or_default();
        let env_token = std::env::var("MM_TOKEN").unwrap_or_default();
        let auto = if !env_url.is_empty() && !env_token.is_empty() {
            Some((env_url.clone(), env_token, AuthKind::Token))
        } else {
            credentials::load().map(|s| (s.server_url, s.token, s.kind))
        };
        let initial_url = auto
            .as_ref()
            .map(|(u, ..)| u.clone())
            .or_else(credentials::last_server)
            .unwrap_or(env_url);

        let url_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("https://chat.exemple.org")
                .default_value(initial_url)
        });
        let token_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Jeton d'accès personnel")
                .masked(true)
        });
        let email_input = cx.new(|cx| InputState::new(window, cx).placeholder("prenom.nom@exemple.org"));
        let password_input = cx.new(|cx| InputState::new(window, cx).placeholder("Mot de passe").masked(true));
        let mfa_input = cx.new(|cx| InputState::new(window, cx).placeholder("Code à 6 chiffres"));
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 8)
                .submit_on_enter(true)
                .placeholder("Écrire un message…   (Maj+Entrée : nouvelle ligne)")
        });
        let thread_composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 6)
                .submit_on_enter(true)
                .placeholder("Répondre dans le fil…")
        });
        let scroller = cx.new(|cx| MessageScrollerState::new(0, cx));
        let thread_scroller = cx.new(|cx| MessageScrollerState::new(0, cx));

        let composer_events = |pane: Pane| {
            move |this: &mut Self, _: &Entity<TextareaState>, ev: &InputEvent, window: &mut Window, cx: &mut Context<Self>| {
                match ev {
                    InputEvent::PressEnter { .. } => this.send_from(pane, window, cx),
                    InputEvent::Change => this.signal_typing(pane),
                    _ => {}
                }
            }
        };

        let subscriptions = vec![
            cx.subscribe_in(&composer, window, composer_events(Pane::Channel)),
            cx.subscribe_in(&thread_composer, window, composer_events(Pane::Thread)),
            cx.subscribe_in(&token_input, window, |this, _, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.connect(window, cx);
                }
            }),
            cx.subscribe_in(&password_input, window, |this, _, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.login_password(window, cx);
                }
            }),
            cx.subscribe_in(&mfa_input, window, |this, _, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.login_password(window, cx);
                }
            }),
            // Retour sur la fenêtre : le canal affiché devient « lu ».
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.mark_current_read();
                    cx.notify();
                }
            }),
            // Défilement vers le haut : chargement des messages plus anciens.
            cx.observe(&scroller, |this, scroller, cx| this.maybe_load_older(&scroller, cx)),
        ];

        let mut this = Self {
            screen: Screen::Login,
            url_input,
            token_input,
            email_input,
            password_input,
            mfa_input,
            show_mfa: false,
            show_token: false,
            login_busy: None,
            commands: None,
            pending_credentials: None,
            server_url: None,
            active_auth: None,
            me: None,
            teams: Vec::new(),
            team_id: None,
            channels: HashMap::new(),
            categories: Vec::new(),
            raw_prefs: Vec::new(),
            prefs: SidebarPrefs::default(),
            palette: Palette::from_theme(&MattermostTheme::default()),
            demo: false,
            sticky_unread: None,
            users: HashMap::new(),
            avatars: HashMap::new(),
            team_icons: HashMap::new(),
            team_unreads: HashMap::new(),
            previews: HashMap::new(),
            requested_previews: HashSet::new(),
            attachments: Vec::new(),
            next_local_id: 0,
            info: None,
            statuses: HashMap::new(),
            typing: Vec::new(),
            last_typing_sent: None,
            channel_id: None,
            posts: Vec::new(),
            loading_posts: false,
            has_more: false,
            loading_older: false,
            scroller,
            composer,
            thread: None,
            thread_scroller,
            thread_composer,
            editing: None,
            confirm_delete: None,
            connected: false,
            was_connected: false,
            error: None,
            _subscriptions: subscriptions,
            _event_task: None,
        };
        this.palette.apply(cx);
        if std::env::args().any(|a| a == "--demo") {
            this.start_demo(window, cx);
        } else if let Some((url, token, kind)) = auto.filter(|_| !std::env::args().any(|a| a == "--login")) {
            this.connect_with(url, token, kind, window, cx);
        }
        this
    }

    /// Mode démo : faux serveur local, rien n'est enregistré.
    fn start_demo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo = true;
        self.screen = Screen::Connecting;
        let backend = demo::start();
        self.commands = Some(backend.commands);
        self.listen(backend.events, window, cx);
    }

    /// Relaie les événements du backend vers `on_event`.
    fn listen(&mut self, events: async_channel::Receiver<Event>, window: &mut Window, cx: &mut Context<Self>) {
        self._event_task = Some(cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = events.recv().await {
                let alive = this.update_in(cx, |this, window, cx| this.on_event(event, window, cx));
                if alive.is_err() {
                    break;
                }
            }
        }));
    }

    /// Recalcule réglages de barre latérale et thème depuis les préférences.
    fn refresh_preferences(&mut self, cx: &mut Context<Self>) {
        self.prefs = SidebarPrefs::from_preferences(&self.raw_prefs);
        let theme = MattermostTheme::from_preferences(&self.raw_prefs, self.team_id.as_deref());
        self.palette = Palette::from_theme(&theme);
        self.palette.apply(cx);
    }

    // ------------------------------------------------------------------
    // Connexion
    // ------------------------------------------------------------------

    fn send(&self, cmd: Command) {
        if let Some(tx) = &self.commands {
            let _ = tx.try_send(cmd);
        }
    }

    /// Adresse du serveur saisie, normalisée (`https://` ajouté si besoin).
    fn server_input(&mut self, cx: &mut Context<Self>) -> Option<String> {
        let url = self.url_input.read(cx).value().trim().trim_end_matches('/').to_string();
        if url.is_empty() {
            self.error = Some("Indiquez l'adresse du serveur Mattermost.".into());
            cx.notify();
            return None;
        }
        Some(if url.starts_with("http://") || url.starts_with("https://") { url } else { format!("https://{url}") })
    }

    /// Connexion avec un jeton d'accès personnel.
    fn connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(url) = self.server_input(cx) else { return };
        let token = self.token_input.read(cx).value().to_string();
        self.connect_with(url, token, AuthKind::Token, window, cx);
    }

    /// Connexion par la vraie page de connexion du serveur, dans une
    /// fenêtre de navigateur intégré (GitLab, mot de passe, double authentification…).
    fn login_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.login_busy.is_some() {
            return;
        }
        let Some(url) = self.server_input(cx) else { return };
        self.login_busy = Some("browser");
        self.error = None;
        let result = in_background({
            let url = url.clone();
            move || sso::login_in_browser(&url).map_err(|e| e.to_string())
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = result.recv().await.unwrap_or_else(|_| Err("Connexion interrompue.".into()));
            let _ = this.update_in(cx, |this, window, cx| {
                this.login_busy = None;
                match result {
                    Ok(token) => this.connect_with(url, token, AuthKind::Session, window, cx),
                    Err(err) => {
                        this.error = Some(err);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Connexion par e-mail et mot de passe (+ code de double authentification).
    fn login_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.login_busy.is_some() {
            return;
        }
        let Some(url) = self.server_input(cx) else { return };
        let email = self.email_input.read(cx).value().trim().to_string();
        let password = self.password_input.read(cx).value().to_string();
        let mfa = if self.show_mfa { self.mfa_input.read(cx).value().trim().to_string() } else { String::new() };
        if email.is_empty() || password.is_empty() {
            self.error = Some("Indiquez votre e-mail et votre mot de passe.".into());
            cx.notify();
            return;
        }
        self.login_busy = Some("password");
        self.error = None;
        let result = in_background({
            let url = url.clone();
            move || {
                let rt = tokio::runtime::Builder::new_current_thread().enable_all().build();
                match rt {
                    Ok(rt) => rt.block_on(login_with_password(&url, &email, &password, &mfa)),
                    Err(e) => Err(LoginError::Other(e.to_string())),
                }
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = result.recv().await.unwrap_or_else(|_| Err(LoginError::Other("Connexion interrompue.".into())));
            let _ = this.update_in(cx, |this, window, cx| {
                this.login_busy = None;
                match result {
                    Ok(token) => {
                        this.password_input.update(cx, |s, cx| s.set_value("", window, cx));
                        this.mfa_input.update(cx, |s, cx| s.set_value("", window, cx));
                        this.show_mfa = false;
                        this.connect_with(url, token, AuthKind::Session, window, cx);
                    }
                    Err(LoginError::MfaRequired) => {
                        this.show_mfa = true;
                        this.error = Some("Saisissez le code de votre application d'authentification.".into());
                        this.mfa_input.update(cx, |s, cx| s.focus(window, cx));
                        cx.notify();
                    }
                    Err(LoginError::Other(err)) => {
                        this.error = Some(err);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn connect_with(&mut self, url: String, token: String, kind: AuthKind, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen == Screen::Connecting {
            return;
        }
        let token = token.trim().to_string();
        if token.is_empty() {
            self.error = Some("Le jeton est vide.".into());
            cx.notify();
            return;
        }

        let backend = backend::start(url.clone(), token.clone());
        self.commands = Some(backend.commands);
        self.pending_credentials = Some((url.clone(), token.clone(), kind));
        self.active_auth = Some((token, kind));
        self.server_url = Some(url);
        self.screen = Screen::Connecting;
        self.error = None;

        self.listen(backend.events, window, cx);
        cx.notify();
    }

    /// Oublie le jeton mémorisé et revient à l'écran de connexion.
    /// Lâcher le canal des commandes arrête le backend et son WebSocket.
    fn logout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let (Some(url), false) = (self.server_url.clone(), self.demo) {
            credentials::forget(&url);
            // Une session (navigateur, mot de passe) est aussi fermée côté
            // serveur, et le navigateur intégré oublie la sienne.
            if let Some((token, AuthKind::Session)) = self.active_auth.take() {
                std::thread::spawn(move || {
                    if let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() {
                        rt.block_on(crate::api::logout(&url, &token));
                    }
                });
                sso::clear_webview_data();
            }
        }
        self.active_auth = None;
        self.demo = false;
        self.commands = None;
        self._event_task = None;
        self.pending_credentials = None;
        self.screen = Screen::Login;
        self.me = None;
        self.teams.clear();
        self.team_id = None;
        self.channels.clear();
        self.categories.clear();
        self.users.clear();
        self.avatars.clear();
        self.statuses.clear();
        self.typing.clear();
        self.channel_id = None;
        self.posts.clear();
        self.thread = None;
        self.editing = None;
        self.confirm_delete = None;
        self.connected = false;
        self.was_connected = false;
        self.error = None;
        self.scroller.update(cx, |s, cx| s.reset(0, cx));
        self.token_input.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Navigation
    // ------------------------------------------------------------------

    fn select_team(&mut self, team_id: String, cx: &mut Context<Self>) {
        if self.team_id.as_deref() == Some(&team_id) {
            return;
        }
        self.team_id = Some(team_id.clone());
        self.attachments.clear();
        self.send(Command::LoadTeamUnreads);
        // Le thème peut être propre à l'équipe.
        self.refresh_preferences(cx);
        self.channels.clear();
        self.categories.clear();
        self.channel_id = None;
        self.posts.clear();
        self.thread = None;
        self.editing = None;
        self.scroller.update(cx, |s, cx| s.reset(0, cx));
        self.send(Command::LoadTeam { team_id });
        cx.notify();
    }

    fn select_channel(&mut self, channel_id: String, cx: &mut Context<Self>) {
        if self.channel_id.as_deref() == Some(&channel_id) {
            return;
        }
        // Un canal non lu reste dans « Non lus » tant qu'on ne le quitte pas.
        self.sticky_unread = self
            .channels
            .get(&channel_id)
            .filter(|c| c.shows_unread())
            .map(|_| channel_id.clone());
        if let Some(state) = self.channels.get_mut(&channel_id) {
            state.unread_messages = false;
            state.mentions = 0;
        }
        self.channel_id = Some(channel_id.clone());
        self.attachments.clear();
        self.posts.clear();
        self.loading_posts = true;
        self.has_more = false;
        self.loading_older = false;
        self.thread = None;
        self.editing = None;
        self.confirm_delete = None;
        self.scroller.update(cx, |s, cx| s.reset(0, cx));
        self.send(Command::LoadChannel { channel_id: channel_id.clone() });
        self.send(Command::MarkRead { channel_id });
        cx.notify();
    }

    fn mark_current_read(&mut self) {
        if let Some(id) = self.channel_id.clone() {
            if let Some(state) = self.channels.get_mut(&id) {
                state.unread_messages = false;
                state.mentions = 0;
            }
            self.send(Command::MarkRead { channel_id: id });
        }
    }

    fn toggle_category(&mut self, category_id: &str, cx: &mut Context<Self>) {
        let Some(team_id) = self.team_id.clone() else { return };
        let Some(category) = self.categories.iter_mut().find(|c| c.id == category_id) else { return };
        category.collapsed = !category.collapsed;
        let category = category.clone();
        self.send(Command::SetCategoryCollapsed { team_id, category });
        cx.notify();
    }

    fn maybe_load_older(&mut self, scroller: &Entity<MessageScrollerState>, cx: &mut Context<Self>) {
        if !self.has_more || self.loading_older || self.posts.is_empty() {
            return;
        }
        let state = scroller.read(cx);
        // Seulement quand l'utilisateur a lui-même remonté jusqu'en haut.
        if state.is_scrolled_up() && state.logical_scroll_top().item_ix <= 2 {
            if let Some(channel_id) = self.channel_id.clone() {
                self.loading_older = true;
                let before = self.posts[0].post.id.clone();
                self.send(Command::LoadOlder { channel_id, before });
                cx.notify();
            }
        }
    }

    fn open_thread(&mut self, root_id: String, window: &mut Window, cx: &mut Context<Self>) {
        // Affiche tout de suite le message racine, le reste arrive ensuite.
        let seed: Vec<PostView> = self
            .posts
            .iter()
            .find(|p| p.post.id == root_id)
            .map(|p| PostView::new(p.post.clone()))
            .into_iter()
            .collect();
        let count = seed.len();
        if self.editing.as_ref().is_some_and(|(pane, _)| *pane == Pane::Thread) {
            self.editing = None;
        }
        self.thread = Some(ThreadView { root_id: root_id.clone(), posts: seed });
        self.thread_scroller.update(cx, |s, cx| s.reset(count, cx));
        self.send(Command::LoadThread { root_id });
        self.thread_composer.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    fn close_thread(&mut self, cx: &mut Context<Self>) {
        self.thread = None;
        if self.editing.as_ref().is_some_and(|(pane, _)| *pane == Pane::Thread) {
            self.editing = None;
        }
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Saisie, modification, suppression, réactions
    // ------------------------------------------------------------------

    fn composer_for(&self, pane: Pane) -> Entity<TextareaState> {
        match pane {
            Pane::Channel => self.composer.clone(),
            Pane::Thread => self.thread_composer.clone(),
        }
    }

    fn send_from(&mut self, pane: Pane, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.composer_for(pane);
        let message = state.read(cx).value().trim().to_string();

        // Mode modification : on remplace le message existant.
        if let Some((_, post_id)) = self.editing.clone().filter(|(p, _)| *p == pane) {
            if !message.is_empty() {
                self.send(Command::Edit { post_id, message });
            }
            self.editing = None;
            state.update(cx, |s, cx| s.set_value("", window, cx));
            cx.notify();
            return;
        }

        // Pièces jointes : on attend la fin de leur envoi.
        let pending: Vec<&Attachment> = self.attachments.iter().filter(|a| a.pane == pane).collect();
        if pending.iter().any(|a| a.file_id.is_none() && a.error.is_none()) {
            self.info = Some("Envoi des fichiers en cours, patientez un instant…".into());
            cx.notify();
            return;
        }
        let file_ids: Vec<String> = pending.iter().filter_map(|a| a.file_id.clone()).collect();
        if message.is_empty() && file_ids.is_empty() {
            return;
        }
        let Some(channel_id) = self.channel_id.clone() else { return };
        let root_id = match (pane, &self.thread) {
            (Pane::Channel, _) => String::new(),
            (Pane::Thread, Some(thread)) => thread.root_id.clone(),
            (Pane::Thread, None) => return,
        };
        self.send(Command::Send { channel_id, root_id, message, file_ids });
        self.attachments.retain(|a| a.pane != pane);
        self.last_typing_sent = None;
        state.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Fichiers
    // ------------------------------------------------------------------

    /// Envoie des fichiers locaux ; ils seront joints au prochain message.
    fn add_files(&mut self, pane: Pane, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let Some(channel_id) = self.channel_id.clone() else { return };
        for path in paths.into_iter().filter(|p| p.is_file()) {
            self.next_local_id += 1;
            let local_id = self.next_local_id;
            self.attachments.push(Attachment {
                local_id,
                pane,
                name: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                size: std::fs::metadata(&path).map(|m| m.len() as i64).unwrap_or(0),
                file_id: None,
                error: None,
            });
            self.send(Command::Upload { local_id, channel_id: channel_id.clone(), path });
        }
        cx.notify();
    }

    /// Ouvre le sélecteur de fichiers du système.
    fn pick_files(&mut self, pane: Pane, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Joindre".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await {
                let _ = this.update(cx, |this, cx| this.add_files(pane, paths, cx));
            }
        })
        .detach();
    }

    fn remove_attachment(&mut self, local_id: u64, cx: &mut Context<Self>) {
        self.attachments.retain(|a| a.local_id != local_id);
        cx.notify();
    }

    /// Télécharge un fichier joint ; `open` : l'ouvrir avec l'application
    /// par défaut, sinon le ranger dans Téléchargements.
    fn download(&mut self, file: FileInfo, open: bool, cx: &mut Context<Self>) {
        self.info = Some(format!("Téléchargement de {}…", file.name));
        self.send(Command::Download { file, open });
        cx.notify();
    }

    /// Demande les aperçus des images jointes pas encore chargées.
    fn request_previews<'a>(&mut self, posts: impl IntoIterator<Item = &'a Post>) {
        let wanted: Vec<String> = posts
            .into_iter()
            .flat_map(|p| p.metadata.files.iter())
            .filter(|f| f.is_image() && !self.requested_previews.contains(&f.id))
            .map(|f| f.id.clone())
            .collect();
        for file_id in wanted {
            self.requested_previews.insert(file_id.clone());
            self.send(Command::LoadPreview { file_id });
        }
    }

    /// Affiche une information quelques secondes.
    fn flash(&mut self, text: String, cx: &mut Context<Self>) {
        self.info = Some(text.clone());
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(5)).await;
            let _ = this.update(cx, |this, cx| {
                if this.info.as_deref() == Some(text.as_str()) {
                    this.info = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn start_edit(&mut self, pane: Pane, post_id: String, window: &mut Window, cx: &mut Context<Self>) {
        let list = match pane {
            Pane::Channel => &self.posts,
            Pane::Thread => match &self.thread {
                Some(t) => &t.posts,
                None => return,
            },
        };
        let Some(text) = list.iter().find(|p| p.post.id == post_id).map(|p| p.post.message.clone()) else {
            return;
        };
        self.editing = Some((pane, post_id));
        let state = self.composer_for(pane);
        state.update(cx, |s, cx| {
            s.set_value(text, window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((pane, _)) = self.editing.take() {
            self.composer_for(pane).update(cx, |s, cx| s.set_value("", window, cx));
            cx.notify();
        }
    }

    fn delete_post(&mut self, post_id: String, cx: &mut Context<Self>) {
        self.confirm_delete = None;
        self.send(Command::Delete { post_id });
        cx.notify();
    }

    fn toggle_reaction(&mut self, post_id: String, emoji: String) {
        let me = self.me.as_ref().map(|u| u.id.as_str()).unwrap_or_default();
        let mine = self
            .posts
            .iter()
            .chain(self.thread.iter().flat_map(|t| t.posts.iter()))
            .find(|p| p.post.id == post_id)
            .is_some_and(|p| {
                p.post.metadata.reactions.iter().any(|r| r.user_id == me && r.emoji_name == emoji)
            });
        self.send(Command::React { post_id, emoji, add: !mine });
    }

    /// Signale au serveur que l'on écrit (au plus toutes les 3 secondes).
    fn signal_typing(&mut self, pane: Pane) {
        if self.editing.is_some() {
            return;
        }
        if self.last_typing_sent.is_some_and(|t| t.elapsed() < TYPING_SEND_INTERVAL) {
            return;
        }
        let Some(channel_id) = self.channel_id.clone() else { return };
        let parent_id = match pane {
            Pane::Channel => String::new(),
            Pane::Thread => match &self.thread {
                Some(t) => t.root_id.clone(),
                None => return,
            },
        };
        self.last_typing_sent = Some(Instant::now());
        self.send(Command::Typing { channel_id, parent_id });
    }

    // ------------------------------------------------------------------
    // Événements du backend
    // ------------------------------------------------------------------

    fn on_event(&mut self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            Event::Ready { me, teams } => {
                if let Some((url, token, kind)) = self.pending_credentials.take() {
                    if let Err(err) = credentials::save(&url, &token, kind) {
                        eprintln!("connexion non mémorisée : {err:#}");
                    }
                }
                self.users.insert(me.id.clone(), me.clone());
                self.me = Some(me);
                self.teams = teams;
                self.screen = Screen::Chat;
                if let Some(team) = self.teams.first() {
                    let id = team.id.clone();
                    self.select_team(id, cx);
                }
            }
            Event::Users(users) => {
                for user in users {
                    self.users.insert(user.id.clone(), user);
                }
            }
            Event::TeamIcon { team_id, bytes } => {
                if let Some(format) = image_format(&bytes) {
                    self.team_icons.insert(team_id, Arc::new(Image::from_bytes(format, bytes.to_vec())));
                }
            }
            Event::TeamUnreads(list) => {
                self.team_unreads = list
                    .into_iter()
                    .map(|t| {
                        let (msgs, mentions) = if t.msg_count_root > 0 || t.mention_count_root > 0 {
                            (t.msg_count_root, t.mention_count_root)
                        } else {
                            (t.msg_count, t.mention_count)
                        };
                        (t.team_id, (msgs > 0, mentions))
                    })
                    .collect();
            }
            Event::FilePreview { file_id, bytes } => {
                if let Some(format) = image_format(&bytes) {
                    self.previews.insert(file_id, Arc::new(Image::from_bytes(format, bytes.to_vec())));
                    // Les messages concernés changent de hauteur.
                    self.scroller.update(cx, |s, cx| s.remeasure(cx));
                    self.thread_scroller.update(cx, |s, cx| s.remeasure(cx));
                }
            }
            Event::Downloaded { path, open } => {
                if open {
                    cx.open_with_system(&path);
                    self.info = None;
                } else {
                    cx.reveal_path(&path);
                    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    self.flash(format!("Enregistré dans Téléchargements : {name}"), cx);
                }
            }
            Event::Uploaded { local_id, file } => {
                if let Some(a) = self.attachments.iter_mut().find(|a| a.local_id == local_id) {
                    a.file_id = Some(file.id);
                }
            }
            Event::UploadFailed { local_id, error } => {
                if let Some(a) = self.attachments.iter_mut().find(|a| a.local_id == local_id) {
                    a.error = Some(error);
                }
            }
            Event::Avatar { user_id, bytes } => {
                if let Some(format) = image_format(&bytes) {
                    self.avatars
                        .insert(user_id, Arc::new(Image::from_bytes(format, bytes.to_vec())));
                }
            }
            Event::Statuses(list) => self.statuses.extend(list),
            Event::Channels { team_id, channels, members } => {
                if self.team_id.as_deref() != Some(&team_id) {
                    return;
                }
                let members: HashMap<_, _> = members.into_iter().map(|m| (m.channel_id.clone(), m)).collect();
                self.channels = channels
                    .into_iter()
                    .filter(|c| c.delete_at == 0)
                    .map(|channel| {
                        let state = match members.get(&channel.id) {
                            Some(m) => ChannelState {
                                unread_messages: channel.total_msg_count_root > m.msg_count_root,
                                mentions: m.mention_count_root,
                                muted: m.is_muted(),
                                channel,
                            },
                            None => ChannelState { channel, unread_messages: false, mentions: 0, muted: false },
                        };
                        (state.channel.id.clone(), state)
                    })
                    .collect();
                // Le canal affiché reste lu.
                if let Some(state) = self.channel_id.as_ref().and_then(|id| self.channels.get_mut(id)) {
                    state.unread_messages = false;
                    state.mentions = 0;
                }
                let current_ok = self.channel_id.as_ref().is_some_and(|id| self.channels.contains_key(id));
                if !current_ok {
                    let default = self
                        .channels
                        .values()
                        .find(|c| c.channel.name == "town-square")
                        .or_else(|| self.channels.values().next())
                        .map(|c| c.channel.id.clone());
                    if let Some(id) = default {
                        self.select_channel(id, cx);
                    }
                }
            }
            Event::Categories { team_id, categories } => {
                if self.team_id.as_deref() == Some(&team_id) {
                    self.categories = categories;
                }
            }
            Event::CategoriesChanged { team_id } => {
                if let Some(current) = self.team_id.clone() {
                    if team_id.is_empty() || team_id == current {
                        self.send(Command::LoadCategories { team_id: current });
                    }
                }
            }
            Event::Prefs(prefs) => {
                self.raw_prefs = prefs;
                self.refresh_preferences(cx);
            }
            Event::PrefsChanged => self.send(Command::LoadPreferences),
            Event::ChannelAdded(channel) => {
                self.channels.entry(channel.id.clone()).or_insert(ChannelState {
                    channel,
                    unread_messages: true,
                    mentions: 1,
                    muted: false,
                });
                // La catégorie « Messages directs » côté serveur l'inclut désormais.
                if let Some(team_id) = self.team_id.clone() {
                    self.send(Command::LoadCategories { team_id });
                }
            }
            Event::ChannelPosts { channel_id, posts } => {
                if self.channel_id.as_deref() == Some(&channel_id) {
                    self.has_more = posts.len() >= PAGE_SIZE;
                    self.request_previews(&posts);
                    self.posts = posts.into_iter().filter(|p| p.delete_at == 0).map(PostView::new).collect();
                    self.loading_posts = false;
                    let n = self.posts.len();
                    self.scroller.update(cx, |s, cx| s.reset(n, cx));
                }
            }
            Event::OlderPosts { channel_id, posts } => {
                if self.channel_id.as_deref() == Some(&channel_id) {
                    self.loading_older = false;
                    self.has_more = posts.len() >= PAGE_SIZE;
                    self.request_previews(&posts);
                    let older: Vec<PostView> = posts
                        .into_iter()
                        .filter(|p| p.delete_at == 0 && !self.posts.iter().any(|q| q.post.id == p.id))
                        .map(PostView::new)
                        .collect();
                    let n = older.len();
                    if n > 0 {
                        self.posts.splice(0..0, older);
                        self.scroller.update(cx, |s, cx| s.prepend(n, cx));
                    }
                }
            }
            Event::Thread { root_id, posts } => {
                self.request_previews(&posts);
                if let Some(thread) = self.thread.as_mut().filter(|t| t.root_id == root_id) {
                    thread.posts = posts.into_iter().filter(|p| p.delete_at == 0).map(PostView::new).collect();
                    let n = thread.posts.len();
                    self.thread_scroller.update(cx, |s, cx| s.reset(n, cx));
                }
            }
            Event::Posted { post, mentions_me, channel_type } => {
                self.on_posted(post, mentions_me, &channel_type, window, cx);
            }
            Event::PostEdited(post) => {
                self.request_previews([&post]);
                self.replace_post(post, cx);
            }
            Event::PostDeleted(post) => self.remove_post(&post.id, cx),
            Event::ReactionAdded(reaction, post_id) => {
                self.update_reactions(&post_id, cx, |list| {
                    if !list.iter().any(|r| r.user_id == reaction.user_id && r.emoji_name == reaction.emoji_name) {
                        list.push(reaction.clone());
                    }
                });
            }
            Event::ReactionRemoved(reaction, post_id) => {
                self.update_reactions(&post_id, cx, |list| {
                    list.retain(|r| !(r.user_id == reaction.user_id && r.emoji_name == reaction.emoji_name));
                });
            }
            Event::Typing { channel_id, parent_id, user_id } => {
                let me = self.me.as_ref().map(|u| u.id.as_str()).unwrap_or_default();
                if user_id.is_empty() || user_id == me {
                    return;
                }
                self.typing.retain(|t| !(t.user_id == user_id && t.channel_id == channel_id && t.parent_id == parent_id));
                self.typing.push(Typing { channel_id, parent_id, user_id, at: Instant::now() });
                // Efface l'indication si aucun nouveau signal n'arrive.
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(TYPING_DISPLAY + Duration::from_millis(200)).await;
                    let _ = this.update(cx, |this, cx| {
                        this.typing.retain(|t| t.at.elapsed() < TYPING_DISPLAY);
                        cx.notify();
                    });
                })
                .detach();
            }
            Event::Connection(up) => {
                let reconnected = up && !self.connected && self.was_connected;
                self.connected = up;
                self.was_connected |= up;
                // Après une coupure, on recharge pour récupérer ce qui a été manqué.
                if reconnected {
                    if let Some(team_id) = self.team_id.clone() {
                        self.send(Command::LoadTeam { team_id });
                    }
                    if let Some(channel_id) = self.channel_id.clone() {
                        self.send(Command::LoadChannel { channel_id });
                    }
                    self.send(Command::LoadTeamUnreads);
                }
            }
            Event::Error(message) => {
                eprintln!("erreur : {message}");
                if self.screen == Screen::Connecting {
                    self.screen = Screen::Login;
                    self.commands = None;
                    self._event_task = None;
                    self.pending_credentials = None;
                }
                self.error = Some(message);
            }
        }
        cx.notify();
    }

    fn on_posted(&mut self, post: Post, mentions_me: bool, channel_type: &str, window: &mut Window, cx: &mut Context<Self>) {
        let me_id = self.me.as_ref().map(|u| u.id.clone()).unwrap_or_default();
        let from_me = post.user_id == me_id;
        let is_current = self.channel_id.as_deref() == Some(&post.channel_id);
        let active = window.is_window_active();

        // Un message envoyé efface son « en train d'écrire ».
        self.typing.retain(|t| !(t.user_id == post.user_id && t.channel_id == post.channel_id));

        self.request_previews([&post]);

        // Nouveau message direct ou de groupe : le canal apparaît dans la liste.
        // Message dans un canal d'une autre équipe : pastilles des équipes à jour.
        if !self.channels.contains_key(&post.channel_id) {
            if channel_type == "D" || channel_type == "G" {
                self.send(Command::FetchChannel { channel_id: post.channel_id.clone() });
            } else if !from_me {
                self.send(Command::LoadTeamUnreads);
            }
        }

        if post.is_reply() {
            if let Some(ix) = self.posts.iter().position(|p| p.post.id == post.root_id) {
                self.posts[ix].post.reply_count += 1;
                self.scroller.update(cx, |s, cx| s.remeasure_items(ix..ix + 1, cx));
            }
            if let Some(thread) = self.thread.as_mut().filter(|t| t.root_id == post.root_id) {
                if !thread.posts.iter().any(|p| p.post.id == post.id) {
                    thread.posts.push(PostView::new(post.clone()));
                    self.thread_scroller.update(cx, |s, cx| s.append(1, cx));
                }
            }
        } else if is_current && !self.posts.iter().any(|p| p.post.id == post.id) {
            self.posts.push(PostView::new(post.clone()));
            self.scroller.update(cx, |s, cx| s.append(1, cx));
        }

        let seen = is_current && active;
        if let Some(state) = self.channels.get_mut(&post.channel_id) {
            // Tri « récents » : activité du canal.
            state.channel.last_post_at = post.create_at;
            if !post.is_reply() {
                state.channel.last_root_post_at = post.create_at;
            }
            if !seen && !from_me {
                // Une réponse dans un fil ne compte que si elle nous mentionne.
                if !post.is_reply() {
                    state.unread_messages = true;
                }
                if mentions_me || channel_type == "D" {
                    state.mentions += 1;
                }
            }
        }
        if seen {
            self.send(Command::MarkRead { channel_id: post.channel_id.clone() });
        }

        let notify = !from_me && !seen && (mentions_me || channel_type == "D");
        if notify {
            let sender = self.user_name(&post.user_id);
            let title = match self.channels.get(&post.channel_id) {
                Some(state) if !state.channel.is_direct() => {
                    format!("{sender} dans {}", self.channel_title(&state.channel))
                }
                _ => sender,
            };
            let body = markdown::plain_text(&post.message);
            self.send(Command::Notify { title, body });
        }
    }

    fn replace_post(&mut self, post: Post, cx: &mut Context<Self>) {
        if let Some(ix) = self.posts.iter().position(|p| p.post.id == post.id) {
            // Le message édité arrive sans compteur de réponses : on garde l'ancien.
            let reply_count = self.posts[ix].post.reply_count;
            self.posts[ix] = PostView::new(Post { reply_count, ..post.clone() });
            self.scroller.update(cx, |s, cx| s.remeasure_items(ix..ix + 1, cx));
        }
        if let Some(thread) = self.thread.as_mut() {
            if let Some(ix) = thread.posts.iter().position(|p| p.post.id == post.id) {
                thread.posts[ix] = PostView::new(post);
                self.thread_scroller.update(cx, |s, cx| s.remeasure_items(ix..ix + 1, cx));
            }
        }
    }

    fn remove_post(&mut self, post_id: &str, cx: &mut Context<Self>) {
        if let Some(ix) = self.posts.iter().position(|p| p.post.id == post_id) {
            self.posts.remove(ix);
            self.scroller.update(cx, |s, cx| s.splice(ix..ix + 1, 0, cx));
        }
        if self.thread.as_ref().is_some_and(|t| t.root_id == post_id) {
            self.thread = None;
        } else if let Some(thread) = self.thread.as_mut() {
            if let Some(ix) = thread.posts.iter().position(|p| p.post.id == post_id) {
                thread.posts.remove(ix);
                self.thread_scroller.update(cx, |s, cx| s.splice(ix..ix + 1, 0, cx));
            }
        }
        if self.editing.as_ref().is_some_and(|(_, id)| id == post_id) {
            self.editing = None;
        }
    }

    fn update_reactions(&mut self, post_id: &str, cx: &mut Context<Self>, f: impl Fn(&mut Vec<Reaction>)) {
        if let Some(ix) = self.posts.iter().position(|p| p.post.id == post_id) {
            f(&mut self.posts[ix].post.metadata.reactions);
            self.scroller.update(cx, |s, cx| s.remeasure_items(ix..ix + 1, cx));
        }
        if let Some(thread) = self.thread.as_mut() {
            if let Some(ix) = thread.posts.iter().position(|p| p.post.id == post_id) {
                f(&mut thread.posts[ix].post.metadata.reactions);
                self.thread_scroller.update(cx, |s, cx| s.remeasure_items(ix..ix + 1, cx));
            }
        }
    }

    // ------------------------------------------------------------------
    // Aides
    // ------------------------------------------------------------------

    fn me_id(&self) -> &str {
        self.me.as_ref().map(|u| u.id.as_str()).unwrap_or_default()
    }

    fn user_name(&self, user_id: &str) -> String {
        self.users
            .get(user_id)
            .map(User::display_name)
            .unwrap_or_else(|| "…".to_string())
    }

    fn channel_title(&self, channel: &Channel) -> String {
        match channel.direct_partner(self.me_id()) {
            Some(partner) => self.user_name(&partner),
            None if !channel.display_name.is_empty() => channel.display_name.clone(),
            None => channel.name.clone(),
        }
    }

    /// « Alice est en train d'écrire… » pour un canal ou un fil.
    fn typing_label(&self, channel_id: &str, parent_id: &str) -> Option<String> {
        let names: Vec<String> = self
            .typing
            .iter()
            .filter(|t| t.channel_id == channel_id && t.parent_id == parent_id && t.at.elapsed() < TYPING_DISPLAY)
            .map(|t| self.user_name(&t.user_id))
            .collect();
        match names.as_slice() {
            [] => None,
            [one] => Some(format!("{one} est en train d'écrire…")),
            [a, b] => Some(format!("{a} et {b} sont en train d'écrire…")),
            _ => Some("Plusieurs personnes sont en train d'écrire…".to_string()),
        }
    }

    fn focus_composer(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |s, cx| s.focus(window, cx));
    }
}

/// Format d'une image d'après ses premiers octets.
fn image_format(bytes: &[u8]) -> Option<ImageFormat> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some(ImageFormat::Png),
        [0xFF, 0xD8, ..] => Some(ImageFormat::Jpeg),
        [b'G', b'I', b'F', ..] => Some(ImageFormat::Gif),
        [b'B', b'M', ..] => Some(ImageFormat::Bmp),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some(ImageFormat::Webp),
        _ if bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml") => Some(ImageFormat::Svg),
        _ => None,
    }
}

/// Exécute une tâche bloquante sur un thread à part ; le résultat arrive
/// sur le canal renvoyé (à attendre depuis une tâche GPUI).
fn in_background<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> async_channel::Receiver<T> {
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let _ = tx.send_blocking(f());
    });
    rx
}
