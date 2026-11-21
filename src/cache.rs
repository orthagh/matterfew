//! Cache local des derniers messages de chaque canal.
//!
//! Ouvrir un canal affiche aussitôt ses messages connus, le temps que le
//! serveur réponde ; sa réponse remplace ensuite l'affichage.
//!
//! Un fichier JSON par canal (derniers messages et leurs auteurs), dans
//! `%LOCALAPPDATA%\mm-gpui\cache\<serveur>\`. Taille bornée : au plus
//! [`MAX_POSTS`] messages par canal et [`MAX_BYTES`] au total (les canaux
//! ouverts le moins récemment sont retirés d'abord). Effacé à la déconnexion.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::api::{Post, User};

/// Messages gardés par canal.
pub const MAX_POSTS: usize = 60;
/// Taille totale du cache d'un serveur.
const MAX_BYTES: u64 = 40 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Entry {
    posts: Vec<Post>,
    users: Vec<User>,
}

/// Dossier du cache pour un serveur (`https://chat.exemple.org` → `chat.exemple.org`).
pub fn dir_for(server_url: &str) -> Option<PathBuf> {
    let host: String = server_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect();
    (!host.is_empty()).then(|| dirs::cache_dir().map(|d| d.join("mm-gpui").join("cache").join(host)))?
}

fn file(dir: &Path, channel_id: &str) -> Option<PathBuf> {
    channel_id.chars().all(|c| c.is_ascii_alphanumeric()).then(|| dir.join(format!("{channel_id}.json")))
}

/// Messages connus d'un canal (du plus ancien au plus récent) et leurs auteurs.
pub fn load(dir: &Path, channel_id: &str) -> Option<(Vec<Post>, Vec<User>)> {
    let bytes = std::fs::read(file(dir, channel_id)?).ok()?;
    let entry: Entry = serde_json::from_slice(&bytes).ok()?;
    Some((entry.posts, entry.users))
}

/// Enregistre les derniers messages d'un canal (en arrière-plan : appel bloquant).
pub fn store(dir: &Path, channel_id: &str, posts: &[Post], users: &HashMap<String, User>) {
    let Some(path) = file(dir, channel_id) else { return };
    let start = posts.len().saturating_sub(MAX_POSTS);
    let posts: Vec<Post> = posts[start..].to_vec();
    let mut ids: Vec<&String> = posts.iter().map(|p| &p.user_id).collect();
    ids.sort();
    ids.dedup();
    let users: Vec<User> = ids.into_iter().filter_map(|id| users.get(id).cloned()).collect();
    let Ok(json) = serde_json::to_vec(&Entry { posts, users }) else { return };
    if std::fs::create_dir_all(dir).is_ok() {
        let _ = std::fs::write(path, json);
    }
}

/// Retire les canaux les plus anciens tant que le cache dépasse sa taille.
pub fn prune(dir: &Path) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = read
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            Some((meta.modified().ok()?, meta.len(), e.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort_by_key(|f| f.0);
    for (_, len, path) in files {
        if total <= MAX_BYTES {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= len;
        }
    }
}

/// Efface tout le cache d'un serveur.
pub fn clear(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aller_retour_et_limite() {
        let dir = std::env::temp_dir().join(format!("mm-gpui-cache-test-{}", std::process::id()));
        let posts: Vec<Post> = (0..80)
            .map(|i| Post { id: format!("p{i}"), user_id: "u1".into(), message: format!("m{i}"), ..Default::default() })
            .collect();
        let users = HashMap::from([(
            "u1".to_string(),
            serde_json::from_value::<User>(serde_json::json!({"id": "u1", "username": "lea"})).unwrap(),
        )]);
        store(&dir, "chan1", &posts, &users);
        let (back, who) = load(&dir, "chan1").unwrap();
        assert_eq!(back.len(), MAX_POSTS);
        assert_eq!(back.last().unwrap().id, "p79");
        assert_eq!(who[0].username, "lea");
        // Identifiant douteux : jamais de chemin construit avec.
        assert!(load(&dir, "../x").is_none());
        clear(&dir);
        assert!(load(&dir, "chan1").is_none());
    }

    #[test]
    fn dossier_par_serveur() {
        let d = dir_for("https://chat.exemple.org/").unwrap();
        assert!(d.ends_with("chat.exemple.org"));
    }
}
