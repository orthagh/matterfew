//! Serveur MCP local, en lecture seule : un assistant (Claude Code, Claude
//! Desktop…) cherche et lit les messages avec la session de Matterfew.
//!
//! Il écoute sur `127.0.0.1` seulement et chaque requête doit porter le jeton
//! choisi dans les paramètres (`Authorization: Bearer …`). Aucun outil
//! n'écrit : ni message, ni réaction, ni lecture marquée.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use anyhow::Result;
use chrono::{Local, NaiveDate, TimeZone as _};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::api::{Channel, Client, Post, Team, User};

/// Ce que l'interface affiche : sert à « ce canal », « ce fil ».
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Screen {
    pub team_id: String,
    pub channel_id: String,
    pub thread_root: String,
}

static SCREEN: RwLock<Screen> =
    RwLock::new(Screen { team_id: String::new(), channel_id: String::new(), thread_root: String::new() });

/// Appelé par l'interface quand le canal ou le fil affiché change.
pub fn set_screen(screen: Screen) {
    if let Ok(mut current) = SCREEN.write()
        && *current != screen
    {
        *current = screen;
    }
}

fn screen() -> Screen {
    SCREEN.read().map(|s| s.clone()).unwrap_or_default()
}

/// Adresse locale du serveur.
pub fn url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/mcp")
}

/// Commande à coller dans un terminal pour brancher Claude Code.
pub fn claude_command(port: u16, token: &str) -> String {
    format!("claude mcp add --transport http matterfew {} --header \"Authorization: Bearer {token}\"", url(port))
}

/// Nouveau jeton d'accès, aléatoire.
pub fn new_token() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

/// Démarre le serveur ; il s'arrête quand `stop` est annulé.
pub async fn serve(
    client: Client,
    server_url: String,
    me_id: String,
    teams: Vec<Team>,
    port: u16,
    token: String,
    stop: CancellationToken,
) -> Result<()> {
    let tools = Matterfew::new(client, server_url, me_id, teams);
    let service = StreamableHttpService::new(
        move || Ok(tools.clone()),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default().with_cancellation_token(stop.child_token()),
    );
    let expected = format!("Bearer {token}");
    let router = axum::Router::new().nest_service("/mcp", service).layer(axum::middleware::from_fn(
        move |req: axum::extract::Request, next: axum::middleware::Next| {
            let ok = req
                .headers()
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| constant_time_eq(v.as_bytes(), expected.as_bytes()));
            async move { if ok { next.run(req).await } else { axum::http::StatusCode::UNAUTHORIZED.into_response() } }
        },
    ));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    axum::serve(listener, router).with_graceful_shutdown(async move { stop.cancelled().await }).await?;
    Ok(())
}

use axum::response::IntoResponse as _;

