//! Langue de l'interface : catalogues `locales/*.yml` (rust-i18n), choisie
//! d'après la langue du système, ou forcée par `MM_LANG` (ex. `MM_LANG=en`).

/// Choisit et applique la langue de l'interface (à appeler au démarrage).
pub fn init() {
    let forced = std::env::var("MM_LANG").ok();
    let system = sys_locale::get_locale();
    rust_i18n::set_locale(pick(forced.as_deref(), system.as_deref()));
}

/// Langue reconnue dans une étiquette (« fr-FR », « pt_BR », « en »…).
fn recognize(tag: &str) -> Option<&'static str> {
    let tag = tag.trim().to_ascii_lowercase().replace('_', "-");
    if tag.starts_with("pt") {
        Some("pt-BR")
    } else if tag.starts_with("fr") {
        Some("fr")
    } else if tag.starts_with("en") {
        Some("en")
    } else {
        None
    }
}

/// `MM_LANG` l'emporte s'il désigne une langue connue ; sinon la langue du
/// système ; à défaut, l'anglais.
pub fn pick(forced: Option<&str>, system: Option<&str>) -> &'static str {
    forced.and_then(recognize).or_else(|| system.and_then(recognize)).unwrap_or("en")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Langues disponibles (noms des fichiers de `locales/`).
    const LOCALES: [&str; 3] = ["en", "fr", "pt-BR"];

    #[test]
    fn choix_de_la_langue() {
        assert_eq!(pick(None, Some("fr-FR")), "fr");
        assert_eq!(pick(None, Some("pt-BR")), "pt-BR");
        assert_eq!(pick(None, Some("pt-PT")), "pt-BR");
        assert_eq!(pick(None, Some("de-DE")), "en");
        assert_eq!(pick(None, None), "en");
        assert_eq!(pick(Some("en"), Some("fr-FR")), "en");
        assert_eq!(pick(Some("pt_BR"), Some("fr-FR")), "pt-BR");
        assert_eq!(pick(Some("pt"), None), "pt-BR");
        assert_eq!(pick(Some("klingon"), Some("fr-CA")), "fr");
    }

    /// Clés complètes (« a.b.c ») des valeurs d'un catalogue YAML imbriqué
    /// tel que ceux de `locales/` (deux espaces par niveau).
    fn keys(yaml: &str) -> BTreeSet<String> {
        let mut path: Vec<String> = Vec::new();
        let mut out = BTreeSet::new();
        for line in yaml.lines() {
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let depth = (line.len() - trimmed.len()) / 2;
            let Some((key, value)) = trimmed.split_once(':') else { continue };
            path.truncate(depth);
            path.push(key.trim().to_string());
            if !value.trim().is_empty() {
                out.insert(path.join("."));
                path.pop();
            }
        }
        out
    }

    fn catalog(locale: &str) -> BTreeSet<String> {
        let path = format!("{}/locales/{locale}.yml", env!("CARGO_MANIFEST_DIR"));
        keys(&std::fs::read_to_string(&path).expect("catalogue lisible"))
    }

    #[test]
    fn catalogues_complets() {
        let en = catalog("en");
        assert!(en.len() > 100, "{}", en.len());
        for locale in LOCALES {
            let other = catalog(locale);
            let missing: Vec<_> = en.difference(&other).collect();
            let extra: Vec<_> = other.difference(&en).collect();
            assert!(missing.is_empty() && extra.is_empty(), "{locale} : manquantes {missing:?}, en trop {extra:?}");
        }
    }

    /// Chaque `t!("…")` du code désigne une clé du catalogue.
    #[test]
    fn cles_utilisees_definies() {
        let en = catalog("en");
        let re = regex::Regex::new(r#"t!\(\s*"([A-Za-z0-9_.]+)""#).unwrap();
        let mut stack = vec![std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))];
        let mut unknown = Vec::new();
        let mut used = 0;
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    for cap in re.captures_iter(&text) {
                        used += 1;
                        if !en.contains(&cap[1]) {
                            unknown.push(format!("{}: {}", path.display(), &cap[1]));
                        }
                    }
                }
            }
        }
        assert!(used > 100, "{used}");
        assert!(unknown.is_empty(), "{unknown:?}");
    }

    #[test]
    fn traduction() {
        assert_eq!(t!("sidebar.unreads", locale = "en"), "Unreads");
        assert_eq!(t!("sidebar.unreads", locale = "fr"), "Non lus");
        assert_eq!(t!("sidebar.unreads", locale = "pt-BR"), "Não lidas");
    }
}
