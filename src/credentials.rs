//! Mémorisation de la connexion.
//!
//! - L'adresse du serveur et le type de connexion sont enregistrés dans un
//!   petit fichier (`%APPDATA%\mm-gpui\config.json` sous Windows).
//! - Le jeton va dans le coffre du système : Gestionnaire d'identifiants
//!   Windows, Trousseau macOS ou Secret Service sous Linux. Il n'est
//!   jamais écrit en clair sur le disque.

use std::path::PathBuf;

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

const SERVICE: &str = "mm-gpui";

/// Nature du jeton mémorisé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AuthKind {
    /// Jeton de session (connexion par navigateur ou mot de passe) :
    /// il expire, et la déconnexion le révoque côté serveur.
    Session,
    /// Jeton d'accès personnel : ne pas le révoquer à la déconnexion.
    #[default]
    Token,
}

#[derive(Serialize, Deserialize)]
struct Config {
    server_url: String,
    #[serde(default)]
    auth: AuthKind,
}

pub struct Saved {
    pub server_url: String,
    pub token: String,
    pub kind: AuthKind,
}

fn dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("mm-gpui"))
}

fn read_config() -> Option<Config> {
    let dir = dir()?;
    if let Ok(text) = std::fs::read_to_string(dir.join("config.json")) {
        return serde_json::from_str(&text).ok();
    }
    // Ancien format (première version) : l'adresse seule.
    let url = std::fs::read_to_string(dir.join("server.txt")).ok()?;
    let url = url.trim();
    (!url.is_empty()).then(|| Config { server_url: url.to_string(), auth: AuthKind::Token })
}

fn entry(server_url: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, server_url).with_context(|| t!("errors.credential_store").to_string())
}

/// Connexion enregistrée, s'il y en a une.
pub fn load() -> Option<Saved> {
    let config = read_config()?;
    let token = entry(&config.server_url).ok()?.get_password().ok()?;
    Some(Saved { server_url: config.server_url, token, kind: config.auth })
}

pub fn save(server_url: &str, token: &str, kind: AuthKind) -> Result<()> {
    let dir = dir().with_context(|| t!("errors.config_dir").to_string())?;
    std::fs::create_dir_all(&dir)?;
    let config = Config { server_url: server_url.to_string(), auth: kind };
    std::fs::write(dir.join("config.json"), serde_json::to_string_pretty(&config)?)?;
    let _ = std::fs::remove_file(dir.join("server.txt"));
    entry(server_url)?.set_password(token).with_context(|| t!("errors.save_token").to_string())?;
    Ok(())
}

/// Oublie le jeton (l'adresse du serveur est conservée pour la prochaine fois).
pub fn forget(server_url: &str) {
    if let Ok(e) = entry(server_url) {
        let _ = e.delete_credential();
    }
}

/// Dernière adresse de serveur utilisée.
pub fn last_server() -> Option<String> {
    read_config().map(|c| c.server_url)
}
