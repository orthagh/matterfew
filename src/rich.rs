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
    fn no_props_means_nothing() {
        let post = post_with(Value::Null, json!({}));
        assert!(attachments(&post).is_empty());
        assert!(embeds(&post).is_empty());
        assert!(override_username(&post).is_none());
    }
}
