//! Conversion des codes emoji Mattermost (`:smile:`) en caractères Unicode.

/// Emoji Unicode pour un nom Mattermost, ou `:nom:` s'il est inconnu
/// (emoji personnalisé du serveur, par exemple).
pub fn lookup(name: &str) -> String {
    emojis::get_by_shortcode(name)
        .map(|e| e.as_str().to_string())
        .unwrap_or_else(|| format!(":{name}:"))
}

/// Remplace tous les `:code:` reconnus dans un texte.
pub fn replace_shortcodes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(':') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-')));
        match end {
            Some(end) if end > 0 && after[end..].starts_with(':') => {
                let code = &after[..end];
                if let Some(e) = emojis::get_by_shortcode(code) {
                    out.push_str(e.as_str());
                    rest = &after[end + 1..];
                    continue;
                }
                out.push(':');
                rest = after;
            }
            _ => {
                out.push(':');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Emojis standard dont un nom court commence par `prefix` : (nom, caractère).
/// Les noms les plus courts d'abord (« smile » avant « smiley_cat »).
pub fn search(prefix: &str, limit: usize) -> Vec<(String, String)> {
    let prefix = prefix.to_lowercase();
    let mut found: Vec<(String, String)> = emojis::iter()
        .flat_map(|e| e.shortcodes().map(move |code| (code.to_string(), e.as_str().to_string())))
        .filter(|(code, _)| code.starts_with(&prefix))
        .collect();
    found.sort_by(|a, b| a.0.len().cmp(&b.0.len()).then_with(|| a.0.cmp(&b.0)));
    found.dedup_by(|a, b| a.1 == b.1);
    found.truncate(limit);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_by_prefix() {
        let found = search("thumbs", 5);
        assert!(found.iter().any(|(code, glyph)| code == "thumbsup" && glyph == "👍"));
        assert!(search("zzzz", 5).is_empty());
    }

    #[test]
    fn replaces_known_codes() {
        assert_eq!(replace_shortcodes("salut :smile: !"), "salut 😄 !");
        assert_eq!(replace_shortcodes(":+1::+1:"), "👍👍");
    }

    #[test]
    fn keeps_unknown_and_plain_colons() {
        assert_eq!(replace_shortcodes("heure 12:30"), "heure 12:30");
        assert_eq!(replace_shortcodes(":perso_glpi:"), ":perso_glpi:");
        assert_eq!(replace_shortcodes("a: b :c"), "a: b :c");
    }
}
