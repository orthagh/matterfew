//! Mise à jour de l'application.
//!
//! Chaque release publie `latest.json` (version, notes, et par plateforme :
//! adresse du paquet, signature minisign, format). Selon l'installation :
//! - installateur Windows (NSIS), `.app` macOS, AppImage : téléchargement,
//!   vérification de la signature, installation puis redémarrage
//!   (`cargo-packager-updater`) ;
//! - flatpak : mis à jour par flatpak lui-même (dépôt sur GitHub Pages) ;
//! - snap, `.deb`, exécutable seul : simple avis, avec un lien.
//!
//! Tout est bloquant : à appeler depuis un fil à part.

use std::time::Duration;

use anyhow::{Context as _, Result};
use serde::Deserialize;

/// Manifeste de la dernière release.
pub const MANIFEST_URL: &str = "https://github.com/orthagh/matterfew/releases/latest/download/latest.json";

/// Page d'une release (notes, paquets).
pub fn release_page(version: &str) -> String {
    format!("https://github.com/orthagh/matterfew/releases/tag/v{version}")
}

/// Clé publique minisign des paquets (sortie de `cargo packager signer
/// generate`, identifiant 6098B3E00CB601C7). Vide : aucune installation
/// automatique, l'avis seulement.
pub const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDYwOThCM0UwMENCNjAxQzcKUldUSEFiWU00TE9ZWUU2RkRxQjMxN0NoR29DbHFkczYyZzI5MDVZUml2aGMvd0E4a1RORjdleFIK";

/// Comment l'application a été installée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Install {
    Nsis,
    App,
    AppImage,
    Flatpak,
    Snap,
    /// `.deb`, exécutable seul, compilation locale…
    Manual,
}

impl Install {
    /// Peut se remplacer lui-même (et une clé de vérification est connue).
    pub fn self_updating(self) -> bool {
        matches!(self, Install::Nsis | Install::App | Install::AppImage) && !PUBKEY.is_empty()
    }
}

/// Type d'installation en cours.
pub fn detect() -> Install {
    let env = |k: &str| std::env::var_os(k).is_some_and(|v| !v.is_empty());
    let exe = std::env::current_exe().unwrap_or_default();
    detect_from(cfg!(windows), cfg!(target_os = "macos"), &exe, env("APPIMAGE"), env("SNAP"), env("FLATPAK_ID"))
}

fn detect_from(
    windows: bool,
    macos: bool,
    exe: &std::path::Path,
    appimage: bool,
    snap: bool,
    flatpak: bool,
) -> Install {
    if flatpak {
        Install::Flatpak
    } else if snap {
        Install::Snap
    } else if appimage {
        Install::AppImage
    } else if macos {
        // …/Matterfew.app/Contents/MacOS/matterfew
        let in_bundle = exe.ancestors().nth(3).is_some_and(|b| b.extension().is_some_and(|e| e == "app"));
        if in_bundle { Install::App } else { Install::Manual }
    } else if windows {
        // Compilation locale (target\…) : pas un exécutable installé.
        let dev = exe.to_string_lossy().split(['/', '\\']).any(|c| c == "target");
        if dev { Install::Manual } else { Install::Nsis }
    } else {
        Install::Manual
    }
}

/// Nouvelle version publiée.
#[derive(Debug, Clone, PartialEq)]
pub struct Available {
    pub version: String,
    pub notes: String,
    /// Paquet installable ici (plateforme présente dans le manifeste et
    /// installation capable de se remplacer).
    pub installable: bool,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    version: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    platforms: std::collections::HashMap<String, serde_json::Value>,
}

/// Clé de plateforme du manifeste (« windows-x86_64 »…).
fn platform_key() -> String {
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x86_64" };
    format!("{os}-{arch}")
}

/// Lit le manifeste : une version plus récente que `current` ?
fn parse(json: &str, current: &str, install: Install) -> Result<Option<Available>> {
    let manifest: Manifest = serde_json::from_str(json).context("latest.json illisible")?;
    let latest = semver::Version::parse(manifest.version.trim_start_matches('v'))?;
    if latest <= semver::Version::parse(current)? {
        return Ok(None);
    }
    Ok(Some(Available {
        version: latest.to_string(),
        notes: manifest.notes,
        installable: install.self_updating() && manifest.platforms.contains_key(&platform_key()),
    }))
}

