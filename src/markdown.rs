//! Préparation du Markdown Mattermost avant affichage.
//!
//! Le rendu lui-même est fait par `TextView::markdown` (GPUI Component).
//! Ici on ajoute seulement ce qui est propre à Mattermost :
//! - `:code:` → emoji Unicode ;
//! - `@utilisateur` → lien `mm-mention:`, `#étiquette` → lien `mm-hashtag:`,
//!   affichés en pastilles par [`TagPlugin`] (voir `ui::view`).
//!
//! Les transformations ne touchent que le texte courant : jamais le code
//! (en ligne ou en bloc), ni le texte des liens existants.

use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::emoji;

/// Préfixe des liens de mention, intercepté au clic par l'interface.
pub const MENTION_SCHEME: &str = "mm-mention:";
/// Préfixe des liens d'étiquettes (`#sujet`), qui lancent une recherche.
pub const HASHTAG_SCHEME: &str = "mm-hashtag:";

fn options() -> Options {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts
}

/// Plages du source contenant du texte « transformable ».
/// Les événements texte consécutifs sont fusionnés : pulldown-cmark coupe
/// parfois le texte sur `_` ou `[`, ce qui casserait `:white_check_mark:`.
fn text_ranges(source: &str) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut in_link = 0usize;
    let mut in_code_block = false;
    for (event, range) in Parser::new_ext(source, options()).into_offset_iter() {
        match event {
            Event::Start(Tag::Link { .. }) | Event::Start(Tag::Image { .. }) => in_link += 1,
            Event::End(TagEnd::Link) | Event::End(TagEnd::Image) => in_link = in_link.saturating_sub(1),
            Event::Start(Tag::CodeBlock(_)) => in_code_block = true,
            Event::End(TagEnd::CodeBlock) => in_code_block = false,
            Event::Text(_) if in_link == 0 && !in_code_block => {
                // Le texte d'un événement correspond exactement au source,
                // sauf échappements (`\*`) : on vérifie avant de l'utiliser.
                match ranges.last_mut() {
                    Some(last) if last.end == range.start => last.end = range.end,
                    _ => ranges.push(range),
                }
            }
            _ => {}
        }
    }
    ranges
}

/// Préfixe des images d'emojis personnalisés (résolues par l'interface).
pub const CUSTOM_EMOJI_SCHEME: &str = "mm-emoji:";

/// Markdown prêt pour l'affichage.
pub fn prepare(source: &str) -> String {
    prepare_with(source, &[])
}

