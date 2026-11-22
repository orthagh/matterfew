//! Éditeur riche (bêta) : la mise en forme Markdown s'affiche pendant la
//! saisie (gras en gras, code sur fond teinté, liens soulignés…), les
//! marques (`**`, `_`, `` ` ``, `#`…) restent visibles mais estompées.
//! Le texte envoyé reste du Markdown : seul l'affichage change.
//!
//! Ici, seulement l'analyse : des plages d'octets et leur sorte. Les
//! couleurs sont choisies par l'interface.

use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Span {
    /// Marque de mise en forme, estompée.
    Marker,
    Bold,
    Italic,
    Strike,
    /// Code en ligne ou bloc de code.
    Code,
    Heading,
    Link,
    Quote,
    /// Puce ou numéro de liste.
    ListMarker,
    /// `@personne` ou `~canal`.
    Mention,
}

fn options() -> Options {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts
}

/// Plages mises en forme du texte saisi (elles peuvent se chevaucher).
pub fn spans(text: &str) -> Vec<(Range<usize>, Span)> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    // Longueur d'une suite du même caractère à partir de `at`.
    let run = |at: usize, c: u8| bytes[at.min(bytes.len())..].iter().take_while(|b| **b == c).count();
    let run_back = |end: usize, c: u8| bytes[..end.min(bytes.len())].iter().rev().take_while(|b| **b == c).count();
    let wrap = |out: &mut Vec<(Range<usize>, Span)>, range: Range<usize>, marker: u8, max: usize, kind: Span| {
        let open = run(range.start, marker).min(max);
        let close = run_back(range.end, marker).min(max);
        out.push((range.clone(), kind));
        if open > 0 && close > 0 && range.len() > open + close {
            out.push((range.start..range.start + open, Span::Marker));
            out.push((range.end - close..range.end, Span::Marker));
        }
    };
    for (event, range) in Parser::new_ext(text, options()).into_offset_iter() {
        match event {
            Event::Start(Tag::Strong) => {
                let marker = bytes.get(range.start).copied().unwrap_or(b'*');
                wrap(&mut out, range, marker, 2, Span::Bold);
            }
            Event::Start(Tag::Emphasis) => {
                let marker = bytes.get(range.start).copied().unwrap_or(b'_');
                wrap(&mut out, range, marker, 1, Span::Italic);
            }
            Event::Start(Tag::Strikethrough) => wrap(&mut out, range, b'~', 2, Span::Strike),
            Event::Code(_) => wrap(&mut out, range, b'`', usize::MAX, Span::Code),
            Event::Start(Tag::CodeBlock(_)) => {
                out.push((range.clone(), Span::Code));
                // Lignes de clôture (```lang et ```) estompées.
                let block = &text[range.clone()];
                if block.starts_with("```") || block.starts_with("~~~") {
                    let first = block.find('\n').map_or(block.len(), |i| i + 1);
                    out.push((range.start..range.start + first, Span::Marker));
                    let trimmed = block.trim_end_matches('\n');
                    if trimmed.len() > first
                        && let Some(last) = trimmed.rfind('\n')
                        && (trimmed[last + 1..].starts_with("```") || trimmed[last + 1..].starts_with("~~~"))
                    {
                        out.push((range.start + last + 1..range.start + trimmed.len(), Span::Marker));
                    }
                }
            }
            Event::Start(Tag::Heading { .. }) => {
                let hashes = run(range.start, b'#');
                out.push((range.clone(), Span::Heading));
                if hashes > 0 {
                    let marker_end = (range.start + hashes + 1).min(range.end);
                    out.push((range.start..marker_end, Span::Marker));
                }
            }
            Event::Start(Tag::Link { .. }) => {
                let link = &text[range.clone()];
                // `[texte](adresse)` : le texte en lien, le reste estompé.
                if link.starts_with('[')
                    && let Some(mid) = link.find("](")
                {
                    out.push((range.start + 1..range.start + mid, Span::Link));
                    out.push((range.start..range.start + 1, Span::Marker));
                    out.push((range.start + mid..range.end, Span::Marker));
                } else {
                    out.push((range, Span::Link));
                }
            }
            Event::Start(Tag::BlockQuote(_)) => {
                out.push((range.clone(), Span::Quote));
                // Chaque « > » en début de ligne.
                let mut at = range.start;
                for line in text[range].split_inclusive('\n') {
                    let indent = line.len() - line.trim_start().len();
                    if line.trim_start().starts_with('>') {
                        out.push((at + indent..at + indent + 1, Span::Marker));
                    }
                    at += line.len();
                }
            }
            Event::Start(Tag::Item) => {
                let item = &text[range.clone()];
                let marker = item.find(|c: char| c.is_whitespace()).unwrap_or(0);
                if marker > 0 {
                    out.push((range.start..range.start + marker, Span::ListMarker));
                }
            }
            Event::TaskListMarker(_) => out.push((range, Span::ListMarker)),
            Event::Text(_) => mentions(text, range, &mut out),
            Event::End(TagEnd::Paragraph) => {}
            _ => {}
        }
    }
    out
}

/// `@nom` et `~canal` dans un morceau de texte courant.
fn mentions(text: &str, range: Range<usize>, out: &mut Vec<(Range<usize>, Span)>) {
    let part = &text[range.clone()];
    let mut chars = part.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c != '@' && c != '~' {
            continue;
        }
        let before = part[..i].chars().next_back();
        if before.is_some_and(|b| b.is_alphanumeric()) {
            continue;
        }
        let mut end = i + 1;
        while let Some(&(j, n)) = chars.peek() {
            if n.is_alphanumeric() || matches!(n, '.' | '_' | '-') {
                end = j + n.len_utf8();
                chars.next();
            } else {
                break;
            }
        }
        // Le point final d'une phrase n'appartient pas au nom.
        while end > i + 1 && part[..end].ends_with('.') {
            end -= 1;
        }
        if end > i + 1 {
            out.push((range.start + i..range.start + end, Span::Mention));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(&str, Span)> {
        spans(text).into_iter().map(|(r, k)| (&text[r], k)).collect()
    }

    #[test]
    fn gras_italique_code_et_marques() {
        let k = kinds("un **gras** et _ital_ puis `code` ~~non~~");
        assert!(k.contains(&("**gras**", Span::Bold)));
        assert!(k.contains(&("**", Span::Marker)));
        assert!(k.contains(&("_ital_", Span::Italic)));
        assert!(k.contains(&("`code`", Span::Code)));
        assert!(k.contains(&("~~non~~", Span::Strike)));
    }

    #[test]
    fn liens_titres_listes_citations_mentions() {
        let k = kinds("### Titre\n\n> cité\n\n- [doc](https://x.org) pour @lea.b.\n");
        assert!(k.contains(&("### ", Span::Marker)));
        assert!(k.contains(&("doc", Span::Link)));
        assert!(k.contains(&("](https://x.org)", Span::Marker)));
        assert!(k.contains(&(">", Span::Marker)));
        assert!(k.contains(&("-", Span::ListMarker)));
        assert!(k.contains(&("@lea.b", Span::Mention)));
    }

    #[test]
    fn bloc_de_code_et_texte_inacheve() {
        let k = kinds("```sql\nSELECT 1;\n```");
        assert!(k.contains(&("```sql\n", Span::Marker)));
        assert!(k.contains(&("```", Span::Marker)));
        // Saisie en cours : aucune plage invalide.
        for text in ["**gr", "[lien](", "```", "> ", "# ", "- ", "~~a"] {
            for (r, _) in spans(text) {
                assert!(r.start <= r.end && r.end <= text.len(), "{text:?}");
            }
        }
    }
}