/// Comparaison sans fuite de durée (jeton).
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Clone)]
pub struct Matterfew {
    client: Client,
    server_url: String,
    me_id: String,
    teams: Arc<Vec<Team>>,
    users: Arc<Mutex<HashMap<String, User>>>,
    channels: Arc<Mutex<HashMap<String, Channel>>>,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchArgs {
    /// Mattermost search terms: words (all must match), "exact phrase",
    /// prefix* , -excluded, from:username, in:channel-name, on:/after:/before:YYYY-MM-DD.
    pub query: String,
    /// Team name or id; all teams when omitted.
    #[serde(default)]
    pub team: Option<String>,
    /// Maximum number of results (default 30, at most 100).
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ThreadArgs {
    /// Id of any message of the thread (root or reply).
    pub post_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ChannelArgs {
    /// Channel id, name (e.g. "town-square") or display name; "current" for
    /// the channel open in Matterfew.
    pub channel: String,
    /// Only messages on or after this day (YYYY-MM-DD, local time).
    #[serde(default)]
    pub after: Option<String>,
    /// Only messages before this day (YYYY-MM-DD, local time).
    #[serde(default)]
    pub before: Option<String>,
    /// Maximum number of messages, the most recent kept (default 100, at most 500).
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListArgs {
    /// Team name or id; all teams when omitted.
    #[serde(default)]
    pub team: Option<String>,
    /// Only channels with unread messages.
    #[serde(default)]
    pub unread_only: bool,
    /// Only channels whose name contains this text: part of a channel name,
    /// or of a username for a direct message.
    #[serde(default)]
    pub query: Option<String>,
    /// Maximum number of channels per team, the most recently active kept
    /// (default 50, at most 500).
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UserArgs {
    /// Username (with or without @) or user id.
    pub user: String,
}

const MAX_TEXT: usize = 1500;

#[tool_router]
impl Matterfew {
    pub fn new(client: Client, server_url: String, me_id: String, teams: Vec<Team>) -> Self {
        Self {
            client,
            server_url: server_url.trim_end_matches('/').to_string(),
            me_id,
            teams: Arc::new(teams),
            users: Arc::default(),
            channels: Arc::default(),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        annotations(read_only_hint = true),
        description = "Search messages with Mattermost search. Words must all match (no fuzzy \
        matching): try synonyms, prefixes (deploy*) and filters (from:, in:, after:, before:) \
        across several searches, then read the threads found with get_thread. Newest first."
    )]
    async fn search_messages(&self, Parameters(args): Parameters<SearchArgs>) -> Result<String, String> {
        let teams = self.teams_for(args.team.as_deref())?;
        let mut posts: Vec<Post> = Vec::new();
        for team in &teams {
            let found = self.client.search_posts(&team.id, &args.query).await.map_err(|e| format!("{e:#}"))?;
            posts.extend(found);
        }
        posts.sort_by_key(|p| std::cmp::Reverse(p.create_at));
        posts.dedup_by(|a, b| a.id == b.id);
        posts.truncate(args.limit.unwrap_or(30).clamp(1, 100));
        if posts.is_empty() {
            return Ok(format!("No message matches {:?}.", args.query));
        }
        let mut out = format!("{} result(s) for {:?}:\n\n", posts.len(), args.query);
        out.push_str(&self.format_posts(&posts, true).await);
        Ok(out)
    }

    #[tool(
        annotations(read_only_hint = true),
        description = "Read a whole thread (root message and all replies) from the id of any of its messages."
    )]
    async fn get_thread(&self, Parameters(args): Parameters<ThreadArgs>) -> Result<String, String> {
        let posts = self.client.thread(&args.post_id).await.map_err(|e| format!("{e:#}"))?;
        if posts.is_empty() {
            return Err("Thread not found.".into());
        }
        Ok(self.format_posts(&posts, false).await)
    }

    #[tool(
        annotations(read_only_hint = true),
        description = "Read the messages of a channel, oldest first, optionally between two days. \
        Replies are included."
    )]
    async fn get_channel_messages(&self, Parameters(args): Parameters<ChannelArgs>) -> Result<String, String> {
        let channel = self.find_channel(&args.channel).await?;
        let after = args.after.as_deref().map(day_start).transpose()?;
        let before = args.before.as_deref().map(day_start).transpose()?;
        let limit = args.limit.unwrap_or(100).clamp(1, 500);
        let mut kept: Vec<Post> = Vec::new();
        // Pages de 200, des plus récents aux plus anciens, jusqu'à la date voulue.
        for page in 0..10 {
            let posts = self.client.channel_posts_page(&channel.id, page, 200).await.map_err(|e| format!("{e:#}"))?;
            let oldest = posts.first().map(|p| p.create_at);
            let done = posts.len() < 200;
            kept.extend(posts.into_iter().filter(|p| {
                p.delete_at == 0 && after.is_none_or(|a| p.create_at >= a) && before.is_none_or(|b| p.create_at < b)
            }));
            let reached = match after {
                Some(a) => oldest.is_some_and(|o| o < a),
                None => kept.len() >= limit,
            };
            if done || reached {
                break;
            }
        }
        kept.sort_by_key(|p| p.create_at);
        kept.dedup_by(|a, b| a.id == b.id);
        let skip = kept.len().saturating_sub(limit);
        let kept = &kept[skip..];
        let name = self.channel_label(&channel).await;
        if kept.is_empty() {
            return Ok(format!("No message in {name} for this period."));
        }
        Ok(format!("{} message(s) in {name}:\n\n{}", kept.len(), self.format_posts(kept, false).await))
    }

    #[tool(
        annotations(read_only_hint = true),
        description = "List the channels, direct and group messages the user belongs to, most recently \
        active first, with unread counts. 50 per team unless `limit` says otherwise: narrow the list with \
        `query` or `unread_only` rather than reading it whole."
    )]
    async fn list_channels(&self, Parameters(args): Parameters<ListArgs>) -> Result<String, String> {
        let teams = self.teams_for(args.team.as_deref())?;
        let wanted = args.query.as_deref().map(str::trim).filter(|q| !q.is_empty()).map(str::to_lowercase);
        let limit = args.limit.unwrap_or(50).clamp(1, 500);
        let mut out = String::new();
        for team in &teams {
            let (channels, members) =
                tokio::try_join!(self.client.my_channels(&team.id), self.client.my_channel_members(&team.id))
                    .map_err(|e| format!("{e:#}"))?;
            let read: HashMap<&str, i64> = members.iter().map(|m| (m.channel_id.as_str(), m.msg_count)).collect();
            let mut rows: Vec<(i64, String)> = Vec::new();
            // The partners of all direct messages in one go, not one request each.
            let partners = channels.iter().filter(|c| c.kind == "D").flat_map(|c| c.name.split("__"));
            self.users(partners.map(str::to_string)).await;
            for c in channels.iter().filter(|c| c.delete_at == 0) {
                let unread = (c.total_msg_count - read.get(c.id.as_str()).copied().unwrap_or(c.total_msg_count)).max(0);
                if args.unread_only && unread == 0 {
                    continue;
                }
                self.channels.lock().unwrap().insert(c.id.clone(), c.clone());
                let kind = match c.kind.as_str() {
                    "O" => "public",
                    "P" => "private",
                    "D" => "direct",
                    _ => "group",
                };
                let label = self.channel_label(c).await;
                if wanted.as_ref().is_some_and(|w| !label.to_lowercase().contains(w)) {
                    continue;
                }
                let unread = if unread > 0 { format!(", {unread} unread") } else { String::new() };
                rows.push((c.last_post_at, format!("- {label} ({kind}{unread}) id: {}", c.id)));
            }
            rows.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
            let hidden = rows.len().saturating_sub(limit);
            out.push_str(&format!("Team {} ({}):\n", team.display_name, team.name));
            for (_, row) in rows.into_iter().take(limit) {
                out.push_str(&row);
                out.push('\n');
            }
            if hidden > 0 {
                out.push_str(&format!("… and {hidden} less recently active: narrow with `query`, or raise `limit`.\n"));
            }
            out.push('\n');
        }
        Ok(out)
    }

    #[tool(
        annotations(read_only_hint = true),
        description = "Profile of a user: name, username, position, email when visible."
    )]
    async fn get_user(&self, Parameters(args): Parameters<UserArgs>) -> Result<String, String> {
        let name = args.user.trim().trim_start_matches('@');
        let user = match self.client.user_by_username(name).await {
            Ok(u) => u,
            Err(_) => self
                .client
                .users_by_ids(&[name.to_string()])
                .await
                .ok()
                .and_then(|mut l| l.pop())
                .ok_or_else(|| format!("No user {name:?}."))?,
        };
        let mut out = format!("@{} — {}", user.username, user.display_name());
        for (label, value) in [("nickname", &user.nickname), ("position", &user.position), ("email", &user.email)] {
            if !value.is_empty() {
                out.push_str(&format!("\n{label}: {value}"));
            }
        }
        out.push_str(&format!("\nid: {}", user.id));
        Ok(out)
    }

    #[tool(
        annotations(read_only_hint = true),
        description = "What is open in Matterfew right now: team, channel and thread, with their ids. \
        Use it for requests about \"this channel\" or \"this thread\"."
    )]
    async fn get_current_context(&self) -> Result<String, String> {
        let screen = screen();
        if screen.channel_id.is_empty() {
            return Ok("No channel is open.".into());
        }
        let mut out = String::new();
        if let Some(team) = self.teams.iter().find(|t| t.id == screen.team_id) {
            out.push_str(&format!("team: {} ({})\n", team.display_name, team.name));
        }
        let channel = self.channel(&screen.channel_id).await?;
        out.push_str(&format!("channel: {} id: {}\n", self.channel_label(&channel).await, channel.id));
        if !screen.thread_root.is_empty() {
            out.push_str(&format!("open thread root id: {}\n", screen.thread_root));
        }
        Ok(out)
    }
}

