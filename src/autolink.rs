//! Liens automatiques, comme le plugin Mattermost « Autolink » : un motif
//! (expression régulière) et un modèle Markdown (`[${id}](https://…/${id})`).
//!
//! Les règles viennent de la configuration du plugin sur le serveur (lisible
//! seulement par un administrateur) et du fichier `autolinks.json` placé à
//! côté de `ui.json` (`%APPDATA%\mm-gpui\`) :
//!
//! ```json
//! [
//!   { "Name": "Tickets", "Pattern": "(?P<id>TK-\\d+)", "Template": "[${id}](https://support.exemple.org/${id})",
//!     "Scope": ["equipe/support"] }
//! ]
//! ```
//!
//! Même format que le plugin : on peut recopier sa configuration telle quelle.
//! `Scope` limite une règle à des canaux (`équipe/canal`) ou à une équipe
//! entière (`équipe`) ; sans `Scope`, elle s'applique partout.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Règle telle qu'écrite dans la configuration du plugin (ou `ui.json`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RuleDef {
    #[serde(default, alias = "Name")]
    pub name: String,
    #[serde(default, alias = "Pattern")]
    pub pattern: String,
    #[serde(default, alias = "Template")]
    pub template: String,
    #[serde(default, alias = "Disabled")]
    pub disabled: bool,
    /// Par défaut, le motif doit commencer et finir sur une limite de mot.
    #[serde(default, alias = "DisableNonWordPrefix")]
    pub disable_non_word_prefix: bool,
    #[serde(default, alias = "DisableNonWordSuffix")]
    pub disable_non_word_suffix: bool,
    /// `équipe/canal` ou `équipe` ; vide : partout.
    #[serde(default, alias = "Scope")]
    pub scope: Vec<String>,
}

struct Rule {
    re: Regex,
    template: String,
    prefix: bool,
    suffix: bool,
    scope: Vec<String>,
}

static RULES: RwLock<Vec<Rule>> = RwLock::new(Vec::new());

/// Équipe et canal (noms courts, ceux des liens) de chaque canal connu.
static PLACES: RwLock<Option<HashMap<String, (String, String)>>> = RwLock::new(None);

/// Retient l'équipe et le nom d'un canal, pour les règles limitées à des canaux.
pub fn set_place(channel_id: &str, team: &str, channel: &str) {
    if let Ok(mut places) = PLACES.write() {
        places.get_or_insert_with(HashMap::new).insert(channel_id.to_string(), (team.to_string(), channel.to_string()));
    }
}

/// La règle s'applique-t-elle dans ce canal ?
fn in_scope(scope: &[String], channel_id: Option<&str>) -> bool {
    if scope.is_empty() {
        return true;
    }
    let Some(id) = channel_id else { return false };
    let Ok(places) = PLACES.read() else { return false };
    let Some((team, channel)) = places.as_ref().and_then(|p| p.get(id)) else { return false };
    scope.iter().any(|s| match s.split_once('/') {
        Some((t, c)) => t.eq_ignore_ascii_case(team) && c.eq_ignore_ascii_case(channel),
        None => s.eq_ignore_ascii_case(team),
    })
}

/// Fichier des règles locales : `%APPDATA%\mm-gpui\autolinks.json`.
pub fn local_file() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("mm-gpui").join("autolinks.json"))
}

