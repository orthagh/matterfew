//! Correction orthographique de la saisie, par le correcteur intégré à
//! Windows (le même que celui de Word et Edge) : local, sans envoi du texte
//! à un service extérieur. Dictionnaire de la langue de l'interface, plus
//! l'anglais : un mot n'est signalé que si aucun des deux ne le connaît.
//!
//! Hors Windows, aucun mot n'est signalé.

// Hors Windows, une partie de ce module ne sert pas (correcteur de Windows,
// icônes de la barre des tâches).
#![cfg_attr(not(windows), allow(dead_code))]

use std::ops::Range;

/// Mot mal orthographié dans un texte.
#[derive(Debug, Clone, PartialEq)]
pub struct Misspelling {
    /// Position en octets dans le texte.
    pub range: Range<usize>,
    pub word: String,
    pub suggestions: Vec<String>,
}

/// Fautes encore à leur place dans le texte modifié (le mot n'a pas changé) :
/// elles restent soulignées en attendant la prochaine vérification.
pub fn still_valid(found: &[Misspelling], text: &str) -> Vec<Misspelling> {
    let word_char = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '\'' || c == '’');
    found
        .iter()
        .filter(|m| {
            text.get(m.range.clone()) == Some(m.word.as_str())
                && !word_char(text[..m.range.start].chars().next_back())
                && !word_char(text[m.range.end..].chars().next())
        })
        .cloned()
        .collect()
}

/// Faut-il ignorer ce mot ? (mention, étiquette, code, adresse, identifiant…)
pub fn skip(text: &str, range: &Range<usize>) -> bool {
    let word = &text[range.clone()];
    let before = text[..range.start].chars().next_back();
    let after = text[range.end..].chars().next();
    if matches!(before, Some('@' | '#' | '~' | ':' | '/' | '.' | '_' | '\\'))
        || matches!(after, Some(':' | '/' | '_' | '@'))
    {
        return true;
    }
    if word.chars().any(|c| c.is_ascii_digit() || c == '_') {
        return true;
    }
    // Dans du code entre accents graves.
    if text[..range.start].matches('`').count() % 2 == 1 {
        return true;
    }
    // Dans une adresse web (le « mot » entre espaces contient « :// » ou « www. »).
    let start = text[..range.start].rfind(char::is_whitespace).map_or(0, |i| i + 1);
    let end = text[range.end..].find(char::is_whitespace).map_or(text.len(), |i| range.end + i);
    let token = &text[start..end];
    token.contains("://") || token.starts_with("www.")
}

#[cfg(windows)]
mod imp {
    use super::{Misspelling, skip};
    use windows::Win32::Globalization::{ISpellChecker, ISpellCheckerFactory, ISpellingError, SpellCheckerFactory};
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    };
    use windows::core::{HSTRING, PWSTR};

    /// Correcteur de Windows (objets COM : à garder sur le fil de l'interface).
    /// Le premier dictionnaire est le principal (propositions, ajouts) ; les
    /// autres ne servent qu'à accepter un mot.
    pub struct Checker {
        inner: ISpellChecker,
        others: Vec<ISpellChecker>,
    }

    impl Checker {
        pub fn new() -> Option<Self> {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                let factory: ISpellCheckerFactory =
                    CoCreateInstance(&SpellCheckerFactory, None, CLSCTX_INPROC_SERVER).ok()?;
                let mut found: Vec<(&str, ISpellChecker)> = Vec::new();
                for language in super::languages(&rust_i18n::locale()) {
                    // Une seule variante par langue (fr-FR ou fr, en-US ou en-GB).
                    let base = language.split('-').next().unwrap_or(language);
                    if found.iter().any(|(b, _)| *b == base) {
                        continue;
                    }
                    let tag = HSTRING::from(language);
                    if factory.IsSupported(&tag).is_ok_and(|b| b.as_bool())
                        && let Ok(checker) = factory.CreateSpellChecker(&tag)
                    {
                        found.push((base, checker));
                    }
                }
                let mut checkers = found.into_iter().map(|(_, c)| c);
                let inner = checkers.next()?;
                Some(Self { inner, others: checkers.collect() })
            }
        }

        /// Mots inconnus du texte (au plus `limit`), avec des propositions.
        pub fn check(&self, text: &str, limit: usize) -> Vec<Misspelling> {
            let mut out = Vec::new();
            if text.trim().is_empty() {
                return out;
            }
            // Le correcteur compte en unités UTF-16 : table vers les octets.
            let mut utf16_to_byte = Vec::with_capacity(text.len() + 1);
            for (i, c) in text.char_indices() {
                for _ in 0..c.len_utf16() {
                    utf16_to_byte.push(i);
                }
            }
            utf16_to_byte.push(text.len());
            unsafe {
                let Ok(errors) = self.inner.Check(&HSTRING::from(text)) else { return out };
                loop {
                    let mut error: Option<ISpellingError> = None;
                    if errors.Next(&mut error).is_err() {
                        break;
                    }
                    let Some(error) = error else { break };
                    let (Ok(start), Ok(len)) = (error.StartIndex(), error.Length()) else { continue };
                    let (start, end) = (start as usize, (start + len) as usize);
                    let (Some(&a), Some(&b)) = (utf16_to_byte.get(start), utf16_to_byte.get(end)) else { continue };
                    let range = a..b;
                    if range.is_empty() || skip(text, &range) {
                        continue;
                    }
                    let word = text[range.clone()].to_string();
                    // Connu d'un autre dictionnaire (l'anglais) : pas une faute.
                    if self.others.iter().any(|c| known(c, &word)) {
                        continue;
                    }
                    let suggestions = self.suggest(&word, 4);
                    out.push(Misspelling { range, word, suggestions });
                    if out.len() >= limit {
                        break;
                    }
                }
            }
            out
        }

        /// Propositions des dictionnaires, à tour de rôle (principal d'abord) :
        /// une faute de frappe anglaise trouve aussi sa correction anglaise.
        fn suggest(&self, word: &str, limit: usize) -> Vec<String> {
            let lists: Vec<Vec<String>> = std::iter::once(&self.inner)
                .chain(&self.others)
                .map(|checker| suggestions(checker, word, limit))
                .collect();
            let mut out: Vec<String> = Vec::new();
            for rank in 0..limit {
                for s in lists.iter().filter_map(|l| l.get(rank)) {
                    if out.len() < limit && !out.contains(s) {
                        out.push(s.clone());
                    }
                }
            }
            out
        }

        /// Ajoute un mot au dictionnaire personnel de Windows.
        pub fn add(&self, word: &str) {
            unsafe {
                let _ = self.inner.Add(&HSTRING::from(word));
            }
        }
    }

    /// Le dictionnaire accepte-t-il ce mot ?
    fn known(checker: &ISpellChecker, word: &str) -> bool {
        unsafe {
            let Ok(errors) = checker.Check(&HSTRING::from(word)) else { return false };
            let mut error: Option<ISpellingError> = None;
            errors.Next(&mut error).is_ok() && error.is_none()
        }
    }

    /// Propositions d'un dictionnaire pour ce mot.
    fn suggestions(checker: &ISpellChecker, word: &str, limit: usize) -> Vec<String> {
        let mut out = Vec::new();
        unsafe {
            let Ok(list) = checker.Suggest(&HSTRING::from(word)) else { return out };
            loop {
                let mut item = [PWSTR::null()];
                let mut fetched = 0u32;
                if list.Next(&mut item, Some(&mut fetched)).is_err() || fetched == 0 {
                    break;
                }
                if let Ok(s) = item[0].to_string()
                    && s != word
                {
                    out.push(s);
                }
                CoTaskMemFree(Some(item[0].0 as *const _));
                if out.len() >= limit {
                    break;
                }
            }
        }
        out
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Misspelling;

    pub struct Checker;

    impl Checker {
        pub fn new() -> Option<Self> {
            None
        }
        pub fn check(&self, _: &str, _: usize) -> Vec<Misspelling> {
            Vec::new()
        }
        pub fn add(&self, _: &str) {}
    }
}

