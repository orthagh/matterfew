//! Contenus enrichis d'un message :
//! - pièces jointes des intégrations (`post.props.attachments`, format
//!   « Slack » utilisé par GitLab, Jenkins, les webhooks…) ;
//! - aperçus de liens fournis par le serveur (`post.metadata.embeds`).
//!
//! Seulement la lecture des données ; le rendu est dans `ui/view.rs`.

use serde::Deserialize;
use serde_json::Value;

use crate::api::Post;

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Attachment {
    pub fallback: String,
    pub color: String,
    pub pretext: String,
    pub author_name: String,
    pub author_link: String,
    pub title: String,
    pub title_link: String,
    pub text: String,
    pub fields: Vec<Field>,
    pub image_url: String,
    pub thumb_url: String,
    pub footer: String,
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Field {
    pub title: String,
    /// Texte ou nombre selon les intégrations.
    #[serde(deserialize_with = "any_to_string")]
    pub value: String,
    pub short: bool,
}

/// Bouton d'un message interactif (`POST /posts/{id}/actions/{action_id}`).
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Action {
    pub id: String,
    pub name: String,
    /// "button" (ou vide) ; les listes déroulantes ("select") ne sont pas gérées.
    #[serde(rename = "type")]
    pub kind: String,
    /// "default", "primary", "success", "good", "warning", "danger"…
    pub style: String,
}

fn any_to_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => s,
        Value::Null => String::new(),
        other => other.to_string(),
    })
}

/// Pièces jointes d'intégration d'un message (vide s'il n'y en a pas).
pub fn attachments(post: &Post) -> Vec<Attachment> {
    post.props
        .get("attachments")
        .and_then(|v| serde_json::from_value::<Vec<Attachment>>(v.clone()).ok())
        .unwrap_or_default()
}

/// What happened to an issue or a pull request, as told by the GitHub plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeEventKind {
    IssueOpened,
    IssueClosed,
    IssueReopened,
    PrOpened,
    PrDraft,
    PrMerged,
    PrClosed,
    PrReopened,
}

/// A GitHub plugin notification about an issue or a pull request, taken apart
/// so that the interface can show its title alone and the rest as metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeEvent {
    pub kind: CodeEventKind,
    /// "#123 Title", without Markdown.
    pub title: String,
    pub url: String,
    /// Repository name, without its owner.
    pub repo: String,
    pub author: String,
}

/// Reads a notification of the form
/// `[\[owner/repo\]](url) New issue [#12 Title](url) opened by [user](url).`
/// Anything else (comments, reviews, pushes, other integrations) is `None`
/// and keeps its plain rendering.
pub fn code_event(message: &str) -> Option<CodeEvent> {
    let line = message.lines().map(str::trim).find(|l| !l.is_empty())?;
    let (repo, rest) = line.strip_prefix("[\\[")?.split_once("\\]](")?;
    let (_, rest) = rest.split_once(')')?;
    let (before, rest) = rest.split_once("[#")?;
    let (item, rest) = rest.split_once("](")?;
    let (url, after) = rest.split_once(')')?;
    let (number, title) = item.split_once(' ')?;
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (before, verb) = (before.to_lowercase(), after.to_lowercase());
    let has = |word: &str| verb.contains(word);
    let kind = if before.contains("pull request") {
        if has("merged") {
            CodeEventKind::PrMerged
        } else if has("reopened") {
            CodeEventKind::PrReopened
        } else if has("closed") {
            CodeEventKind::PrClosed
        } else if has("opened") && before.contains("draft") {
            CodeEventKind::PrDraft
        } else if has("opened") {
            CodeEventKind::PrOpened
        } else {
            return None;
        }
    } else if before.contains("issue") {
        if has("reopened") {
            CodeEventKind::IssueReopened
        } else if has("closed") {
            CodeEventKind::IssueClosed
        } else if has("opened") {
            CodeEventKind::IssueOpened
        } else {
            return None;
        }
    } else {
        return None;
    };
    // "... by [name](url)." or "... by @name."
    let by = after.rsplit_once(" by ")?.1.trim().trim_end_matches('.');
    let author = by.strip_prefix('[').and_then(|link| link.split_once("](")).map_or(by, |(name, _)| name);
    Some(CodeEvent {
        kind,
        title: format!("#{number} {}", title.replace('`', "")),
        url: url.to_string(),
        repo: repo.rsplit('/').next().unwrap_or(repo).to_string(),
        author: author.trim_start_matches('@').to_string(),
    })
}