#[tool_handler]
impl ServerHandler for Matterfew {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("matterfew", env!("CARGO_PKG_VERSION")).with_title("Matterfew"))
            .with_instructions(
                "Read-only access to the user's Mattermost through the Matterfew desktop client: search \
             messages, read threads and channels, look up users. Mattermost search matches whole \
             words, so run several searches with variants, then read the threads found. Quote \
             messages with their link.",
            )
    }
}

impl Matterfew {
    /// Équipes visées : celle nommée, sinon toutes.
    fn teams_for(&self, wanted: Option<&str>) -> Result<Vec<Team>, String> {
        match wanted.map(str::trim).filter(|w| !w.is_empty()) {
            None => Ok(self.teams.to_vec()),
            Some(w) => self
                .teams
                .iter()
                .find(|t| t.id == w || t.name.eq_ignore_ascii_case(w) || t.display_name.eq_ignore_ascii_case(w))
                .map(|t| vec![t.clone()])
                .ok_or_else(|| format!("No team {w:?}.")),
        }
    }

    async fn channel(&self, id: &str) -> Result<Channel, String> {
        if let Some(c) = self.channels.lock().unwrap().get(id).cloned() {
            return Ok(c);
        }
        let c = self.client.channel(id).await.map_err(|e| format!("{e:#}"))?;
        self.channels.lock().unwrap().insert(id.to_string(), c.clone());
        Ok(c)
    }