pub use imp::Checker;

/// Dictionnaires à essayer, par ordre de préférence : celui de la langue de
/// l'interface d'abord, puis l'anglais, puis le français.
#[cfg_attr(not(windows), allow(dead_code))]
fn languages(ui_locale: &str) -> Vec<&'static str> {
    let first: &[&str] = match ui_locale {
        "fr" => &["fr-FR", "fr"],
        "pt-BR" => &["pt-BR", "pt"],
        _ => &[],
    };
    let mut out = first.to_vec();
    for tag in ["en-US", "en-GB", "en", "fr-FR", "fr"] {
        if !out.contains(&tag) {
            out.push(tag);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::skip;

    #[test]
    fn mots_ignores() {
        let t = "voir @lea #sujet `codde` https://exemple.org/pagge foo_bar ok";
        let at = |w: &str| {
            let i = t.find(w).unwrap();
            i..i + w.len()
        };
        assert!(skip(t, &at("lea")));
        assert!(skip(t, &at("sujet")));
        assert!(skip(t, &at("codde")));
        assert!(skip(t, &at("pagge")));
        assert!(skip(t, &at("foo")));
        assert!(!skip(t, &at("voir")));
    }

    #[test]
    fn langue_du_dictionnaire() {
        assert_eq!(super::languages("en"), ["en-US", "fr-FR", "fr"]);
        assert_eq!(super::languages("fr"), ["fr-FR", "fr", "en-US"]);
        assert_eq!(super::languages("pt-BR")[0], "pt-BR");
    }

    /// Manuel : vérifie que le correcteur de Windows répond.
    #[test]
    #[ignore]
    fn correcteur_windows() {
        rust_i18n::set_locale("fr");
        let c = super::Checker::new().expect("correcteur");
        let found = c.check("Bonjour, je suis tres contant pour le meeting de tomorrow.", 8);
        assert!(found.iter().any(|m| m.word == "tres"), "{found:?}");
        // Français + anglais : les mots anglais ne sont pas des fautes.
        assert!(!found.iter().any(|m| m.word == "meeting" || m.word == "tomorrow"), "{found:?}");
        let found = c.check("We will ship it tomorow.", 5);
        assert!(found.iter().any(|m| m.word == "tomorow"), "{found:?}");
        let tomorow = found.iter().find(|m| m.word == "tomorow").unwrap();
        assert!(tomorow.suggestions.iter().any(|s| s == "tomorrow"), "{tomorow:?}");
    }

    #[test]
    fn fautes_encore_valables() {
        use super::{Misspelling, still_valid};
        let m = |start: usize, word: &str| Misspelling {
            range: start..start + word.len(),
            word: word.into(),
            suggestions: vec![],
        };
        let found = vec![m(0, "bonjuor"), m(8, "tuot")];
        // Frappe à la fin : les deux fautes restent à leur place.
        assert_eq!(still_valid(&found, "bonjuor tuot le monde").len(), 2);
        // Mot prolongé : il n'est plus le même, sa marque disparaît.
        assert_eq!(still_valid(&found, "bonjuor tuots"), vec![m(0, "bonjuor")]);
        // Texte inséré avant : les positions ont bougé, rien n'est gardé à tort.
        assert!(still_valid(&found, "Ah bonjuor tuot").is_empty());
    }
}
