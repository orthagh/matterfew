//! Client REST minimal pour l'API Mattermost v4.
//!
//! Seuls les points d'entrée nécessaires au client sont couverts :
//! utilisateur courant, équipes, canaux, messages, fils, envoi.

use std::collections::HashMap;

use anyhow::{Context as _, Result};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub last_name: String,
    #[serde(default)]
    pub nickname: String,
    /// Date de la dernière mise à jour de l'avatar (sert à invalider le cache).
    #[serde(default)]
    pub last_picture_update: i64,
}

impl User {
    /// Nom affiché : « Prénom Nom » si disponible, sinon le nom d'utilisateur.
    pub fn display_name(&self) -> String {
        let full = format!("{} {}", self.first_name, self.last_name);
        let full = full.trim();
        if !full.is_empty() {
            full.to_string()
        } else if !self.nickname.is_empty() {
            self.nickname.clone()
        } else {
            self.username.clone()
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Team {
    pub id: String,
    pub display_name: String,
    /// Date de mise à jour du logo de l'équipe (0 : pas de logo).
    #[serde(default)]
    pub last_team_icon_update: i64,
}

/// Non-lus d'une équipe (pour les pastilles de la barre des équipes).
#[derive(Debug, Clone, Deserialize)]
pub struct TeamUnread {
    pub team_id: String,
    #[serde(default)]
    pub msg_count_root: i64,
    #[serde(default)]
    pub mention_count_root: i64,
    #[serde(default)]
    pub msg_count: i64,
    #[serde(default)]
    pub mention_count: i64,
}

/// Fichier joint à un message.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct FileInfo {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub extension: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub mime_type: String,
    #[serde(default)]
    pub width: i64,
    #[serde(default)]
    pub height: i64,
    #[serde(default)]
    pub has_preview_image: bool,
}

impl FileInfo {
    pub fn is_image(&self) -> bool {
        self.mime_type.starts_with("image/")
            || matches!(self.extension.to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp")
    }

    /// « 12,4 Ko », « 3,1 Mo »…
    pub fn human_size(&self) -> String {
        human_size(self.size)
    }
}

pub fn human_size(bytes: i64) -> String {
    let b = bytes.max(0) as f64;
    let (value, unit) = if b < 1024.0 {
        return format!("{bytes} o");
    } else if b < 1024.0 * 1024.0 {
        (b / 1024.0, "Ko")
    } else if b < 1024.0 * 1024.0 * 1024.0 {
        (b / 1024.0 / 1024.0, "Mo")
    } else {
        (b / 1024.0 / 1024.0 / 1024.0, "Go")
    };
    format!("{value:.1} {unit}").replace('.', ",")
}

#[derive(Debug, Clone, Deserialize)]
pub struct Channel {
    pub id: String,
    /// "O" public, "P" privé, "D" message direct, "G" groupe.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    /// Objectif du canal (affiché sous son nom).
    #[serde(default)]
    pub purpose: String,
    /// En-tête du canal (Markdown, souvent des liens utiles).
    #[serde(default)]
    pub header: String,
    /// Messages racine (hors réponses de fil), pour le calcul du non-lu.
    #[serde(default)]
    pub total_msg_count_root: i64,
    #[serde(default)]
    pub last_post_at: i64,
    #[serde(default)]
    pub last_root_post_at: i64,
    #[serde(default)]
    pub create_at: i64,
    #[serde(default)]
    pub delete_at: i64,
}

impl Channel {
    /// Clé de tri « récents », comme le client web (fils réduits activés) :
    /// dernier message racine, à défaut dernier message, à défaut création.
    pub fn recency(&self) -> i64 {
        let last = if self.last_root_post_at > 0 { self.last_root_post_at } else { self.last_post_at };
        last.max(self.create_at)
    }
    pub fn is_direct(&self) -> bool {
        self.kind == "D"
    }
    pub fn is_group(&self) -> bool {
        self.kind == "G"
    }
    /// Pour un message direct, l'identifiant de l'autre participant
    /// (le nom du canal est de la forme `idA__idB`).
    pub fn direct_partner(&self, me: &str) -> Option<String> {
        if !self.is_direct() {
            return None;
        }
        self.name
            .split("__")
            .find(|id| *id != me)
            .or(Some(me)) // message à soi-même
            .map(str::to_string)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChannelMember {
    pub channel_id: String,
    #[serde(default)]
    pub msg_count_root: i64,
    #[serde(default)]
    pub mention_count_root: i64,
    #[serde(default)]
    pub notify_props: NotifyProps,
}

impl ChannelMember {
    /// Canal en sourdine : il ne passe en « non lu » que sur mention.
    pub fn is_muted(&self) -> bool {
        self.notify_props.mark_unread == "mention"
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NotifyProps {
    #[serde(default)]
    pub mark_unread: String,
}

/// Catégorie de la barre latérale (Favoris, Canaux, Messages directs,
/// catégories personnelles), telle que l'utilisateur l'a configurée.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SidebarCategory {
    pub id: String,
    /// "favorites", "channels", "direct_messages", "custom" ou "managed".
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub display_name: String,
    /// "" (défaut), "alpha", "recent" ou "manual".
    #[serde(default)]
    pub sorting: String,
    #[serde(default)]
    pub collapsed: bool,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub channel_ids: Vec<String>,
    /// Champs non utilisés ici (user_id, team_id, sort_order…), conservés
    /// tels quels pour les renvoyer intacts lors d'une mise à jour.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct OrderedCategories {
    #[serde(default)]
    categories: Vec<SidebarCategory>,
    #[serde(default)]
    order: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Preference {
    pub category: String,
    pub name: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserStatus {
    pub user_id: String,
    /// "online", "away", "dnd" ou "offline".
    pub status: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Reaction {
    pub user_id: String,
    pub emoji_name: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PostMetadata {
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(default)]
    pub files: Vec<FileInfo>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Post {
    pub id: String,
    pub create_at: i64,
    #[serde(default)]
    pub edit_at: i64,
    #[serde(default)]
    pub delete_at: i64,
    pub user_id: String,
    pub channel_id: String,
    #[serde(default)]
    pub root_id: String,
    #[serde(default)]
    pub message: String,
    /// Vide pour un message normal, "system_*" pour les messages système.
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub reply_count: i64,
    #[serde(default)]
    pub metadata: PostMetadata,
}

impl Post {
    pub fn is_system(&self) -> bool {
        self.kind.starts_with("system_")
    }
    pub fn is_reply(&self) -> bool {
        !self.root_id.is_empty()
    }
}

/// Forme renvoyée par les points d'entrée de liste de messages.
#[derive(Debug, Deserialize)]
struct PostList {
    #[serde(default)]
    order: Vec<String>,
    #[serde(default)]
    posts: HashMap<String, Post>,
}

impl PostList {
    /// Messages du plus ancien au plus récent.
    fn into_chronological(mut self) -> Vec<Post> {
        let mut out: Vec<Post> = self
            .order
            .iter()
            .filter_map(|id| self.posts.remove(id))
            .collect();
        out.sort_by_key(|p| p.create_at);
        out
    }
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base: String,
    token: String,
}

impl Client {
    pub fn new(server_url: &str, token: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        let mut auth = HeaderValue::from_str(&format!("Bearer {token}"))
            .context("jeton invalide (caractères non autorisés)")?;
        auth.set_sensitive(true);
        headers.insert(AUTHORIZATION, auth);
        headers.insert("X-Requested-With", HeaderValue::from_static("XMLHttpRequest"));

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .user_agent(concat!("mm-gpui/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self {
            http,
            base: server_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
        })
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// URL WebSocket dérivée de l'URL du serveur (http → ws, https → wss).
    pub fn websocket_url(&self) -> String {
        let ws = if let Some(rest) = self.base.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = self.base.strip_prefix("http://") {
            format!("ws://{rest}")
        } else {
            format!("wss://{}", self.base)
        };
        format!("{ws}/api/v4/websocket")
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v4{}", self.base, path)
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let resp = self
            .http
            .get(self.url(path))
            .send()
            .await
            .map_err(|e| network_error(&self.base, e))?;
        Self::decode(resp, path).await
    }

    async fn post<B: Serialize + ?Sized, T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let resp = self
            .http
            .post(self.url(path))
            .json(body)
            .send()
            .await
            .map_err(|e| network_error(&self.base, e))?;
        Self::decode(resp, path).await
    }

    async fn decode<T: for<'de> Deserialize<'de>>(resp: reqwest::Response, path: &str) -> Result<T> {
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            anyhow::bail!("Jeton refusé par le serveur (invalide, expiré ou révoqué).");
        }
        if !status.is_success() {
            // Mattermost renvoie {"message": "..."} en cas d'erreur.
            let body = resp.text().await.unwrap_or_default();
            let msg = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(str::to_string))
                .unwrap_or(body);
            anyhow::bail!("{path} : HTTP {status} — {msg}");
        }
        resp.json::<T>()
            .await
            .with_context(|| format!("{path} : réponse illisible"))
    }

    pub async fn me(&self) -> Result<User> {
        self.get("/users/me").await
    }

    pub async fn my_teams(&self) -> Result<Vec<Team>> {
        self.get("/users/me/teams").await
    }

    pub async fn my_channels(&self, team_id: &str) -> Result<Vec<Channel>> {
        self.get(&format!("/users/me/teams/{team_id}/channels")).await
    }

    pub async fn my_channel_members(&self, team_id: &str) -> Result<Vec<ChannelMember>> {
        self.get(&format!("/users/me/teams/{team_id}/channels/members"))
            .await
    }

    pub async fn channel(&self, channel_id: &str) -> Result<Channel> {
        self.get(&format!("/channels/{channel_id}")).await
    }

    pub async fn users_by_ids(&self, ids: &[String]) -> Result<Vec<User>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.post("/users/ids", ids).await
    }

    /// Derniers messages racine d'un canal (les réponses sont dans les fils).
    pub async fn channel_posts(&self, channel_id: &str, per_page: u32) -> Result<Vec<Post>> {
        let list: PostList = self
            .get(&format!(
                "/channels/{channel_id}/posts?page=0&per_page={per_page}&collapsedThreads=true"
            ))
            .await?;
        Ok(list.into_chronological())
    }

    /// Message racine + toutes ses réponses.
    pub async fn thread(&self, root_id: &str) -> Result<Vec<Post>> {
        let list: PostList = self.get(&format!("/posts/{root_id}/thread")).await?;
        Ok(list.into_chronological())
    }

    pub async fn create_post(&self, channel_id: &str, root_id: &str, message: &str, file_ids: &[String]) -> Result<Post> {
        #[derive(Serialize)]
        struct NewPost<'a> {
            channel_id: &'a str,
            root_id: &'a str,
            message: &'a str,
            file_ids: &'a [String],
        }
        self.post(
            "/posts",
            &NewPost {
                channel_id,
                root_id,
                message,
                file_ids,
            },
        )
        .await
    }

    pub async fn team_unreads(&self) -> Result<Vec<TeamUnread>> {
        self.get("/users/me/teams/unread").await
    }

    pub async fn team_image(&self, team_id: &str, last_update: i64) -> Result<Vec<u8>> {
        self.get_bytes(&format!("/teams/{team_id}/image?_={last_update}")).await
    }

    /// Aperçu d'une image jointe (JPEG réduit, généré par le serveur).
    pub async fn file_preview(&self, file_id: &str) -> Result<Vec<u8>> {
        self.get_bytes(&format!("/files/{file_id}/preview")).await
    }

    /// Fichier joint complet.
    pub async fn file_content(&self, file_id: &str) -> Result<Vec<u8>> {
        self.get_bytes(&format!("/files/{file_id}")).await
    }

    /// Envoie un fichier dans un canal ; renvoie sa description (dont l'id
    /// à joindre au message).
    pub async fn upload_file(&self, channel_id: &str, path: &std::path::Path) -> Result<FileInfo> {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "fichier".into());
        let bytes = tokio::fs::read(path)
            .await
            .with_context(|| format!("lecture de {}", path.display()))?;
        let part = reqwest::multipart::Part::bytes(bytes).file_name(name);
        let form = reqwest::multipart::Form::new()
            .text("channel_id", channel_id.to_string())
            .part("files", part);
        let resp = self
            .http
            .post(self.url("/files"))
            // Les gros fichiers peuvent prendre du temps.
            .timeout(std::time::Duration::from_secs(600))
            .multipart(form)
            .send()
            .await
            .map_err(|e| network_error(&self.base, e))?;
        #[derive(Deserialize)]
        struct Uploaded {
            file_infos: Vec<FileInfo>,
        }
        let uploaded: Uploaded = Self::decode(resp, "/files").await?;
        uploaded.file_infos.into_iter().next().context("le serveur n'a pas renvoyé le fichier")
    }

    async fn get_bytes(&self, path: &str) -> Result<Vec<u8>> {
        let resp = self
            .http
            .get(self.url(path))
            .timeout(std::time::Duration::from_secs(600))
            .send()
            .await
            .map_err(|e| network_error(&self.base, e))?;
        if !resp.status().is_success() {
            anyhow::bail!("{path} : HTTP {}", resp.status());
        }
        Ok(resp.bytes().await?.to_vec())
    }

    /// Marque le canal comme lu.
    pub async fn view_channel(&self, channel_id: &str) -> Result<()> {
        let _: serde_json::Value = self
            .post(
                "/channels/members/me/view",
                &serde_json::json!({ "channel_id": channel_id }),
            )
            .await?;
        Ok(())
    }

    /// Messages racine antérieurs à `before` (pour remonter l'historique).
    pub async fn channel_posts_before(&self, channel_id: &str, before: &str, per_page: u32) -> Result<Vec<Post>> {
        let list: PostList = self
            .get(&format!(
                "/channels/{channel_id}/posts?before={before}&per_page={per_page}&collapsedThreads=true"
            ))
            .await?;
        Ok(list.into_chronological())
    }

    /// Catégories de la barre latérale, dans l'ordre choisi par l'utilisateur.
    pub async fn categories(&self, team_id: &str) -> Result<Vec<SidebarCategory>> {
        let mut ordered: OrderedCategories = self
            .get(&format!("/users/me/teams/{team_id}/channels/categories"))
            .await?;
        let order = std::mem::take(&mut ordered.order);
        let mut categories = ordered.categories;
        if !order.is_empty() {
            categories.sort_by_key(|c| order.iter().position(|id| *id == c.id).unwrap_or(usize::MAX));
        }
        Ok(categories)
    }

    pub async fn update_category(&self, team_id: &str, category: &SidebarCategory) -> Result<()> {
        let path = format!("/users/me/teams/{team_id}/channels/categories/{}", category.id);
        self.send_json(reqwest::Method::PUT, &path, Some(category)).await
    }

    pub async fn preferences(&self) -> Result<Vec<Preference>> {
        self.get("/users/me/preferences").await
    }

    pub async fn statuses(&self, ids: &[String]) -> Result<Vec<UserStatus>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.post("/users/status/ids", ids).await
    }

    /// Image de profil (octets bruts, PNG en général).
    pub async fn profile_image(&self, user_id: &str, last_update: i64) -> Result<Vec<u8>> {
        self.get_bytes(&format!("/users/{user_id}/image?_={last_update}")).await
    }

    pub async fn patch_post(&self, post_id: &str, message: &str) -> Result<()> {
        let path = format!("/posts/{post_id}/patch");
        self.send_json(reqwest::Method::PUT, &path, Some(&serde_json::json!({ "message": message })))
            .await
    }

    pub async fn delete_post(&self, post_id: &str) -> Result<()> {
        self.send_json::<()>(reqwest::Method::DELETE, &format!("/posts/{post_id}"), None)
            .await
    }

    pub async fn add_reaction(&self, user_id: &str, post_id: &str, emoji_name: &str) -> Result<()> {
        let body = serde_json::json!({ "user_id": user_id, "post_id": post_id, "emoji_name": emoji_name });
        self.send_json(reqwest::Method::POST, "/reactions", Some(&body)).await
    }

    pub async fn remove_reaction(&self, user_id: &str, post_id: &str, emoji_name: &str) -> Result<()> {
        let path = format!("/users/{user_id}/posts/{post_id}/reactions/{emoji_name}");
        self.send_json::<()>(reqwest::Method::DELETE, &path, None).await
    }

    /// Requête dont seule la réussite compte (le corps de réponse est ignoré).
    async fn send_json<B: Serialize + ?Sized>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<()> {
        let mut req = self.http.request(method, self.url(path));
        if let Some(body) = body {
            req = req.json(body);
        }
        let resp = req.send().await.map_err(|e| network_error(&self.base, e))?;
        let _: serde_json::Value = Self::decode(resp, path).await.or_else(|e| {
            // Certaines réponses de succès ont un corps vide.
            if e.to_string().contains("réponse illisible") { Ok(serde_json::Value::Null) } else { Err(e) }
        })?;
        Ok(())
    }
}

/// Échec d'une connexion par identifiant et mot de passe.
#[derive(Debug)]
pub enum LoginError {
    /// Le compte a la double authentification : un code est nécessaire.
    MfaRequired,
    Other(String),
}

/// Connexion par e-mail et mot de passe (+ code de double authentification).
/// Renvoie le jeton de session.
pub async fn login_with_password(server: &str, login_id: &str, password: &str, mfa: &str) -> Result<String, LoginError> {
    let base = server.trim_end_matches('/');
    let http = reqwest::Client::builder()
        .user_agent(concat!("mm-gpui/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| LoginError::Other(e.to_string()))?;
    let mut body = serde_json::json!({ "login_id": login_id, "password": password });
    if !mfa.is_empty() {
        body["token"] = mfa.into();
    }
    let resp = http
        .post(format!("{base}/api/v4/users/login"))
        .header("X-Requested-With", "XMLHttpRequest")
        .json(&body)
        .send()
        .await
        .map_err(|e| LoginError::Other(network_error(base, e).to_string()))?;

    if resp.status().is_success() {
        return resp
            .headers()
            .get("Token")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
            .ok_or_else(|| LoginError::Other("Le serveur n'a pas renvoyé de jeton de session.".into()));
    }
    let err: serde_json::Value = resp.json().await.unwrap_or_default();
    let id = err.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    let message = err.get("message").and_then(|v| v.as_str()).unwrap_or("Connexion refusée.");
    if id.contains("mfa") && mfa.is_empty() {
        Err(LoginError::MfaRequired)
    } else {
        Err(LoginError::Other(message.to_string()))
    }
}

/// Ferme la session côté serveur (jeton de session uniquement : appelé avec
/// un jeton d'accès personnel, cela ne servirait à rien).
pub async fn logout(server: &str, token: &str) {
    if let Ok(client) = Client::new(server, token) {
        let _ = client.send_json::<()>(reqwest::Method::POST, "/users/logout", None).await;
    }
}

/// Erreur réseau lisible : « Serveur injoignable (…) » plutôt que la chaîne
/// complète des erreurs internes de reqwest.
fn network_error(base: &str, err: reqwest::Error) -> anyhow::Error {
    if err.is_timeout() {
        return anyhow::anyhow!("Le serveur {base} ne répond pas (délai dépassé).");
    }
    // La cause la plus profonde est en général la plus parlante
    // (« Hôte inconnu », « Connexion refusée », certificat invalide…).
    let mut cause: &dyn std::error::Error = &err;
    while let Some(next) = cause.source() {
        cause = next;
    }
    anyhow::anyhow!("Serveur injoignable ({base}) : {cause}")
}