/// Comme [`prepare`], et `:nom:` d'un emoji personnalisé connu devient une
/// petite image en ligne (20 px), résolue par l'interface.
pub fn prepare_with(source: &str, custom_emojis: &[String]) -> String {
    let mut prepared = prepare_text(source);
    for name in custom_emojis {
        let code = format!(":{name}:");
        if prepared.contains(&code) {
            let tag = format!(r#"<img src="{CUSTOM_EMOJI_SCHEME}{name}" alt="{code}" width="20" height="20" />"#);
            prepared = prepared.replace(&code, &tag);
        }
    }
    prepared
}

fn prepare_text(source: &str) -> String {
    let mut out = String::with_capacity(source.len() + 16);
    let mut cursor = 0;
    for range in text_ranges(source) {
        out.push_str(&source[cursor..range.start]);
        out.push_str(&transform_text(&source[range.clone()]));
        cursor = range.end;
    }
    out.push_str(&source[cursor..]);
    out
}

fn transform_text(text: &str) -> String {
    let text = emoji::replace_shortcodes(text);
    let mut out = String::with_capacity(text.len());
    for (chunk, kind) in split_tokens(&text) {
        match kind {
            Token::Mention => out.push_str(&format!("[{chunk}]({MENTION_SCHEME}{})", &chunk[1..])),
            Token::Hashtag => out.push_str(&format!("[{chunk}]({HASHTAG_SCHEME}{})", &chunk[1..])),
            Token::Text => out.push_str(chunk),
        }
    }
    out
}

/// Étiquettes (`#sujet`, sans le `#`) présentes dans un message.
pub fn hashtags(source: &str) -> Vec<String> {
    text_ranges(source)
        .into_iter()
        .flat_map(|r| {
            split_tokens(&source[r])
                .into_iter()
                .filter(|(_, t)| *t == Token::Hashtag)
                .map(|(chunk, _)| chunk[1..].to_string())
                .collect::<Vec<_>>()
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Token {
    Text,
    Mention,
    Hashtag,
}

/// Découpe « salut @alice, voir #release-12 » en texte, mentions et étiquettes.
/// Une étiquette commence par une lettre et compte au moins 3 caractères,
/// comme dans Mattermost (« # Titre » et « #12 » n'en sont pas).
fn split_tokens(text: &str) -> Vec<(&str, Token)> {
    let mut out = Vec::new();
    for (chunk, is_mention) in split_mentions(text) {
        if is_mention {
            out.push((chunk, Token::Mention));
            continue;
        }
        let mut last = 0;
        let mut prev: Option<char> = None;
        let mut iter = chunk.char_indices().peekable();
        while let Some((i, c)) = iter.next() {
            let starts_word = prev.is_none_or(|p| !(p.is_alphanumeric() || matches!(p, '#' | '&' | '/' | '_' | '-')));
            if c == '#' && starts_word {
                let after = &chunk[i + 1..];
                let len = after
                    .find(|c: char| !(c.is_alphanumeric() || matches!(c, '.' | '_' | '-')))
                    .unwrap_or(after.len());
                let name = after[..len].trim_end_matches(['.', '-', '_']);
                if name.chars().next().is_some_and(char::is_alphabetic) && name.chars().count() >= 3 {
                    let end = i + 1 + name.len();
                    if last < i {
                        out.push((&chunk[last..i], Token::Text));
                    }
                    out.push((&chunk[i..end], Token::Hashtag));
                    last = end;
                    while iter.peek().is_some_and(|(j, _)| *j < end) {
                        iter.next();
                    }
                    prev = chunk[..end].chars().last();
                    continue;
                }
            }
            prev = Some(c);
        }
        if last < chunk.len() {
            out.push((&chunk[last..], Token::Text));
        }
    }
    out
}

/// Découpe « salut @alice ! » en [("salut ", false), ("@alice", true), (" !", false)].
fn split_mentions(text: &str) -> Vec<(&str, bool)> {
    let mut out = Vec::new();
    let mut last = 0;
    let mut prev: Option<char> = None;
    let mut iter = text.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        let starts_word = prev.is_none_or(|p| !(p.is_alphanumeric() || matches!(p, '.' | '_' | '-' | '@')));
        if c == '@' && starts_word {
            let after = &text[i + 1..];
            let len = after
                .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
                .unwrap_or(after.len());
            // Un point (ou tiret) final appartient à la phrase, pas au nom.
            let name = after[..len].trim_end_matches(['.', '-', '_']);
            if !name.is_empty() {
                let end = i + 1 + name.len();
                if last < i {
                    out.push((&text[last..i], false));
                }
                out.push((&text[i..end], true));
                last = end;
                // Avance jusqu'à la fin de la mention.
                while iter.peek().is_some_and(|(j, _)| *j < end) {
                    iter.next();
                }
                prev = text[..end].chars().last();
                continue;
            }
        }
        prev = Some(c);
    }
    if last < text.len() {
        out.push((&text[last..], false));
    }
    out
}

/// Texte brut d'un message, pour les notifications.
pub fn plain_text(source: &str) -> String {
    let mut out = String::new();
    for event in Parser::new_ext(source, options()) {
        match event {
            Event::Text(t) | Event::Code(t) => out.push_str(&t),
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            Event::End(TagEnd::Paragraph) | Event::End(TagEnd::Item) => out.push(' '),
            _ => {}
        }
    }
    emoji::replace_shortcodes(out.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emojis_outside_code_only() {
        assert_eq!(prepare("bravo :tada:"), "bravo 🎉");
        assert_eq!(prepare("`:tada:`"), "`:tada:`");
        assert_eq!(prepare("```\n:tada:\n```"), "```\n:tada:\n```");
        assert_eq!(prepare("ok :white_check_mark:"), "ok ✅");
    }

    #[test]
    fn mentions_become_links() {
        assert_eq!(
            prepare("merci @jean.dupont."),
            "merci [@jean.dupont](mm-mention:jean.dupont)."
        );
        assert_eq!(prepare("**@alice** !"), "**[@alice](mm-mention:alice)** !");
    }

    #[test]
    fn hashtags_become_links() {
        let out = prepare("Livré, voir #release-12. Et # Titre, #12, C#.");
        assert!(out.contains("[#release-12](mm-hashtag:release-12)."), "{out}");
        assert!(!out.contains("mm-hashtag:12"));
        assert!(!out.contains("(mm-hashtag:Titre)"));
        assert!(!out.contains("mm-hashtag:."));
        assert!(prepare("`#code`").contains("`#code`"));
    }

    #[test]
    fn emails_and_existing_links_untouched() {
        assert_eq!(prepare("écrire à a@b.fr"), "écrire à a@b.fr");
        assert_eq!(prepare("[@x](https://x.org)"), "[@x](https://x.org)");
    }

    #[test]
    fn custom_emojis_become_inline_images() {
        let out = prepare_with("bravo :glpi: !", &["glpi".to_string()]);
        assert_eq!(out, r#"bravo <img src="mm-emoji:glpi" alt=":glpi:" width="20" height="20" /> !"#);
        // Inconnu : laissé tel quel.
        assert_eq!(prepare_with(":autre:", &["glpi".to_string()]), ":autre:");
    }

    #[test]
    fn plain_text_for_notifications() {
        assert_eq!(plain_text("**Salut** `toi` :wave:"), "Salut toi 👋");
    }
}
