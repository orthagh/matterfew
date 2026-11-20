//! Préparation du Markdown Mattermost avant affichage.
//!
//! Le rendu lui-même est fait par `TextView::markdown` (GPUI Component).
//! Ici on ajoute seulement ce qui est propre à Mattermost :
//! - `:code:` → emoji Unicode ;
//! - `@utilisateur` → lien `mm-mention:` affiché dans la couleur des liens.
//!
//! Les transformations ne touchent que le texte courant : jamais le code
//! (en ligne ou en bloc), ni le texte des liens existants.

use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::emoji;

/// Préfixe des liens de mention, intercepté au clic par l'interface.
pub const MENTION_SCHEME: &str = "mm-mention:";

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

/// Markdown prêt pour l'affichage.
pub fn prepare(source: &str) -> String {
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
    for (chunk, is_mention) in split_mentions(&text) {
        if is_mention {
            let name = &chunk[1..];
            out.push_str(&format!("[{chunk}]({MENTION_SCHEME}{name})"));
        } else {
            out.push_str(chunk);
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
    fn emails_and_existing_links_untouched() {
        assert_eq!(prepare("écrire à a@b.fr"), "écrire à a@b.fr");
        assert_eq!(prepare("[@x](https://x.org)"), "[@x](https://x.org)");
    }

    #[test]
    fn plain_text_for_notifications() {
        assert_eq!(plain_text("**Salut** `toi` :wave:"), "Salut toi 👋");
    }
}
