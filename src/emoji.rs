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

#[cfg(test)]
mod tests {
    use super::*;

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
