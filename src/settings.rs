//! Préférences d'affichage locales (largeur de la barre latérale…),
//! enregistrées dans `%APPDATA%\mm-gpui\ui.json`.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const SIDEBAR_MIN: f32 = 200.;
pub const SIDEBAR_MAX: f32 = 520.;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    pub sidebar_width: f32,
    /// Dernière équipe ouverte, rouverte au lancement.
    pub last_team: Option<String>,
    /// Dernier canal ouvert, par équipe.
    pub last_channels: HashMap<String, String>,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self { sidebar_width: 280., last_team: None, last_channels: HashMap::new() }
    }
}

fn file() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("mm-gpui").join("ui.json"))
}

impl UiSettings {
    pub fn load() -> Self {
        let mut s: Self = file()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        s.sidebar_width = s.sidebar_width.clamp(SIDEBAR_MIN, SIDEBAR_MAX);
        s
    }

    pub fn save(&self) {
        let Some(file) = file() else { return };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(file, text);
        }
    }
}
