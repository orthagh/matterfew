//! Mise en forme Markdown de la zone de saisie (barre d'outils et raccourcis),
//! comme l'éditeur du client web : on agit sur la sélection, ou sur les
//! lignes sélectionnées pour les titres, citations et listes.
//!
//! Fonctions pures (texte + sélection → texte + sélection), testées.

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Bold,
    Italic,
    Strike,
    Code,
    Link,
    Heading,
    Quote,
    Bullet,
    Numbered,
    CodeBlock,
}

impl Format {
    /// Nom affiché dans l'infobulle, avec le raccourci clavier.
    pub fn label(self) -> String {
        match self {
            Format::Bold => t!("editor.bold"),
            Format::Italic => t!("editor.italic"),
            Format::Strike => t!("editor.strike"),
            Format::Code => t!("editor.code"),
            Format::Link => t!("editor.link"),
            Format::Heading => t!("editor.heading"),
            Format::Quote => t!("editor.quote"),
            Format::Bullet => t!("editor.bullet"),
            Format::Numbered => t!("editor.numbered"),
            Format::CodeBlock => t!("editor.code_block"),
        }
        .to_string()
    }
}

/// Applique une mise en forme ; renvoie le nouveau texte et la sélection à
/// placer ensuite (en octets).
pub fn apply(text: &str, selection: Range<usize>, format: Format) -> (String, Range<usize>) {
    let sel = clamp(text, selection);
    match format {
        Format::Bold => wrap(text, sel, "**", "**"),
        Format::Italic => wrap(text, sel, "*", "*"),
        Format::Strike => wrap(text, sel, "~~", "~~"),
        Format::Code => wrap(text, sel, "`", "`"),
        Format::Link => link(text, sel),
        Format::CodeBlock => code_block(text, sel),
        Format::Heading => prefix_lines(text, sel, |_| "### ".to_string()),
        Format::Quote => prefix_lines(text, sel, |_| "> ".to_string()),
        Format::Bullet => prefix_lines(text, sel, |_| "- ".to_string()),
        Format::Numbered => prefix_lines(text, sel, |i| format!("{}. ", i + 1)),
    }
}

/// Sélection ramenée dans le texte et sur des limites de caractères.
fn clamp(text: &str, sel: Range<usize>) -> Range<usize> {
    let fix = |mut i: usize| {
        i = i.min(text.len());
        while !text.is_char_boundary(i) {
            i -= 1;
        }
        i
    };
    let (a, b) = (fix(sel.start), fix(sel.end));
    a.min(b)..a.max(b)
}

/// Entoure la sélection (ou la retire si elle est déjà entourée).
fn wrap(text: &str, sel: Range<usize>, open: &str, close: &str) -> (String, Range<usize>) {
    let before = &text[..sel.start];
    let after = &text[sel.end..];
    if before.ends_with(open) && after.starts_with(close) && !sel.is_empty() {
        let start = sel.start - open.len();
        let out = format!("{}{}{}", &text[..start], &text[sel.clone()], &after[close.len()..]);
        return (out, start..start + sel.len());
    }
    let out = format!("{before}{open}{}{close}{after}", &text[sel.clone()]);
    let start = sel.start + open.len();
    (out, start..start + sel.len())
}

/// `[texte](url)` : sélectionne « url » pour la remplacer tout de suite.
fn link(text: &str, sel: Range<usize>) -> (String, Range<usize>) {
    let label = &text[sel.clone()];
    let out = format!("{}[{label}](url){}", &text[..sel.start], &text[sel.end..]);
    if label.is_empty() {
        let at = sel.start + 1;
        (out, at..at)
    } else {
        let url = sel.start + label.len() + 3;
        (out, url..url + 3)
    }
}

fn code_block(text: &str, sel: Range<usize>) -> (String, Range<usize>) {
    let before = &text[..sel.start];
    let lead = if before.is_empty() || before.ends_with('\n') { "" } else { "\n" };
    let inner = &text[sel.clone()];
    let out = format!("{before}{lead}```\n{inner}\n```{}", &text[sel.end..]);
    let start = sel.start + lead.len() + 4;
    (out, start..start + inner.len())
}