    /// Canal par identifiant, nom, nom affiché, ou « current ».
    async fn find_channel(&self, wanted: &str) -> Result<Channel, String> {
        let wanted = wanted.trim().trim_start_matches(['~', '#']);
        if wanted.eq_ignore_ascii_case("current") {
            let id = screen().channel_id;
            if id.is_empty() {
                return Err("No channel is open in Matterfew.".into());
            }
            return self.channel(&id).await;
        }
        if let Ok(c) = self.channel(wanted).await {
            return Ok(c);
        }
        for team in self.teams.iter() {
            let channels = self.client.my_channels(&team.id).await.map_err(|e| format!("{e:#}"))?;
            for c in &channels {
                self.channels.lock().unwrap().insert(c.id.clone(), c.clone());
            }
            if let Some(c) = channels
                .into_iter()
                .find(|c| c.name.eq_ignore_ascii_case(wanted) || c.display_name.eq_ignore_ascii_case(wanted))
            {
                return Ok(c);
            }
        }
        Err(format!("No channel {wanted:?} among the user's channels (try list_channels)."))
    }

    async fn users(&self, ids: impl IntoIterator<Item = String>) -> HashMap<String, User> {
        let missing: Vec<String> = {
            let known = self.users.lock().unwrap();
            let mut m: Vec<String> = ids.into_iter().filter(|id| !known.contains_key(id)).collect();
            m.sort();
            m.dedup();
            m
        };
        if !missing.is_empty()
            && let Ok(found) = self.client.users_by_ids(&missing).await
        {
            let mut known = self.users.lock().unwrap();
            for u in found {
                known.insert(u.id.clone(), u);
            }
        }
        self.users.lock().unwrap().clone()
    }

    /// « ~nom-du-canal », « @personne » pour un message direct.
    async fn channel_label(&self, c: &Channel) -> String {
        match c.kind.as_str() {
            "D" => {
                let other = c.name.split("__").find(|id| *id != self.me_id).unwrap_or(&self.me_id).to_string();
                let users = self.users([other.clone()]).await;
                format!("direct message with @{}", users.get(&other).map(|u| u.username.as_str()).unwrap_or("?"))
            }
            "G" => format!("group message {}", c.display_name),
            _ => format!("~{} ({})", c.name, c.display_name),
        }
    }

    /// Lien permanent : équipe du canal, sinon celle affichée (messages directs).
    fn permalink(&self, post: &Post, channel: Option<&Channel>) -> String {
        let team_id = channel.map(|c| c.team_id.clone()).filter(|t| !t.is_empty()).unwrap_or_else(|| screen().team_id);
        let team = self.teams.iter().find(|t| t.id == team_id).or(self.teams.first());
        format!("{}/{}/pl/{}", self.server_url, team.map(|t| t.name.as_str()).unwrap_or_default(), post.id)
    }

