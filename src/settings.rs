//! Préférences d'affichage locales (largeur de la barre latérale…),
//! enregistrées dans `%APPDATA%\matterfew\ui.json`.

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
    /// Liste des membres affichée à droite.
    pub show_members: bool,
    /// Dernières recherches, la plus récente d'abord.
    pub recent_searches: Vec<String>,
    /// Barre de mise en forme visible au-dessus de la zone de saisie.
    pub show_formatting: bool,
    /// Éditeur riche (bêta) : mise en forme visible pendant la saisie.
    pub rich_editor: bool,
    /// Rechercher les nouvelles versions de l'application.
    pub check_updates: bool,
    /// The Unreads section of the sidebar is collapsed.
    pub unreads_collapsed: bool,
    /// Serveur MCP local (lecture seule) pour les assistants.
    pub mcp_enabled: bool,
    pub mcp_port: u16,
    /// Jeton exigé par le serveur MCP (créé à la première activation).
    pub mcp_token: String,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            sidebar_width: 280.,
            last_team: None,
            last_channels: HashMap::new(),
            show_members: true,
            recent_searches: Vec::new(),
            show_formatting: false,
            rich_editor: false,
            check_updates: true,
            unreads_collapsed: false,
            mcp_enabled: false,
            mcp_port: 41814,
            mcp_token: String::new(),
        }
    }
}

fn file() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("matterfew").join("ui.json"))
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
