//! Correction orthographique hors Windows : dictionnaires Hunspell lus par
//! Spellbook (en Rust, sans bibliothèque système).
//!
//! Dictionnaires : ceux du système quand ils sont lisibles (`.deb`, AppImage :
//! `/usr/share/hunspell`), sinon le français et l'anglais embarqués (ceux de
//! LibreOffice, voir `assets/dictionaries`). Comme sous Windows : la langue de
//! l'interface plus l'anglais, un mot n'est signalé que si aucun ne le connaît.
//! Les mots ajoutés vont dans `matterfew/words.txt` (dossier de configuration).

// Sous Windows, le correcteur du système sert : ce module n'y est que testé.
#![cfg_attr(windows, allow(dead_code))]

use std::collections::HashSet;
use std::io::Read as _;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use crate::spell::{Misspelling, skip};

/// Dossiers des dictionnaires Hunspell du système.
const SYSTEM_DIRS: [&str; 3] = ["/usr/share/hunspell", "/usr/share/myspell", "/usr/share/myspell/dicts"];

/// Dictionnaires embarqués (compressés) : (langue, .aff, .dic).
const EMBEDDED: [(&str, &[u8], &[u8]); 2] = [
    ("fr", include_bytes!("../assets/dictionaries/fr.aff.gz"), include_bytes!("../assets/dictionaries/fr.dic.gz")),
    (
        "en",
        include_bytes!("../assets/dictionaries/en_US.aff.gz"),
        include_bytes!("../assets/dictionaries/en_US.dic.gz"),
    ),
];

pub struct Hunspell {
    /// Chargés en arrière-plan (le français prend un moment) ; vide avant.
    dictionaries: Arc<OnceLock<Vec<spellbook::Dictionary>>>,
    personal: Mutex<HashSet<String>>,
}

impl Hunspell {
    /// Dictionnaires pour ces langues (« fr-FR », « en-US »…, par préférence),
    /// chargés sur un fil à part.
    pub fn new(languages: Vec<&'static str>) -> Option<Self> {
        let dictionaries: Arc<OnceLock<Vec<spellbook::Dictionary>>> = Arc::default();
        let slot = dictionaries.clone();
        std::thread::Builder::new()
            .name("dictionnaires".into())
            .spawn(move || {
                let _ = slot.set(load(&languages));
            })
            .ok()?;
        Some(Self { dictionaries, personal: Mutex::new(read_personal()) })
    }

    /// Chargement immédiat (tests).
    #[cfg(test)]
    pub fn new_blocking(languages: Vec<&'static str>) -> Self {
        let dictionaries: Arc<OnceLock<Vec<spellbook::Dictionary>>> = Arc::default();
        let _ = dictionaries.set(load(&languages));
        Self { dictionaries, personal: Mutex::default() }
    }

    /// Mots inconnus du texte (au plus `limit`), avec des propositions.
    pub fn check(&self, text: &str, limit: usize) -> Vec<Misspelling> {
        let Some(dicts) = self.dictionaries.get().filter(|d| !d.is_empty()) else { return Vec::new() };
        let personal = self.personal.lock().map(|p| p.clone()).unwrap_or_default();
        let mut out = Vec::new();
        for range in words(text) {
            if skip(text, &range) {
                continue;
            }
            let word = &text[range.clone()];
            if personal.contains(word) || known(dicts, word) {
                continue;
            }
            let suggestions = suggest(dicts, word, 4);
            out.push(Misspelling { range, word: word.to_string(), suggestions });
            if out.len() >= limit {
                break;
            }
        }
        out
    }

    /// Ajoute un mot au dictionnaire personnel.
    pub fn add(&self, word: &str) {
        if let Ok(mut personal) = self.personal.lock()
            && personal.insert(word.to_string())
        {
            let mut words: Vec<&String> = personal.iter().collect();
            words.sort();
            if let Some(path) = personal_file() {
                let _ = std::fs::create_dir_all(path.parent().unwrap_or(&path));
                let text: String = words.iter().map(|w| format!("{w}\n")).collect();
                let _ = std::fs::write(path, text);
            }
        }
    }
}

fn personal_file() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("matterfew").join("words.txt"))
}

fn read_personal() -> HashSet<String> {
    personal_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .map(|t| t.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect())
        .unwrap_or_default()
}

/// Un dictionnaire par langue : celui du système s'il est lisible, sinon
/// l'embarqué (français, anglais).
fn load(languages: &[&str]) -> Vec<spellbook::Dictionary> {
    let mut done: Vec<&str> = Vec::new();
    let mut out = Vec::new();
    for tag in languages {
        let base = tag.split('-').next().unwrap_or(tag);
        if done.contains(&base) {
            continue;
        }
        let dict = system_dictionary(tag).or_else(|| embedded_dictionary(base));
        if let Some(dict) = dict {
            done.push(base);
            out.push(dict);
        }
    }
    out
}