/// Règles du fichier local (aucune s'il manque ou s'il est illisible).
pub fn load_local() -> Vec<RuleDef> {
    local_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Remplace les règles actives (les motifs invalides sont ignorés).
pub fn set_rules(defs: &[RuleDef]) -> usize {
    let rules: Vec<Rule> = defs
        .iter()
        .filter(|d| !d.disabled && !d.pattern.is_empty())
        .filter_map(|d| {
            Some(Rule {
                re: Regex::new(&d.pattern).ok()?,
                template: d.template.clone(),
                prefix: !d.disable_non_word_prefix,
                suffix: !d.disable_non_word_suffix,
                scope: d.scope.clone(),
            })
        })
        .collect();
    let n = rules.len();
    if let Ok(mut current) = RULES.write() {
        *current = rules;
    }
    n
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Découpe un texte d'un message du canal `channel_id` :
/// (texte, false) ou (Markdown du lien, true).
pub fn apply(text: &str, channel_id: Option<&str>) -> Vec<(String, bool)> {
    let Ok(rules) = RULES.read() else { return vec![(text.to_string(), false)] };
    if rules.is_empty() {
        return vec![(text.to_string(), false)];
    }
    // Les correspondances de toutes les règles, sans chevauchement (la première règle gagne).
    let mut found: Vec<(usize, usize, String)> = Vec::new();
    for rule in rules.iter().filter(|r| in_scope(&r.scope, channel_id)) {
        for caps in rule.re.captures_iter(text) {
            let m = caps.get(0).expect("correspondance");
            if m.is_empty() || found.iter().any(|(s, e, _)| m.start() < *e && *s < m.end()) {
                continue;
            }
            if rule.prefix && text[..m.start()].chars().next_back().is_some_and(is_word) {
                continue;
            }
            if rule.suffix && text[m.end()..].chars().next().is_some_and(is_word) {
                continue;
            }
            let mut link = String::new();
            caps.expand(&rule.template, &mut link);
            found.push((m.start(), m.end(), link));
        }
    }
    found.sort_by_key(|f| f.0);
    let mut out = Vec::new();
    let mut last = 0;
    for (start, end, link) in found {
        if last < start {
            out.push((text[last..start].to_string(), false));
        }
        out.push((link, true));
        last = end;
    }
    if last < text.len() {
        out.push((text[last..].to_string(), false));
    }
    out
}

/// Règles du plugin dans la configuration du serveur (`PluginSettings`).
pub fn from_server_config(config: &serde_json::Value) -> Vec<RuleDef> {
    let plugins = config.pointer("/PluginSettings/Plugins");
    let links = plugins
        .and_then(|p| p.get("mattermost-autolink").or_else(|| p.get("autolink")))
        .and_then(|a| a.get("links"));
    links.and_then(|l| serde_json::from_value(l.clone()).ok()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les règles sont globales : un test à la fois les modifie.
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn regles_teclib() {
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let defs: Vec<RuleDef> = serde_json::from_str(r##"[
            {"Name": "github_agent", "Pattern": "(#)(?P<github_id>\\d+)",
             "Template": "[#${github_id}](https://github.com/glpi-project/glpi-agent/issues/${github_id})",
             "Scope": ["teclib/glpi-agent-and-inventory"]},
            {"Name": "github_glpi", "Pattern": "(#)(?P<github_id>\\d+)",
             "Template": "[#${github_id}](https://github.com/glpi-project/glpi/issues/${github_id})",
             "Scope": ["teclib/GLPI", "teclib/ProServices"]},
            {"Name": "github_glpi_advisory", "Pattern": "(GHSA-(?P<b1>\\w{4})-?(?P<b2>\\w{4})-?(?P<b3>\\w{4}))",
             "Template": "[GHSA-${b1}-${b2}-${b3}](https://github.com/glpi-project/glpi/security/advisories/GHSA-${b1}-${b2}-${b3})",
             "Scope": ["teclib/GLPI"]},
            {"Name": "github_services", "Pattern": "(#)(?P<github_id>\\d+)",
             "Template": "[#${github_id}](https://github.com/glpi-network/services/issues/${github_id})",
             "Scope": ["teclib/services-dev"]},
            {"Name": "support", "Pattern": "(!)(?P<ticket_id>\\d+)",
             "Template": "[!${ticket_id}](https://support.teclib.com/front/ticket.form.php?id=${ticket_id})"}
        ]"##).unwrap();
        assert_eq!(set_rules(&defs), 5);
        set_place("ch-glpi", "teclib", "glpi");
        set_place("ch-services", "teclib", "services-dev");
        set_place("ch-autre", "teclib", "town-square");
        let link = |text: &str, ch: &str| apply(text, Some(ch)).into_iter().filter(|(_, l)| *l).map(|(t, _)| t).collect::<Vec<_>>();
        assert_eq!(link("voir #1234", "ch-glpi"), vec!["[#1234](https://github.com/glpi-project/glpi/issues/1234)"]);
        assert_eq!(link("voir #1234", "ch-services"), vec!["[#1234](https://github.com/glpi-network/services/issues/1234)"]);
        assert!(link("voir #1234", "ch-autre").is_empty());
        assert_eq!(
            link("GHSA-abcd-1234-wxyz", "ch-glpi"),
            vec!["[GHSA-abcd-1234-wxyz](https://github.com/glpi-project/glpi/security/advisories/GHSA-abcd-1234-wxyz)"]
        );
        // Le ticket de support : partout.
        assert_eq!(link("ticket !4821", "ch-autre"), vec!["[!4821](https://support.teclib.com/front/ticket.form.php?id=4821)"]);
        set_rules(&[]);
    }

    #[test]
    fn liens_du_plugin() {
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let defs: Vec<RuleDef> = serde_json::from_str(
            r#"[{"Name":"Tickets","Pattern":"(?P<id>TK-\\d+)","Template":"[${id}](https://support.ex/${id})"}]"#,
        )
        .unwrap();
        assert_eq!(set_rules(&defs), 1);
        let parts = apply("voir TK-12 et XTK-3, TK-4.", None);
        assert_eq!(parts[1], ("[TK-12](https://support.ex/TK-12)".to_string(), true));
        // « XTK-3 » : pas de limite de mot avant.
        assert!(!parts.iter().any(|(t, l)| *l && t.contains("TK-3")));
        assert!(parts.iter().any(|(t, l)| *l && t.contains("TK-4")));
        set_rules(&[]);
        assert_eq!(apply("TK-1", None), vec![("TK-1".to_string(), false)]);
    }

    #[test]
    fn portee_par_canal() {
        // Les règles du serveur de l'utilisateur : même motif, dépôts différents.
        set_place("c-agent", "teclib", "glpi-agent-and-inventory");
        set_place("c-glpi", "teclib", "glpi");
        let scope = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(in_scope(&scope(&["teclib/GLPI", "teclib/ProServices"]), Some("c-glpi")));
        assert!(!in_scope(&scope(&["teclib/GLPI"]), Some("c-agent")));
        assert!(in_scope(&scope(&["teclib"]), Some("c-agent")));
        assert!(in_scope(&[], None));
        assert!(!in_scope(&scope(&["teclib/GLPI"]), None));
    }

    #[test]
    fn configuration_du_serveur() {
        let cfg = serde_json::json!({"PluginSettings": {"Plugins": {"mattermost-autolink": {"links": [
            {"Name": "a", "Pattern": "x", "Template": "y"}
        ]}}}});
        assert_eq!(from_server_config(&cfg).len(), 1);
    }
}