    /// Messages lisibles : date, canal, auteur, texte, puis id et lien.
    async fn format_posts(&self, posts: &[Post], with_channel: bool) -> String {
        let users = self.users(posts.iter().map(|p| p.user_id.clone())).await;
        let mut out = String::new();
        for post in posts {
            let channel = self.channel(&post.channel_id).await.ok();
            let author = users.get(&post.user_id).map(|u| format!("@{}", u.username)).unwrap_or_else(|| "?".into());
            let when = Local.timestamp_millis_opt(post.create_at).single().map(|t| t.format("%Y-%m-%d %H:%M"));
            let when = when.map(|w| w.to_string()).unwrap_or_default();
            let place = match &channel {
                Some(c) if with_channel => format!(" {}", self.channel_label(c).await),
                _ => String::new(),
            };
            let mut text: String = post.message.trim().chars().take(MAX_TEXT).collect();
            if post.message.trim().chars().count() > MAX_TEXT {
                text.push('…');
            }
            out.push_str(&format!("[{when}]{place} {author}:\n{}\n", indent(&text)));
            let mut meta = format!("  id: {}", post.id);
            if !post.root_id.is_empty() {
                meta.push_str(&format!(" · reply in thread {}", post.root_id));
            } else if post.reply_count > 0 {
                meta.push_str(&format!(" · {} replies", post.reply_count));
            }
            meta.push_str(&format!(" · {}", self.permalink(post, channel.as_ref())));
            out.push_str(&meta);
            out.push_str("\n\n");
        }
        out
    }
}

fn indent(text: &str) -> String {
    text.lines().map(|l| format!("  {l}")).collect::<Vec<_>>().join("\n")
}