/// Préfixe chaque ligne touchée par la sélection ; si toutes l'ont déjà,
/// le retire (bascule).
fn prefix_lines(text: &str, sel: Range<usize>, prefix: impl Fn(usize) -> String) -> (String, Range<usize>) {
    let start = text[..sel.start].rfind('\n').map_or(0, |i| i + 1);
    let end = text[sel.end..].find('\n').map_or(text.len(), |i| sel.end + i);
    let block = &text[start..end];
    let lines: Vec<&str> = block.split('\n').collect();

    let has_prefix = |i: usize, line: &str| {
        let p = prefix(i);
        // Listes numérotées : n'importe quel numéro compte.
        if p.ends_with(". ") && p.trim_end_matches(". ").chars().all(|c| c.is_ascii_digit()) {
            let digits = line.chars().take_while(char::is_ascii_digit).count();
            digits > 0 && line[digits..].starts_with(". ")
        } else {
            line.starts_with(&p)
        }
    };
    let all_prefixed = lines.iter().enumerate().all(|(i, l)| has_prefix(i, l));

    let new_lines: Vec<String> = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            if all_prefixed {
                let cut = if prefix(i).ends_with(". ") && !prefix(i).starts_with('-') {
                    line.find(". ").map_or(0, |p| p + 2)
                } else {
                    prefix(i).len()
                };
                line[cut.min(line.len())..].to_string()
            } else {
                format!("{}{line}", prefix(i))
            }
        })
        .collect();
    let new_block = new_lines.join("\n");
    let out = format!("{}{new_block}{}", &text[..start], &text[end..]);
    (out, start..start + new_block.len())
}

/// Lien Markdown `[texte](adresse)` : crochets du texte et parenthèses de
/// l'adresse protégés pour que le lien reste entier.
pub fn markdown_link(text: &str, url: &str) -> String {
    let text = text.replace('[', r"\[").replace(']', r"\]");
    let url = url.replace('(', "%28").replace(')', "%29");
    format!("[{text}]({url})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_and_unwraps_selection() {
        let (t, s) = apply("un mot ici", 3..6, Format::Bold);
        assert_eq!(t, "un **mot** ici");
        assert_eq!(&t[s.clone()], "mot");
        let (t, s) = apply(&t, s, Format::Bold);
        assert_eq!(t, "un mot ici");
        assert_eq!(&t[s], "mot");
    }

    #[test]
    fn empty_selection_puts_cursor_inside() {
        let (t, s) = apply("a ", 2..2, Format::Italic);
        assert_eq!(t, "a **");
        assert_eq!(s, 3..3);
    }

    #[test]
    fn link_selects_url_placeholder() {
        let (t, s) = apply("voir doc", 5..8, Format::Link);
        assert_eq!(t, "voir [doc](url)");
        assert_eq!(&t[s], "url");
    }

    #[test]
    fn line_prefixes_toggle_on_every_selected_line() {
        let (t, s) = apply("un\ndeux\ntrois", 1..5, Format::Bullet);
        assert_eq!(t, "- un\n- deux\ntrois");
        let (t, _) = apply(&t, s, Format::Bullet);
        assert_eq!(t, "un\ndeux\ntrois");
        let (t, _) = apply("a\nb", 0..3, Format::Numbered);
        assert_eq!(t, "1. a\n2. b");
        let (t, _) = apply(&t, 0..t.len(), Format::Numbered);
        assert_eq!(t, "a\nb");
    }

    #[test]
    fn code_block_on_its_own_lines() {
        let (t, s) = apply("voici x = 1", 6..11, Format::CodeBlock);
        assert_eq!(t, "voici \n```\nx = 1\n```");
        assert_eq!(&t[s], "x = 1");
    }

    #[test]
    fn handles_accents_and_out_of_range() {
        let (t, _) = apply("éte", 0..99, Format::Code);
        assert_eq!(t, "`éte`");
    }

    #[test]
    fn lien_markdown() {
        use super::markdown_link;
        assert_eq!(markdown_link("la doc", "https://ex.org/a"), "[la doc](https://ex.org/a)");
        assert_eq!(markdown_link("[v2]", "https://ex.org/a_(b)"), r"[\[v2\]](https://ex.org/a_%28b%29)");
    }
}
