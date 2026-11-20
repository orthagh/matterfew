//! Liens automatiques, comme le plugin Mattermost « Autolink » : un motif
//! (expression régulière) et un modèle Markdown (`[${id}](https://…/${id})`).
//!
//! Les règles viennent de la configuration du plugin sur le serveur (lisible
//! seulement par un administrateur) et de `autolinks` dans `ui.json` :
//!
//! ```json
//! "autolinks": [
//!   { "Name": "Tickets", "Pattern": "(?P<id>TK-\\d+)", "Template": "[${id}](https://support.exemple.org/${id})" }
//! ]
//! ```
//!
//! Même format que le plugin : on peut recopier sa configuration telle quelle.

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
}

struct Rule {
    re: Regex,
    template: String,
    prefix: bool,
    suffix: bool,
}

static RULES: RwLock<Vec<Rule>> = RwLock::new(Vec::new());

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

/// Découpe un texte : (texte, false) ou (Markdown du lien, true).
pub fn apply(text: &str) -> Vec<(String, bool)> {
    let Ok(rules) = RULES.read() else { return vec![(text.to_string(), false)] };
    if rules.is_empty() {
        return vec![(text.to_string(), false)];
    }
    // Les correspondances de toutes les règles, sans chevauchement (la première règle gagne).
    let mut found: Vec<(usize, usize, String)> = Vec::new();
    for rule in rules.iter() {
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

    #[test]
    fn liens_du_plugin() {
        let defs: Vec<RuleDef> = serde_json::from_str(
            r#"[{"Name":"Tickets","Pattern":"(?P<id>TK-\\d+)","Template":"[${id}](https://support.ex/${id})"}]"#,
        )
        .unwrap();
        assert_eq!(set_rules(&defs), 1);
        let parts = apply("voir TK-12 et XTK-3, TK-4.");
        assert_eq!(parts[1], ("[TK-12](https://support.ex/TK-12)".to_string(), true));
        // « XTK-3 » : pas de limite de mot avant.
        assert!(!parts.iter().any(|(t, l)| *l && t.contains("TK-3")));
        assert!(parts.iter().any(|(t, l)| *l && t.contains("TK-4")));
        set_rules(&[]);
        assert_eq!(apply("TK-1"), vec![("TK-1".to_string(), false)]);
    }

    #[test]
    fn configuration_du_serveur() {
        let cfg = serde_json::json!({"PluginSettings": {"Plugins": {"mattermost-autolink": {"links": [
            {"Name": "a", "Pattern": "x", "Template": "y"}
        ]}}}});
        assert_eq!(from_server_config(&cfg).len(), 1);
    }
}