/// « fr-FR » → `/usr/share/hunspell/fr_FR.{aff,dic}` (UTF-8 seulement).
fn system_dictionary(tag: &str) -> Option<spellbook::Dictionary> {
    if cfg!(windows) {
        return None;
    }
    let name = tag.replace('-', "_");
    SYSTEM_DIRS.iter().find_map(|dir| {
        let aff = std::fs::read_to_string(format!("{dir}/{name}.aff")).ok()?;
        let dic = std::fs::read_to_string(format!("{dir}/{name}.dic")).ok()?;
        spellbook::Dictionary::new(&aff, &dic).ok()
    })
}

fn embedded_dictionary(base: &str) -> Option<spellbook::Dictionary> {
    let (_, aff, dic) = EMBEDDED.iter().find(|(lang, _, _)| *lang == base)?;
    let unzip = |bytes: &[u8]| {
        let mut text = String::new();
        flate2::read::GzDecoder::new(bytes).read_to_string(&mut text).ok().map(|_| text)
    };
    spellbook::Dictionary::new(&unzip(aff)?, &unzip(dic)?).ok()
}

/// Un des dictionnaires connaît-il ce mot ? Les élisions (« l'ordinateur »,
/// « qu'il ») et les mots composés (« peut-être ») se vérifient par morceaux.
fn known(dicts: &[spellbook::Dictionary], word: &str) -> bool {
    let check = |w: &str| w.is_empty() || dicts.iter().any(|d| d.check(w));
    let word = word.replace('’', "'");
    if check(&word) {
        return true;
    }
    // Sigles (CI, API, HTTP) : pas des fautes.
    if word.chars().count() > 1 && word.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()) {
        return true;
    }
    if let Some((head, rest)) = word.split_once('\'')
        && head.chars().count() <= 7
        && check(rest)
    {
        return true;
    }
    word.contains('-') && word.split('-').all(|part| check(part) || known(dicts, part))
}

/// Propositions des dictionnaires, à tour de rôle.
fn suggest(dicts: &[spellbook::Dictionary], word: &str, limit: usize) -> Vec<String> {
    let lists: Vec<Vec<String>> = dicts
        .iter()
        .map(|d| {
            let mut s = Vec::new();
            d.suggest(word, &mut s);
            s
        })
        .collect();
    let mut out: Vec<String> = Vec::new();
    for rank in 0..limit {
        for s in lists.iter().filter_map(|l| l.get(rank)) {
            if out.len() < limit && !out.contains(s) && s != word {
                out.push(s.clone());
            }
        }
    }
    out
}

/// Mots d'un texte (positions en octets) : lettres, avec apostrophes et
/// traits d'union entre deux lettres.
fn words(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let letter = c.is_alphabetic();
        let joiner =
            matches!(c, '\'' | '’' | '-') && start.is_some() && chars.peek().is_some_and(|(_, n)| n.is_alphabetic());
        match (start, letter || joiner) {
            (None, true) if letter => start = Some(i),
            (Some(s), false) => {
                out.push(s..i);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push(s..text.len());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoupage_en_mots() {
        let text = "L’ordinateur, c'est peut-être prêt : http://x.fr !";
        let found: Vec<&str> = words(text).into_iter().map(|r| &text[r]).collect();
        assert_eq!(found, ["L’ordinateur", "c'est", "peut-être", "prêt", "http", "x", "fr"]);
    }

    #[test]
    fn francais_et_anglais_embarques() {
        let started = std::time::Instant::now();
        let checker = Hunspell::new_blocking(vec!["fr-FR", "fr", "en-US", "en"]);
        eprintln!("chargement : {:?}", started.elapsed());
        let text = "Bonjour, je suis tres contente, cet ordinatuer et l'ordinateur est peut-être prêt pour le meeting de tomorow, la CI passe.";
        let found = checker.check(text, 10);
        let words: Vec<&str> = found.iter().map(|m| m.word.as_str()).collect();
        assert!(words.contains(&"tres"), "{words:?}");
        assert!(words.contains(&"ordinatuer"), "{words:?}");
        assert!(words.contains(&"tomorow"), "{words:?}");
        for ok in ["Bonjour", "l'ordinateur", "peut-être", "prêt", "meeting", "CI"] {
            assert!(!words.contains(&ok), "{ok} signalé à tort : {words:?}");
        }
        let tomorow = found.iter().find(|m| m.word == "tomorow").unwrap();
        assert!(tomorow.suggestions.iter().any(|s| s == "tomorrow"), "{tomorow:?}");
        let tres = found.iter().find(|m| m.word == "tres").unwrap();
        assert!(tres.suggestions.iter().any(|s| s == "très"), "{tres:?}");
    }
}
