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
use crate::backend::{self, AcKind, Command, Event, PAGE_SIZE, Suggestion};
use crate::sidebar::{ChannelState, SidebarPrefs};
use crate::settings::{SIDEBAR_MAX, SIDEBAR_MIN, UiSettings};
use crate::theme::{MattermostTheme, Palette};
use crate::api::{LoginError, login_with_password};
use crate::credentials::AuthKind;
use crate::desktop::{self, ClickTarget, TrayAction, Tray};
use crate::{credentials, demo, markdown, sso};

/// Un message et son Markdown déjà préparé (emojis, mentions).
struct PostView {
    post: Post,
    md: SharedString,
}

impl PostView {
    fn new(post: Post) -> Self {
        let custom: Vec<String> = post.metadata.emojis.iter().map(|e| e.name.clone()).collect();
        let md = markdown::prepare_with(&post.message, &custom).into();
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

/// Image affichée en grand dans la visionneuse.
struct Viewer {
    file: FileInfo,
    image: Option<Arc<Image>>,
    error: Option<String>,
}

/// Marqueur du glisser de la poignée de redimensionnement.
struct SidebarResize;

impl Render for SidebarResize {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// Propositions d'autocomplétion affichées au-dessus d'une zone de saisie.
struct Autocomplete {
    pane: Pane,
    kind: AcKind,
    /// Début (octets) du mot remplacé par la proposition choisie.
    start: usize,
    items: Vec<Suggestion>,
    selected: usize,
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

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
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
    /// Configuration publique du serveur.
    server_config: HashMap<String, String>,
    /// Fils réduits actifs (réponses rangées dans les fils, pas dans le canal).
    crt: bool,
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
    /// Images externes (aperçus de liens, intégrations), par URL.
    external_images: HashMap<String, Arc<Image>>,
    /// Images des emojis personnalisés, par nom (partagées avec le rendu
    /// Markdown, d'où l'`Arc`).
    custom_emojis: Arc<HashMap<String, Arc<Image>>>,
    requested_emojis: HashSet<String>,
    autocomplete: Option<Autocomplete>,
    /// Brouillons : (canal, message racine du fil ou vide) → texte.
    drafts: HashMap<(String, String), String>,
    /// Textes à placer dans les zones de saisie au prochain rendu (il faut
    /// la fenêtre pour modifier une zone de saisie).
    pending_text: Vec<(Pane, String)>,
    /// Canal marqué « non lu » à la main : on ne le marque plus lu
    /// automatiquement tant qu'on ne l'a pas quitté.
    manually_unread: Option<String>,
    /// Aperçu du Markdown à la place de la zone de saisie.
    preview: HashSet<Pane>,
    /// Canal pour lequel l'invite de la zone de saisie a été écrite.
    placeholder_for: Option<String>,
    /// Numéro de la dernière demande d'autocomplétion (les réponses plus
    /// anciennes sont ignorées).
    ac_seq: u64,
    attachments: Vec<Attachment>,
    next_local_id: u64,
    /// Message d'information temporaire (ex. fichier téléchargé).
    info: Option<String>,
    viewer: Option<Viewer>,
    viewer_focus: FocusHandle,
    settings: UiSettings,
    /// Redimensionnement de la barre latérale en cours.
    resizing: bool,
    /// Poignée Win32 de la fenêtre (pastille, clignotement, masquage).
    hwnd: Option<isize>,
    /// Icône de la zone de notification.
    tray: Option<Tray>,
    /// Dernier état affiché sur la pastille : (mentions, non-lus, session expirée).
    badge: (i64, bool, bool),
    /// Le serveur a refusé le jeton en cours d'utilisation.
    session_expired: bool,
    statuses: HashMap<String, String>,
    typing: Vec<Typing>,
    last_typing_sent: Option<Instant>,

    channel_id: Option<String>,
    /// Dernière lecture connue de chaque canal (ms).
    last_viewed: HashMap<String, i64>,
    /// Ligne « Nouveaux messages » du canal affiché : messages postérieurs à
    /// cette date (lecture précédente), fixée à l'ouverture du canal.
    new_since: Option<i64>,
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
                .placeholder("Écrire un message")
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
                    InputEvent::Change => {
                        this.signal_typing(pane);
                        this.update_autocomplete(pane, cx);
                    }
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
            server_config: HashMap::new(),
            crt: true,
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
            external_images: HashMap::new(),
            custom_emojis: Arc::default(),
            requested_emojis: HashSet::new(),
            autocomplete: None,
            drafts: HashMap::new(),
            pending_text: Vec::new(),
            manually_unread: None,
            preview: HashSet::new(),
            placeholder_for: None,
            ac_seq: 0,
            attachments: Vec::new(),
            next_local_id: 0,
            info: None,
            viewer: None,
            viewer_focus: cx.focus_handle(),
            settings: UiSettings::load(),
            resizing: false,
            hwnd: desktop::hwnd(window),
            tray: Tray::new(),
            badge: (0, false, false),
            session_expired: false,
            statuses: HashMap::new(),
            typing: Vec::new(),
            last_typing_sent: None,
            channel_id: None,
            last_viewed: HashMap::new(),
            new_since: None,
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
        this.listen_desktop(window, cx);
        if std::env::args().any(|a| a == "--demo") {
            this.start_demo(window, cx);
        } else if let Some((url, token, kind)) = auto.filter(|_| !std::env::args().any(|a| a == "--login")) {
            this.connect_with(url, token, kind, window, cx);
        }
        this
    }

    /// Clics sur les notifications et actions de l'icône de la zone de
    /// notification ; fermeture de la fenêtre → réduction dans cette zone.
    fn listen_desktop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let clicks = desktop::clicks();
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(target) = clicks.recv().await {
                if this.update_in(cx, |this, window, cx| this.open_target(target, window, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        let actions = desktop::tray_actions();
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(action) = actions.recv().await {
                let alive = this.update_in(cx, |this, window, cx| match action {
                    TrayAction::Show => this.bring_to_front(window),
                    TrayAction::Quit => cx.quit(),
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        // Avec une icône dans la zone de notification, fermer la fenêtre la
        // masque (comme le client officiel) ; « Quitter » est dans son menu.
        if self.tray.is_some() {
            if let Some(hwnd) = self.hwnd {
                window.on_window_should_close(cx, move |_, _| {
                    desktop::hide_window(hwnd);
                    false
                });
            }
        }
    }

    fn bring_to_front(&mut self, window: &mut Window) {
        if let Some(hwnd) = self.hwnd {
            desktop::show_window(hwnd);
        }
        window.activate_window();
    }

    /// Clic sur une notification : fenêtre au premier plan, bonne équipe,
    /// bon canal.
    fn open_target(&mut self, target: ClickTarget, window: &mut Window, cx: &mut Context<Self>) {
        self.bring_to_front(window);
        let other_team = !target.team_id.is_empty() && self.team_id.as_deref() != Some(target.team_id.as_str());
        if other_team {
            // Le canal sera ouvert quand les canaux de l'équipe seront chargés.
            self.settings.last_channels.insert(target.team_id.clone(), target.channel_id);
            self.select_team(target.team_id, cx);
        } else {
            self.select_channel(target.channel_id, cx);
        }
        self.focus_composer(window, cx);
    }

    /// Pastille de la barre des tâches et icône de la zone de notification :
    /// mentions et non-lus de toutes les équipes.
    fn update_badge(&mut self) {
        let current_team = self.team_id.clone().unwrap_or_default();
        let mut mentions: i64 = self.channels.values().map(|c| c.mentions).sum();
        let mut unread = self.channels.values().any(|c| c.shows_unread());
        for (team, (team_unread, team_mentions)) in &self.team_unreads {
            if *team != current_team {
                mentions += team_mentions;
                unread |= *team_unread;
            }
        }
        let state = (mentions, unread, self.session_expired);
        if state == self.badge {
            return;
        }
        self.badge = state;
        if let Some(hwnd) = self.hwnd {
            desktop::set_taskbar_badge(hwnd, mentions, unread, self.session_expired);
        }
        if let Some(tray) = self.tray.as_mut() {
            tray.update(mentions, unread);
        }
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
        let crt = collapsed_threads_enabled(&self.server_config, &self.raw_prefs);
        if crt != self.crt {
            self.crt = crt;
            self.send(Command::SetCollapsedThreads(crt));
            // Compteurs et liste des messages changent de nature : on recharge.
            if self.screen == Screen::Chat {
                self.thread = None;
                if let Some(team_id) = self.team_id.clone() {
                    self.send(Command::LoadTeam { team_id });
                }
                if let Some(channel_id) = self.channel_id.clone() {
                    self.loading_posts = true;
                    self.send(Command::LoadChannel { channel_id });
                }
            }
        }
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
        if !self.demo {
            self.settings.last_team = Some(team_id.clone());
            self.settings.save();
        }
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
        // Brouillons : on garde le texte du canal quitté, on reprend celui du nouveau.
        self.stash_draft(Pane::Channel, cx);
        self.stash_draft(Pane::Thread, cx);
        let draft = self.drafts.get(&(channel_id.clone(), String::new())).cloned().unwrap_or_default();
        self.pending_text.push((Pane::Channel, draft));
        self.pending_text.push((Pane::Thread, String::new()));
        self.manually_unread = None;
        self.preview.clear();
        if let (Some(team_id), false) = (self.team_id.clone(), self.demo) {
            self.settings.last_channels.insert(team_id, channel_id.clone());
            self.settings.save();
        }
        // Un canal non lu reste dans « Non lus » tant qu'on ne le quitte pas.
        self.sticky_unread = self
            .channels
            .get(&channel_id)
            .filter(|c| c.shows_unread())
            .map(|_| channel_id.clone());
        self.new_since = self
            .channels
            .get(&channel_id)
            .filter(|c| c.shows_unread())
            .and_then(|_| self.last_viewed.get(&channel_id).copied());
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
        if let Some(id) = self.channel_id.clone().filter(|id| self.manually_unread.as_ref() != Some(id)) {
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
        self.stash_draft(Pane::Thread, cx);
        if let Some(channel_id) = self.channel_id.clone() {
            let draft = self.drafts.get(&(channel_id, root_id.clone())).cloned().unwrap_or_default();
            self.pending_text.push((Pane::Thread, draft));
        }
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
        self.stash_draft(Pane::Thread, cx);
        self.pending_text.push((Pane::Thread, String::new()));
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
        self.autocomplete = None;
        // « /commande » : exécutée par le serveur (« //texte » s'envoie tel quel).
        if message.starts_with('/') && !message.starts_with("//") && file_ids.is_empty() {
            let team_id = self.team_id.clone().unwrap_or_default();
            self.send(Command::ExecuteCommand { channel_id, team_id, root_id, command: message });
            state.update(cx, |s, cx| s.set_value("", window, cx));
            cx.notify();
            return;
        }
        self.send(Command::Send { channel_id, root_id, message, file_ids });
        self.attachments.retain(|a| a.pane != pane);
        self.last_typing_sent = None;
        state.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    /// Rattrapage après une coupure où des événements ont été perdus :
    /// canaux, membres et catégories de l'équipe, pastilles des équipes,
    /// messages du canal affiché modifiés depuis le plus récent connu, fil ouvert.
    fn resync(&mut self) {
        if let Some(team_id) = self.team_id.clone() {
            self.send(Command::LoadTeam { team_id });
        }
        self.send(Command::LoadTeamUnreads);
        self.send(Command::LoadPreferences);
        if let Some(channel_id) = self.channel_id.clone() {
            let since = self
                .posts
                .iter()
                .map(|p| p.post.create_at.max(p.post.update_at).max(p.post.edit_at))
                .max();
            match since {
                Some(since) => self.send(Command::SyncChannel { channel_id, since }),
                None => self.send(Command::LoadChannel { channel_id }),
            }
        }
        if let Some(root_id) = self.thread.as_ref().map(|t| t.root_id.clone()) {
            self.send(Command::LoadThread { root_id });
        }
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

    /// Ouvre une image jointe dans la visionneuse intégrée.
    fn open_image(&mut self, file: FileInfo, window: &mut Window, cx: &mut Context<Self>) {
        // L'aperçu s'affiche en attendant l'image complète.
        let image = self.previews.get(&file.id).cloned();
        self.send(Command::LoadFullImage { file_id: file.id.clone() });
        self.viewer = Some(Viewer { file, image, error: None });
        window.focus(&self.viewer_focus, cx);
        cx.notify();
    }

    fn close_viewer(&mut self, cx: &mut Context<Self>) {
        self.viewer = None;
        cx.notify();
    }

    /// Nouvelle largeur de la barre latérale pendant le glisser.
    fn resize_sidebar(&mut self, width: f32, cx: &mut Context<Self>) {
        self.settings.sidebar_width = width.clamp(SIDEBAR_MIN, SIDEBAR_MAX);
        self.resizing = true;
        cx.notify();
    }

    /// Fin du glisser : la largeur est mémorisée.
    fn end_resize(&mut self) {
        if self.resizing {
            self.resizing = false;
            self.settings.save();
        }
    }

    /// Index du premier message « nouveau » (après la lecture précédente,
    /// et pas de soi), s'il y en a un.
    fn first_new_index(&self) -> Option<usize> {
        let since = self.new_since?;
        let me = self.me_id();
        self.posts.iter().position(|p| p.post.create_at > since && p.post.user_id != me)
    }

    /// Demande les aperçus des images jointes pas encore chargées.
    fn request_previews<'a>(&mut self, posts: impl IntoIterator<Item = &'a Post> + Clone) {
        self.request_external_images(posts.clone());
        let emojis: Vec<(String, String)> = posts
            .clone()
            .into_iter()
            .flat_map(|p| p.metadata.emojis.iter())
            .filter(|e| !self.requested_emojis.contains(&e.name))
            .map(|e| (e.id.clone(), e.name.clone()))
            .collect();
        for (id, name) in emojis {
            if self.requested_emojis.insert(name.clone()) {
                self.send(Command::LoadEmoji { id, name });
            }
        }
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

    /// Demande les images externes (aperçus de liens, intégrations) des messages.
    fn request_external_images<'a>(&mut self, posts: impl IntoIterator<Item = &'a Post>) {
        let mut urls = Vec::new();
        for post in posts {
            for a in crate::rich::attachments(post) {
                urls.extend([a.image_url, a.thumb_url].into_iter().filter(|u| !u.is_empty()));
            }
            for e in crate::rich::embeds(post) {
                match e {
                    crate::rich::Embed::Link { image: Some(url), .. } | crate::rich::Embed::Image { url, .. } => urls.push(url),
                    _ => {}
                }
            }
        }
        for url in urls {
            // Même ensemble que les aperçus de fichiers : une URL n'est
            // jamais un identifiant de fichier.
            if self.requested_previews.insert(url.clone()) {
                self.send(Command::LoadExternalImage { url });
            }
        }
    }

    // ------------------------------------------------------------------
    // Autocomplétion
    // ------------------------------------------------------------------

    /// Analyse le mot en cours (juste avant le curseur) et demande des
    /// propositions : `@personne`, `~canal`, `:emoji`, `/commande`.
    fn update_autocomplete(&mut self, pane: Pane, cx: &mut Context<Self>) {
        let state = self.composer_for(pane);
        let (text, cursor) = {
            let s = state.read(cx);
            let text = s.value().to_string();
            let cursor = s.cursor().min(text.len());
            (text, cursor)
        };
        let Some((kind, start, term)) = autocomplete_trigger(&text, cursor) else {
            if self.autocomplete.as_ref().is_some_and(|a| a.pane == pane) {
                self.autocomplete = None;
                cx.notify();
            }
            return;
        };
        let (Some(team_id), Some(channel_id)) = (self.team_id.clone(), self.channel_id.clone()) else { return };
        let root_id = match (pane, &self.thread) {
            (Pane::Thread, Some(t)) => t.root_id.clone(),
            _ => String::new(),
        };

        self.ac_seq += 1;
        // Emojis standard : proposés tout de suite, sans attendre le serveur.
        let local: Vec<Suggestion> = if kind == AcKind::Emoji {
            crate::emoji::search(&term, 8)
                .into_iter()
                .map(|(code, glyph)| Suggestion {
                    label: format!(":{code}:"),
                    detail: String::new(),
                    insert: format!("{glyph} "),
                    user_id: None,
                    glyph: Some(glyph),
                })
                .collect()
        } else {
            Vec::new()
        };
        let keep = self.autocomplete.as_ref().filter(|a| a.pane == pane && a.kind == kind && a.start == start);
        let items = if kind == AcKind::Emoji { local } else { keep.map(|a| a.items.clone()).unwrap_or_default() };
        self.autocomplete = Some(Autocomplete { pane, kind, start, items, selected: 0 });
        self.send(Command::Autocomplete { seq: self.ac_seq, kind, term, team_id, channel_id, root_id });
        cx.notify();
    }

    /// Remplace le mot en cours par la proposition `index`.
    fn accept_suggestion(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ac) = self.autocomplete.take() else { return };
        let Some(item) = ac.items.get(index).cloned() else { return };
        let state = self.composer_for(ac.pane);
        state.update(cx, |s, cx| {
            let end = s.cursor();
            s.set_selected_range(ac.start..end.max(ac.start), cx);
            s.replace(item.insert.clone(), window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    /// Touches de navigation dans les propositions. Renvoie `true` si la
    /// touche a été utilisée (elle ne doit alors pas aller à la zone de saisie).
    fn autocomplete_key(&mut self, pane: Pane, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(ac) = self.autocomplete.as_mut().filter(|a| a.pane == pane && !a.items.is_empty()) else {
            return false;
        };
        match key {
            "up" => ac.selected = ac.selected.checked_sub(1).unwrap_or(ac.items.len() - 1),
            "down" => ac.selected = (ac.selected + 1) % ac.items.len(),
            "enter" | "tab" => {
                let index = ac.selected;
                self.accept_suggestion(index, window, cx);
            }
            "escape" => self.autocomplete = None,
            _ => return false,
        }
        cx.notify();
        true
    }

    // ------------------------------------------------------------------
    // Brouillons et éditeur
    // ------------------------------------------------------------------

    /// Range le texte en cours d'une zone de saisie dans les brouillons.
    fn stash_draft(&mut self, pane: Pane, cx: &mut Context<Self>) {
        let Some(channel_id) = self.channel_id.clone() else { return };
        let root = match (pane, &self.thread) {
            (Pane::Channel, _) => String::new(),
            (Pane::Thread, Some(t)) => t.root_id.clone(),
            (Pane::Thread, None) => return,
        };
        // Un message en cours de modification n'est pas un brouillon.
        if self.editing.as_ref().is_some_and(|(p, _)| *p == pane) {
            return;
        }
        let text = self.composer_for(pane).read(cx).value().to_string();
        if text.trim().is_empty() {
            self.drafts.remove(&(channel_id, root));
        } else {
            self.drafts.insert((channel_id, root), text);
        }
    }

    /// Canal avec un brouillon (indiqué dans la barre latérale).
    fn has_draft(&self, channel_id: &str) -> bool {
        self.drafts.keys().any(|(c, _)| c == channel_id)
    }

    /// Place les textes en attente dans les zones de saisie (appelé au rendu),
    /// et adapte l'invite au canal affiché (« Écrire dans Général »).
    fn apply_pending_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (pane, text) in std::mem::take(&mut self.pending_text) {
            let end = text.len();
            self.composer_for(pane).update(cx, |s, cx| {
                s.set_value(text, window, cx);
                // Curseur en fin de brouillon, prêt à continuer.
                s.set_selected_range(end..end, cx);
            });
        }
        if self.placeholder_for != self.channel_id {
            self.placeholder_for = self.channel_id.clone();
            let title = self
                .channel_id
                .as_ref()
                .and_then(|id| self.channels.get(id))
                .map(|c| self.channel_title(&c.channel));
            if let Some(title) = title {
                self.composer.update(cx, |s, cx| s.set_placeholder(format!("Écrire dans {title}"), window, cx));
            } else {
                // Canaux pas encore chargés : on réessaiera au prochain rendu.
                self.placeholder_for = None;
            }
        }
    }

    /// Mise en forme depuis la barre d'outils ou un raccourci.
    fn format(&mut self, pane: Pane, format: crate::editor::Format, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.composer_for(pane);
        state.update(cx, |s, cx| {
            let text = s.value().to_string();
            let (new_text, selection) = crate::editor::apply(&text, s.selected_range(), format);
            s.set_selected_range(0..text.len(), cx);
            s.replace(new_text, window, cx);
            s.set_selected_range(selection, cx);
            s.focus(window, cx);
        });
        self.preview.remove(&pane);
        cx.notify();
    }

    fn toggle_preview(&mut self, pane: Pane, window: &mut Window, cx: &mut Context<Self>) {
        if !self.preview.remove(&pane) {
            self.preview.insert(pane);
        } else {
            self.composer_for(pane).update(cx, |s, cx| s.focus(window, cx));
        }
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Options des messages, canaux, catégories et du statut
    // ------------------------------------------------------------------

    fn team_name(&self) -> String {
        let id = self.team_id.as_deref().unwrap_or_default();
        self.teams.iter().find(|t| t.id == id).map(|t| t.name.clone()).unwrap_or_default()
    }

    fn copy(&mut self, text: String, what: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.flash(format!("{what} copié dans le presse-papiers."), cx);
    }

    fn copy_post_link(&mut self, post_id: &str, cx: &mut Context<Self>) {
        let server = self.server_url.clone().unwrap_or_default();
        let link = format!("{server}/{}/pl/{post_id}", self.team_name());
        self.copy(link, "Lien du message", cx);
    }

    fn copy_post_text(&mut self, post_id: &str, cx: &mut Context<Self>) {
        let text = self.find_post(post_id).map(|p| p.message.clone()).unwrap_or_default();
        self.copy(text, "Texte du message", cx);
    }

    fn copy_channel_link(&mut self, channel_id: &str, cx: &mut Context<Self>) {
        let server = self.server_url.clone().unwrap_or_default();
        let name = self.channels.get(channel_id).map(|c| c.channel.name.clone()).unwrap_or_default();
        let link = format!("{server}/{}/channels/{name}", self.team_name());
        self.copy(link, "Lien du canal", cx);
    }

    fn find_post(&self, post_id: &str) -> Option<&Post> {
        self.posts
            .iter()
            .chain(self.thread.iter().flat_map(|t| t.posts.iter()))
            .map(|v| &v.post)
            .find(|p| p.id == post_id)
    }

    /// Message enregistré (« flagged » dans l'API).
    fn is_saved(&self, post_id: &str) -> bool {
        self.raw_prefs
            .iter()
            .any(|p| p.category == "flagged_post" && p.name == post_id && p.value == "true")
    }

    fn toggle_saved(&mut self, post_id: String, cx: &mut Context<Self>) {
        let save = !self.is_saved(&post_id);
        if save {
            self.raw_prefs.push(Preference {
                user_id: self.me_id().to_string(),
                category: "flagged_post".into(),
                name: post_id.clone(),
                value: "true".into(),
            });
        } else {
            self.raw_prefs.retain(|p| !(p.category == "flagged_post" && p.name == post_id));
        }
        self.send(Command::SavePost { post_id, save });
        self.flash(if save { "Message enregistré.".into() } else { "Message retiré des enregistrés.".into() }, cx);
    }

    fn toggle_pinned(&mut self, post_id: String) {
        let pin = !self.find_post(&post_id).is_some_and(|p| p.is_pinned);
        self.send(Command::PinPost { post_id, pin });
    }

    /// « Marquer comme non lu » : le canal redevient non lu à partir de ce
    /// message, et le reste tant qu'on ne le quitte pas.
    fn mark_unread_from(&mut self, post_id: String, cx: &mut Context<Self>) {
        let Some(post) = self.find_post(&post_id).cloned() else { return };
        self.send(Command::MarkUnread { post_id });
        self.manually_unread = Some(post.channel_id.clone());
        self.new_since = Some(post.create_at - 1);
        if let Some(state) = self.channels.get_mut(&post.channel_id) {
            state.unread_messages = true;
        }
        cx.notify();
    }

    fn is_favorite(&self, channel_id: &str) -> bool {
        self.categories
            .iter()
            .any(|c| c.kind == "favorites" && c.channel_ids.iter().any(|id| id == channel_id))
    }

    /// Ajoute aux favoris ou retire des favoris (déplace le canal entre la
    /// catégorie Favoris et sa catégorie d'origine).
    fn toggle_favorite(&mut self, channel_id: String, cx: &mut Context<Self>) {
        let Some(team_id) = self.team_id.clone() else { return };
        let favorite = !self.is_favorite(&channel_id);
        let is_dm = self.channels.get(&channel_id).is_some_and(|c| c.channel.is_direct() || c.channel.is_group());
        let home = if is_dm { "direct_messages" } else { "channels" };
        let mut changed = Vec::new();
        for category in self.categories.iter_mut() {
            let had = category.channel_ids.iter().any(|id| *id == channel_id);
            let wants = if favorite { category.kind == "favorites" } else { category.kind == home };
            if had && !wants {
                category.channel_ids.retain(|id| *id != channel_id);
                changed.push(category.clone());
            } else if !had && wants {
                category.channel_ids.insert(0, channel_id.clone());
                changed.push(category.clone());
            }
        }
        if !changed.is_empty() {
            self.send(Command::UpdateCategories { team_id, categories: changed });
        }
        cx.notify();
    }

    fn toggle_muted(&mut self, channel_id: String, cx: &mut Context<Self>) {
        if let Some(state) = self.channels.get_mut(&channel_id) {
            state.muted = !state.muted;
            let muted = state.muted;
            self.send(Command::SetMuted { channel_id, muted });
            cx.notify();
        }
    }

    fn leave_channel(&mut self, channel_id: String, cx: &mut Context<Self>) {
        self.send(Command::LeaveChannel { channel_id: channel_id.clone() });
        self.channels.remove(&channel_id);
        for category in self.categories.iter_mut() {
            category.channel_ids.retain(|id| *id != channel_id);
        }
        if self.channel_id.as_deref() == Some(channel_id.as_str()) {
            let next = self
                .channels
                .values()
                .find(|c| c.channel.name == "town-square")
                .or_else(|| self.channels.values().next())
                .map(|c| c.channel.id.clone());
            self.channel_id = None;
            if let Some(next) = next {
                self.select_channel(next, cx);
            }
        }
        cx.notify();
    }

    fn mark_channels_read(&mut self, channel_ids: Vec<String>, cx: &mut Context<Self>) {
        for id in &channel_ids {
            if let Some(state) = self.channels.get_mut(id) {
                state.unread_messages = false;
                state.mentions = 0;
            }
        }
        self.send(Command::MarkChannelsRead { channel_ids });
        cx.notify();
    }

    fn mark_category_read(&mut self, category_id: &str, cx: &mut Context<Self>) {
        let ids = self
            .categories
            .iter()
            .find(|c| c.id == category_id)
            .map(|c| c.channel_ids.clone())
            .unwrap_or_default();
        self.mark_channels_read(ids, cx);
    }

    fn set_category_sorting(&mut self, category_id: &str, sorting: &str, cx: &mut Context<Self>) {
        let Some(team_id) = self.team_id.clone() else { return };
        if let Some(category) = self.categories.iter_mut().find(|c| c.id == category_id) {
            category.sorting = sorting.to_string();
            let category = category.clone();
            self.send(Command::UpdateCategories { team_id, categories: vec![category] });
            cx.notify();
        }
    }

    fn set_status(&mut self, status: &str, cx: &mut Context<Self>) {
        let me = self.me_id().to_string();
        self.statuses.insert(me, status.to_string());
        self.send(Command::SetStatus { status: status.to_string() });
        cx.notify();
    }

    fn post_action(&mut self, post_id: String, action_id: String) {
        self.send(Command::PostAction { post_id, action_id });
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
            Event::Ready { me, teams, config, prefs } => {
                self.server_config = config;
                self.raw_prefs = prefs;
                self.crt = collapsed_threads_enabled(&self.server_config, &self.raw_prefs);
                self.send(Command::SetCollapsedThreads(self.crt));
                if let Some((url, token, kind)) = self.pending_credentials.take() {
                    if let Err(err) = credentials::save(&url, &token, kind) {
                        eprintln!("connexion non mémorisée : {err:#}");
                    }
                }
                self.users.insert(me.id.clone(), me.clone());
                self.me = Some(me);
                self.teams = teams;
                self.screen = Screen::Chat;
                // Dernière équipe ouverte si elle existe encore, sinon la première.
                let remembered = self.settings.last_team.clone().filter(|id| self.teams.iter().any(|t| &t.id == id));
                if let Some(id) = remembered.or_else(|| self.teams.first().map(|t| t.id.clone())) {
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
            Event::Suggestions { seq, kind, items } => {
                if seq == self.ac_seq {
                    match self.autocomplete.as_mut().filter(|a| a.kind == kind) {
                        // Emojis : les personnalisés complètent les standards déjà proposés.
                        Some(ac) if kind == AcKind::Emoji => {
                            ac.items.extend(items);
                            ac.items.truncate(12);
                        }
                        Some(ac) => {
                            ac.items = items;
                            ac.selected = 0;
                        }
                        None => {}
                    }
                    if self.autocomplete.as_ref().is_some_and(|a| a.items.is_empty()) {
                        self.autocomplete = None;
                    }
                }
            }
            Event::CustomEmoji { name, bytes } => {
                if let Some(format) = image_format(&bytes) {
                    let mut map = (*self.custom_emojis).clone();
                    map.insert(name, Arc::new(Image::from_bytes(format, bytes.to_vec())));
                    self.custom_emojis = Arc::new(map);
                }
            }
            Event::ExternalImage { url, bytes } => {
                if let Some(format) = image_format(&bytes) {
                    self.external_images.insert(url, Arc::new(Image::from_bytes(format, bytes.to_vec())));
                    self.scroller.update(cx, |s, cx| s.remeasure(cx));
                    self.thread_scroller.update(cx, |s, cx| s.remeasure(cx));
                }
            }
            Event::FullImage { file_id, result } => {
                if let Some(viewer) = self.viewer.as_mut().filter(|v| v.file.id == file_id) {
                    match result.map(|bytes| (image_format(&bytes), bytes)) {
                        Ok((Some(format), bytes)) => {
                            viewer.image = Some(Arc::new(Image::from_bytes(format, bytes.to_vec())));
                        }
                        Ok((None, _)) => viewer.error = Some("Format d'image non pris en charge.".into()),
                        Err(err) => viewer.error = Some(err),
                    }
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
                for m in &members {
                    self.last_viewed.insert(m.channel_id.clone(), m.last_viewed_at);
                }
                let members: HashMap<_, _> = members.into_iter().map(|m| (m.channel_id.clone(), m)).collect();
                self.channels = channels
                    .into_iter()
                    .filter(|c| c.delete_at == 0)
                    .map(|channel| {
                        let state = match members.get(&channel.id) {
                            // Avec les fils réduits, seuls les messages racine comptent.
                            Some(m) => ChannelState {
                                unread_messages: if self.crt {
                                    channel.total_msg_count_root > m.msg_count_root
                                } else {
                                    channel.total_msg_count > m.msg_count
                                },
                                mentions: if self.crt { m.mention_count_root } else { m.mention_count },
                                muted: m.is_muted(),
                                notify_desktop: m.notify_props.desktop.clone(),
                                channel,
                            },
                            None => ChannelState {
                                channel,
                                unread_messages: false,
                                mentions: 0,
                                muted: false,
                                notify_desktop: String::new(),
                            },
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
                    // Dernier canal ouvert dans cette équipe, sinon « Town Square ».
                    let remembered = self
                        .settings
                        .last_channels
                        .get(&team_id)
                        .filter(|id| self.channels.contains_key(*id))
                        .cloned();
                    let default = remembered.or_else(|| {
                        self.channels
                            .values()
                            .find(|c| c.channel.name == "town-square")
                            .or_else(|| self.channels.values().next())
                            .map(|c| c.channel.id.clone())
                    });
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
                    notify_desktop: String::new(),
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
                    // Beaucoup de nouveaux messages : on ouvre sur la ligne
                    // « Nouveaux messages » plutôt qu'en bas.
                    if let Some(ix) = self.first_new_index() {
                        if n - ix > 8 {
                            self.scroller.update(cx, |s, cx| {
                                s.scroll_to_item(ix.saturating_sub(1), cx);
                            });
                        }
                    }
                    // Le canal est lu : la prochaine ouverture partira d'ici.
                    if let Some(last) = self.posts.last() {
                        self.last_viewed.insert(channel_id.clone(), last.post.create_at);
                    }
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
            Event::Posted { post, mentions_me, followed, channel_type, team_id } => {
                self.on_posted(post, mentions_me, followed, &channel_type, &team_id, window, cx);
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
                self.connected = up;
                self.was_connected |= up;
            }
            // Le serveur n'a pas pu rejouer les événements manqués pendant une
            // coupure : on rattrape ce qui a changé (comme le client web).
            Event::Resync => self.resync(),
            Event::ChannelSince { channel_id, posts } => {
                if self.channel_id.as_deref() == Some(&channel_id) {
                    self.request_previews(&posts);
                    for post in posts {
                        if post.delete_at > 0 {
                            self.remove_post(&post.id, cx);
                        } else if self.posts.iter().any(|p| p.post.id == post.id) {
                            self.replace_post(post, cx);
                        } else if !self.crt || !post.is_reply() {
                            let ix = self.posts.partition_point(|p| p.post.create_at <= post.create_at);
                            self.posts.insert(ix, PostView::new(post));
                            self.scroller.update(cx, |s, cx| s.splice(ix..ix, 1, cx));
                        }
                    }
                }
            }
            Event::Error(message) => {
                eprintln!("erreur : {message}");
                if self.screen == Screen::Chat && message.contains("Jeton refusé") {
                    self.session_expired = true;
                }
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

    #[allow(clippy::too_many_arguments)]
    fn on_posted(
        &mut self,
        post: Post,
        mentions_me: bool,
        followed: bool,
        channel_type: &str,
        team_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

        // Sans fils réduits, les réponses s'affichent aussi dans le canal.
        let in_channel_list = !post.is_reply() || !self.crt;
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
        }
        if in_channel_list && is_current && !self.posts.iter().any(|p| p.post.id == post.id) {
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
                // Fils réduits : une réponse ne rend pas le canal non lu.
                if in_channel_list {
                    state.unread_messages = true;
                }
                if mentions_me || channel_type == "D" {
                    state.mentions += 1;
                }
            }
        }
        if seen && self.manually_unread.as_deref() != Some(post.channel_id.as_str()) {
            self.send(Command::MarkRead { channel_id: post.channel_id.clone() });
        }

        let state = self.channels.get(&post.channel_id);
        let me = self.me.as_ref();
        let notify = crate::notify_rules::should_notify(&crate::notify_rules::Input {
            from_me,
            from_webhook: post.from_webhook(),
            is_system: post.is_system(),
            channel_type,
            channel_muted: state.is_some_and(|s| s.muted),
            channel_level: state.map(|s| s.notify_desktop.as_str()).unwrap_or(""),
            user_level: me.and_then(|m| m.notify_props.get("desktop")).map(String::as_str).unwrap_or(""),
            my_status: self.statuses.get(&me_id).map(String::as_str).unwrap_or(""),
            mentions_me,
            follows_thread: followed,
            is_reply: post.is_reply(),
            collapsed_threads: self.crt,
            window_active: active,
            channel_visible: is_current,
            thread_visible: self.thread.as_ref().is_some_and(|t| t.root_id == post.root_id),
        });
        if notify {
            let sender = self.user_name(&post.user_id);
            let title = match self.channels.get(&post.channel_id) {
                Some(state) if !state.channel.is_direct() => {
                    format!("{sender} dans {}", self.channel_title(&state.channel))
                }
                _ => sender,
            };
            let body = markdown::plain_text(&post.message);
            let target = ClickTarget {
                team_id: if team_id.is_empty() { self.team_id.clone().unwrap_or_default() } else { team_id.to_string() },
                channel_id: post.channel_id.clone(),
            };
            self.send(Command::Notify { title, body, target: Some(target) });
            if !active {
                if let Some(hwnd) = self.hwnd {
                    desktop::flash(hwnd);
                }
            }
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

/// Mot déclenchant l'autocomplétion juste avant le curseur :
/// (type, début du mot en octets, terme recherché).
fn autocomplete_trigger(text: &str, cursor: usize) -> Option<(AcKind, usize, String)> {
    let before = text.get(..cursor)?;
    let start = before.rfind(char::is_whitespace).map(|i| i + before[i..].chars().next().map_or(1, char::len_utf8)).unwrap_or(0);
    let word = &before[start..];
    let mut chars = word.chars();
    let first = chars.next()?;
    let rest = chars.as_str();
    match first {
        '@' if !rest.is_empty() => Some((AcKind::User, start, rest.to_string())),
        '~' if !rest.is_empty() => Some((AcKind::Channel, start, rest.to_string())),
        ':' if rest.chars().count() >= 2 && !rest.contains(':') => Some((AcKind::Emoji, start, rest.to_lowercase())),
        // Commande : seulement en tout début de message, sur la première ligne.
        '/' if start == 0 && !before.contains('\n') && !rest.starts_with('/') => {
            Some((AcKind::Command, 0, before.to_string()))
        }
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

/// Fils réduits (« Collapsed Reply Threads ») actifs ? Même règle que le
/// client web : autorisés par le serveur, et activés par l'utilisateur ou
/// imposés / activés par défaut par le serveur.
fn collapsed_threads_enabled(config: &HashMap<String, String>, prefs: &[Preference]) -> bool {
    let server = config.get("CollapsedThreads").map(String::as_str).unwrap_or("default_on");
    if server == "disabled" {
        return false;
    }
    if server == "always_on" {
        return true;
    }
    let pref = prefs
        .iter()
        .find(|p| p.category == "display_settings" && p.name == "collapsed_reply_threads")
        .map(|p| p.value.as_str());
    match pref {
        Some(v) => v == "on",
        None => server == "default_on",
    }
}

#[cfg(test)]
mod tests {
    use super::{HashMap, Preference, collapsed_threads_enabled};

    fn pref(v: &str) -> Vec<Preference> {
        vec![Preference { user_id: String::new(), category: "display_settings".into(), name: "collapsed_reply_threads".into(), value: v.into() }]
    }

    #[test]
    fn autocomplete_triggers() {
        use super::{AcKind, autocomplete_trigger};
        assert_eq!(autocomplete_trigger("salut @je", 9), Some((AcKind::User, 6, "je".into())));
        assert_eq!(autocomplete_trigger("voir ~glp", 9), Some((AcKind::Channel, 5, "glp".into())));
        assert_eq!(autocomplete_trigger("bravo :th", 9), Some((AcKind::Emoji, 6, "th".into())));
        assert_eq!(autocomplete_trigger("/aw", 3), Some((AcKind::Command, 0, "/aw".into())));
        assert_eq!(autocomplete_trigger("a /aw", 5), None);
        assert_eq!(autocomplete_trigger("12:30", 5), None);
        assert_eq!(autocomplete_trigger("mail a@b", 8), None);
        assert_eq!(autocomplete_trigger("@", 1), None);
    }

    #[test]
    fn collapsed_threads_rules() {
        let cfg = |v: &str| HashMap::from([("CollapsedThreads".to_string(), v.to_string())]);
        assert!(!collapsed_threads_enabled(&cfg("disabled"), &pref("on")));
        assert!(collapsed_threads_enabled(&cfg("always_on"), &pref("off")));
        assert!(collapsed_threads_enabled(&cfg("default_on"), &[]));
        assert!(!collapsed_threads_enabled(&cfg("default_off"), &[]));
        assert!(!collapsed_threads_enabled(&cfg("default_on"), &pref("off")));
        assert!(collapsed_threads_enabled(&cfg("default_off"), &pref("on")));
    }
}
