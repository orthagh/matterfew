//! Reprise des données de l'ancien nom de l'application (« mm-gpui ») après
//! son renommage en Matterfew : réglages, cache, session du navigateur
//! intégré, et, sous Windows, l'ancien raccourci du menu Démarrer.
//! Le jeton du coffre du système est repris à part (voir `credentials`).

use std::path::Path;

/// Ancien nom des dossiers et du service du coffre.
pub const LEGACY: &str = "mm-gpui";
/// Nouveau nom.
pub const NAME: &str = "matterfew";

/// À appeler au démarrage, avant toute lecture des réglages.
pub fn run() {
    let mut bases = Vec::new();
    for base in [dirs::config_dir(), dirs::data_local_dir(), dirs::cache_dir()].into_iter().flatten() {
        if !bases.contains(&base) {
            bases.push(base);
        }
    }
    for base in bases {
        move_dir(&base);
    }
    #[cfg(windows)]
    windows_cleanup();
}

/// `base/mm-gpui` devient `base/matterfew`, sauf si le nouveau dossier existe
/// déjà (rien n'est écrasé). Un dossier encore ouvert par une ancienne
/// instance reste en place : la reprise se fera au lancement suivant.
fn move_dir(base: &Path) -> bool {
    let (old, new) = (base.join(LEGACY), base.join(NAME));
    old.is_dir() && !new.exists() && std::fs::rename(&old, &new).is_ok()
}

/// Ancien raccourci du menu Démarrer et ancienne identité des notifications.
#[cfg(windows)]
fn windows_cleanup() {
    if let Some(data) = dirs::data_dir() {
        let lnk = data.join(r"Microsoft\Windows\Start Menu\Programs").join("Mattermost (mm-gpui).lnk");
        let _ = std::fs::remove_file(lnk);
    }
    let _ = windows_registry::CURRENT_USER.remove_tree(r"Software\Classes\AppUserModelId\MmGpui.Client");
}

#[cfg(test)]
mod tests {
    use super::move_dir;

    #[test]
    fn dossier_repris_sans_ecraser() {
        let base = std::env::temp_dir().join(format!("matterfew-migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("mm-gpui")).unwrap();
        std::fs::write(base.join("mm-gpui").join("ui.json"), "{}").unwrap();
        assert!(move_dir(&base));
        assert!(base.join("matterfew").join("ui.json").is_file());
        assert!(!base.join("mm-gpui").exists());
        // Déjà fait : rien ne bouge.
        std::fs::create_dir_all(base.join("mm-gpui")).unwrap();
        assert!(!move_dir(&base));
        assert!(base.join("mm-gpui").exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