fn http() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent(concat!("matterfew/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()?)
}

/// Une version plus récente est-elle publiée ?
pub fn check() -> Result<Option<Available>> {
    let resp = http()?.get(MANIFEST_URL).send()?;
    // Pas encore de manifeste publié : rien à proposer.
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let json = resp.error_for_status()?.text()?;
    parse(&json, env!("CARGO_PKG_VERSION"), detect())
}

/// Paquet téléchargé et vérifié, prêt à installer.
pub struct Prepared {
    update: cargo_packager_updater::Update,
    bytes: Vec<u8>,
}

/// Télécharge la nouvelle version ; `progress(reçus, total)`. La signature
/// est vérifiée ici : un paquet altéré est refusé.
pub fn download(progress: impl Fn(u64, Option<u64>)) -> Result<Prepared> {
    use cargo_packager_updater::{Config, UpdaterBuilder, WindowsConfig, WindowsUpdateInstallMode};
    let config = Config {
        endpoints: vec![MANIFEST_URL.parse()?],
        pubkey: PUBKEY.to_string(),
        windows: Some(WindowsConfig { install_mode: Some(WindowsUpdateInstallMode::Passive), ..Default::default() }),
    };
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))?;
    let updater = UpdaterBuilder::new(current, config).timeout(Duration::from_secs(600)).build()?;
    let update = updater.check()?.context("aucune mise à jour")?;
    let received = std::cell::Cell::new(0u64);
    let bytes = update.download_extended(
        |chunk, total| {
            received.set(received.get() + chunk as u64);
            progress(received.get(), total);
        },
        || {},
    )?;
    Ok(Prepared { update, bytes })
}

/// Installe la version téléchargée puis relance l'application. Sous
/// Windows, l'installateur relance lui-même l'application et ce processus
/// se termine ici ; ailleurs, l'appelant doit quitter ensuite.
pub fn apply(prepared: Prepared) -> Result<()> {
    prepared.update.install(prepared.bytes)?;
    relaunch(&prepared.update.extract_path)
}

#[cfg(target_os = "macos")]
fn relaunch(bundle: &std::path::Path) -> Result<()> {
    std::process::Command::new("open").arg("-n").arg(bundle).spawn()?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn relaunch(path: &std::path::Path) -> Result<()> {
    // AppImage : `extract_path` est le fichier AppImage remplacé.
    std::process::Command::new(path).spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn type_d_installation() {
        let d = |w, m, exe: &str, a, s, f| detect_from(w, m, Path::new(exe), a, s, f);
        assert_eq!(d(false, false, "/usr/bin/matterfew", false, false, true), Install::Flatpak);
        assert_eq!(d(false, false, "/snap/matterfew/1/bin/matterfew", false, true, false), Install::Snap);
        assert_eq!(d(false, false, "/tmp/.mount_x/matterfew", true, false, false), Install::AppImage);
        assert_eq!(d(false, false, "/usr/bin/matterfew", false, false, false), Install::Manual);
        assert_eq!(
            d(false, true, "/Applications/Matterfew.app/Contents/MacOS/matterfew", false, false, false),
            Install::App
        );
        assert_eq!(d(false, true, "/Users/a/matterfew", false, false, false), Install::Manual);
        assert_eq!(
            d(true, false, r"C:\Users\a\AppData\Local\Matterfew\matterfew.exe", false, false, false),
            Install::Nsis
        );
        assert_eq!(
            d(true, false, r"C:\src\mm-gpui\target\release\matterfew.exe", false, false, false),
            Install::Manual
        );
    }

    #[test]
    fn lecture_du_manifeste() {
        let key = platform_key();
        let json = format!(
            r#"{{"version": "v1.4.0", "notes": "Nouveautés", "pub_date": "2026-11-23T10:00:00Z",
                "platforms": {{"{key}": {{"url": "https://x/p", "signature": "s", "format": "nsis"}}}}}}"#
        );
        let found = parse(&json, "1.3.9", Install::Manual).unwrap().unwrap();
        assert_eq!(found.version, "1.4.0");
        assert_eq!(found.notes, "Nouveautés");
        assert!(!found.installable, "installation manuelle : avis seulement");
        assert_eq!(parse(&json, "1.4.0", Install::Nsis).unwrap(), None);
        assert_eq!(parse(&json, "2.0.0", Install::Nsis).unwrap(), None);
        assert!(parse("pas du json", "1.0.0", Install::Nsis).is_err());
    }
}