/// Nom affiché à la place de l'auteur (webhooks avec `override_username`).
pub fn override_username(post: &Post) -> Option<String> {
    post.props.get("override_username").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

/// Couleur de la barre latérale d'une pièce jointe.
pub fn attachment_color(color: &str) -> Option<&str> {
    match color {
        "good" => Some("#3db887"),
        "warning" => Some("#ffbc1f"),
        "danger" => Some("#d24b4e"),
        "" => None,
        c => Some(c),
    }
}

/// Aperçu de lien préparé par le serveur.
#[derive(Debug, Clone, PartialEq)]
pub enum Embed {
    /// Page web (OpenGraph) : site, titre, description, image.
    Link { url: String, site: String, title: String, description: String, image: Option<String> },
    /// Image seule (lien direct vers une image).
    Image { url: String, width: i64, height: i64 },
    /// Lien vers un autre message Mattermost.
    Permalink { channel: String, message: String, user_id: String },
}

pub fn embeds(post: &Post) -> Vec<Embed> {
    let Some(list) = post.metadata.embeds.as_array() else { return Vec::new() };
    let str_at = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
    list.iter()
        .filter_map(|e| {
            let url = str_at(e, "url");
            let data = e.get("data").cloned().unwrap_or(Value::Null);
            match e.get("type").and_then(Value::as_str)? {
                "opengraph" => {
                    let title = str_at(&data, "title");
                    let description = str_at(&data, "description");
                    if title.is_empty() && description.is_empty() {
                        return None;
                    }
                    let image = data
                        .get("images")
                        .and_then(Value::as_array)
                        .and_then(|imgs| imgs.first())
                        .map(|img| {
                            let secure = str_at(img, "secure_url");
                            if secure.is_empty() { str_at(img, "url") } else { secure }
                        })
                        .filter(|u| !u.is_empty());
                    Some(Embed::Link { site: str_at(&data, "site_name"), title, description, image, url })
                }
                "image" => {
                    let size = post.metadata.images.get(&url);
                    let dim = |k: &str| size.and_then(|s| s.get(k)).and_then(Value::as_i64).unwrap_or(0);
                    Some(Embed::Image { width: dim("width"), height: dim("height"), url })
                }
                "permalink" => {
                    let quoted = data.get("post")?;
                    Some(Embed::Permalink {
                        channel: str_at(&data, "channel_display_name"),
                        message: str_at(quoted, "message"),
                        user_id: str_at(quoted, "user_id"),
                    })
                }
                _ => None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn post_with(props: Value, metadata: Value) -> Post {
        serde_json::from_value(json!({
            "id": "p", "create_at": 1, "user_id": "u", "channel_id": "c",
            "props": props, "metadata": metadata
        }))
        .unwrap()
    }

    #[test]
    fn reads_gitlab_style_attachments() {
        let post = post_with(
            json!({"override_username": "GitLab", "attachments": [{
                "color": "good", "title": "MR !42 fusionnée", "title_link": "https://gitlab/x/42",
                "text": "**fix** : export CSV", "fields": [{"title": "Pipeline", "value": 1234, "short": true}],
                "actions": [{"id": "ok", "name": "Valider", "type": "button", "style": "primary"}]
            }]}),
            json!({}),
        );
        let a = &attachments(&post)[0];
        assert_eq!(a.title, "MR !42 fusionnée");
        assert_eq!(a.fields[0].value, "1234");
        assert_eq!(a.actions[0].name, "Valider");
        assert_eq!(attachment_color(&a.color), Some("#3db887"));
        assert_eq!(override_username(&post).as_deref(), Some("GitLab"));
    }

    #[test]
    fn reads_embeds() {
        let post = post_with(
            json!({}),
            json!({
                "embeds": [
                    {"type": "opengraph", "url": "https://glpi-project.org",
                     "data": {"site_name": "GLPI", "title": "GLPI Project", "description": "ITSM",
                              "images": [{"secure_url": "https://glpi-project.org/logo.png"}]}},
                    {"type": "image", "url": "https://x/y.png"}
                ],
                "images": {"https://x/y.png": {"width": 800, "height": 600}}
            }),
        );
        let e = embeds(&post);
        assert!(matches!(&e[0], Embed::Link { title, image: Some(_), .. } if title == "GLPI Project"));
        assert_eq!(e[1], Embed::Image { url: "https://x/y.png".into(), width: 800, height: 600 });
    }

    #[test]
    fn code_events_are_taken_apart() {
        let merged = "[\\[glpi-project/glpi\\]](https://github.com/glpi-project/glpi) Pull request \
                      [#26406 Use the GLPI `HttpClient`](https://github.com/glpi-project/glpi/pull/26406) \
                      was merged by [AdrienClairembault](https://github.com/AdrienClairembault).";
        assert_eq!(
            code_event(merged),
            Some(CodeEvent {
                kind: CodeEventKind::PrMerged,
                title: "#26406 Use the GLPI HttpClient".into(),
                url: "https://github.com/glpi-project/glpi/pull/26406".into(),
                repo: "glpi".into(),
                author: "AdrienClairembault".into(),
            })
        );
        let kind = |before: &str, after: &str| {
            let message = format!("[\\[o/r\\]](https://x) {before} [#1 T](https://x/1) {after}");
            code_event(&message).map(|e| (e.kind, e.author))
        };
        assert_eq!(kind("New issue", "opened by @skita."), Some((CodeEventKind::IssueOpened, "skita".into())));
        assert_eq!(kind("Issue", "reopened by [h](https://x/h)."), Some((CodeEventKind::IssueReopened, "h".into())));
        assert_eq!(kind("Issue", "closed by @g."), Some((CodeEventKind::IssueClosed, "g".into())));
        assert_eq!(kind("New draft pull request", "was opened by @r."), Some((CodeEventKind::PrDraft, "r".into())));
        assert_eq!(kind("New pull request", "was opened by @r."), Some((CodeEventKind::PrOpened, "r".into())));
        assert_eq!(kind("Pull request", "was closed by @r."), Some((CodeEventKind::PrClosed, "r".into())));
        // Other shapes keep their plain rendering.
        assert_eq!(code_event("Updated plugins\nSecureScan 2.0.0"), None);
        assert_eq!(code_event("[\\[o/r\\]](https://x) New comment by [u](https://x/u) on [#1 T](https://x/1):"), None);
    }

    #[test]
    fn no_props_means_nothing() {
        let post = post_with(Value::Null, json!({}));
        assert!(attachments(&post).is_empty());
        assert!(embeds(&post).is_empty());
        assert!(override_username(&post).is_none());
    }
}