/// « 2026-11-24 » → début de ce jour, heure locale, en ms.
fn day_start(day: &str) -> Result<i64, String> {
    let date =
        NaiveDate::parse_from_str(day.trim(), "%Y-%m-%d").map_err(|_| format!("Bad date {day:?} (YYYY-MM-DD)."))?;
    let start = date.and_hms_opt(0, 0, 0).and_then(|t| Local.from_local_datetime(&t).earliest());
    start.map(|t| t.timestamp_millis()).ok_or_else(|| format!("Bad date {day:?}."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jeton_et_commande() {
        let t = new_token();
        assert_eq!(t.len(), 64);
        assert_ne!(t, new_token());
        let cmd = claude_command(41814, "abc");
        assert!(cmd.contains("http://127.0.0.1:41814/mcp"));
        assert!(cmd.contains("Authorization: Bearer abc"));
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }

    #[test]
    fn dates() {
        assert!(day_start("2026-11-24").is_ok());
        assert!(day_start("24/11/2026").is_err());
        assert!(day_start("2026-11-25").unwrap() > day_start("2026-11-24").unwrap());
    }

    /// Faux serveur Mattermost + serveur MCP : jeton exigé, outils listés,
    /// recherche lisible avec auteur, canal et lien.
    #[test]
    fn serveur_de_bout_en_bout() {
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        rt.block_on(async {
            use axum::routing::{get, post};
            let mm = axum::Router::new()
                .route(
                    "/api/v4/teams/t1/posts/search",
                    post(|| async {
                        axum::Json(serde_json::json!({"order": ["p1"], "posts": {"p1": {
                            "id": "p1", "create_at": 1764000000000_i64, "user_id": "u1",
                            "channel_id": "c1", "message": "Déploiement de la 2.4 ce soir", "reply_count": 2
                        }}}))
                    }),
                )
                .route(
                    "/api/v4/users/ids",
                    post(|| async { axum::Json(serde_json::json!([{"id": "u1", "username": "alice"}])) }),
                )
                .route(
                    "/api/v4/users/me/teams/t1/channels",
                    get(|| async {
                        axum::Json(serde_json::json!([
                            {"id": "c1", "type": "O", "name": "dev", "display_name": "Dev", "team_id": "t1",
                             "last_post_at": 30, "total_msg_count": 5},
                            {"id": "c2", "type": "P", "name": "ops", "display_name": "Ops", "team_id": "t1",
                             "last_post_at": 20, "total_msg_count": 0},
                            {"id": "c3", "type": "D", "name": "moi__u1", "team_id": "",
                             "last_post_at": 10, "total_msg_count": 0}
                        ]))
                    }),
                )
                .route(
                    "/api/v4/users/me/teams/t1/channels/members",
                    get(|| async { axum::Json(serde_json::json!([{"channel_id": "c1", "msg_count": 3}])) }),
                )
                .route(
                    "/api/v4/channels/c1",
                    get(|| async {
                        axum::Json(serde_json::json!({"id": "c1", "type": "O", "name": "dev",
                            "display_name": "Dev", "team_id": "t1"}))
                    }),
                );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let mm_url = format!("http://{}", listener.local_addr().unwrap());
            tokio::spawn(async move { axum::serve(listener, mm).await.unwrap() });

            let port = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap().port();
            let client = Client::new(&mm_url, "jeton-mm").unwrap();
            let teams: Vec<Team> =
                serde_json::from_value(serde_json::json!([{"id": "t1", "display_name": "Équipe", "name": "equipe"}]))
                    .unwrap();
            let stop = CancellationToken::new();
            tokio::spawn(serve(client, mm_url.clone(), "moi".into(), teams, port, "secret".into(), stop.clone()));
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;

            let http = reqwest::Client::new();
            let rpc = |body: serde_json::Value, auth: &str, session: Option<String>| {
                let mut req = http
                    .post(url(port))
                    .header("Accept", "application/json, text/event-stream")
                    .header("Authorization", format!("Bearer {auth}"))
                    .json(&body);
                if let Some(s) = session {
                    req = req.header("Mcp-Session-Id", s);
                }
                req.send()
            };
            // Mauvais jeton : refusé.
            let refused = rpc(serde_json::json!({}), "faux", None).await.unwrap();
            assert_eq!(refused.status(), 401);

            let init = serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
                "protocolVersion": "2025-06-18", "capabilities": {},
                "clientInfo": {"name": "test", "version": "1"}}});
            let resp = rpc(init, "secret", None).await.unwrap();
            assert!(resp.status().is_success(), "{}", resp.status());
            let session = resp.headers().get("mcp-session-id").map(|v| v.to_str().unwrap().to_string());
            let hello = body_json(resp).await;
            assert_eq!(hello["result"]["serverInfo"]["name"], "matterfew", "{hello}");
            let note = serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
            rpc(note, "secret", session.clone()).await.unwrap();

            let list = serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"});
            let tools = body_json(rpc(list, "secret", session.clone()).await.unwrap()).await;
            let tools = tools["result"]["tools"].as_array().unwrap();
            let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
            // Nothing writes, and every tool says so.
            for tool in tools {
                assert_eq!(tool["annotations"]["readOnlyHint"], true, "{tool}");
            }
            for n in ["search_messages", "get_thread", "get_channel_messages", "list_channels", "get_user"] {
                assert!(names.contains(&n), "{n} absent de {names:?}");
            }

            let call = serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                "params": {"name": "search_messages", "arguments": {"query": "déploiement"}}});
            let found = body_json(rpc(call, "secret", session.clone()).await.unwrap()).await;
            let text = found["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string();
            assert!(text.contains("@alice"), "{found}");
            assert!(text.contains("~dev"), "{text}");
            assert!(text.contains("Déploiement de la 2.4"), "{text}");
            assert!(text.contains(&format!("{mm_url}/equipe/pl/p1")), "{text}");

            // The channel list is cut to the most recently active, and says what it left out.
            let channels = |arguments: serde_json::Value, id: u32| {
                let call = serde_json::json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
                    "params": {"name": "list_channels", "arguments": arguments}});
                let sent = rpc(call, "secret", session.clone());
                async move {
                    let reply = body_json(sent.await.unwrap()).await;
                    reply["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string()
                }
            };
            let cut = channels(serde_json::json!({"limit": 2}), 4).await;
            assert!(cut.contains("~dev (Dev) (public, 2 unread) id: c1"), "{cut}");
            assert!(cut.contains("~ops") && !cut.contains("@alice"), "{cut}");
            assert!(cut.contains("and 1 less recently active"), "{cut}");
            let named = channels(serde_json::json!({"query": "ALI"}), 5).await;
            assert!(named.contains("direct message with @alice") && !named.contains("~dev"), "{named}");
            stop.cancel();
        });
    }

    /// Réponse JSON, directe ou dans un flux SSE (`data: …`).
    async fn body_json(resp: reqwest::Response) -> serde_json::Value {
        let text = resp.text().await.unwrap();
        let json = text
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(str::trim)
            .rfind(|l| l.starts_with('{'))
            .map(str::to_string)
            .unwrap_or(text);
        serde_json::from_str(&json).unwrap_or(serde_json::Value::Null)
    }
}
