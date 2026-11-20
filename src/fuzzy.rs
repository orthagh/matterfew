//! Recherche rapide de canaux (sélecteur Ctrl+K) : insensible à la casse et
//! aux accents (« general » trouve « Général »), et classement simple.

/// Texte en minuscules, sans accents.
pub fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => 'a',
            'ç' => 'c',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' | 'ì' => 'i',
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => 'o',
            'û' | 'ü' | 'ú' | 'ù' => 'u',
            'ÿ' | 'ý' => 'y',
            'ñ' => 'n',
            c => c,
        })
        .collect()
}

/// Rang d'un titre pour une recherche (plus petit = meilleur) :
/// 0 commence par la recherche, 1 un mot commence par elle, 2 la contient.
/// `None` si le titre ne correspond pas.
pub fn rank(title: &str, query: &str) -> Option<u8> {
    let (t, q) = (fold(title), fold(query.trim()));
    if q.is_empty() {
        return Some(0);
    }
    if t.starts_with(&q) {
        Some(0)
    } else if t
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word.starts_with(&q))
    {
        Some(1)
    } else if t.contains(&q) {
        Some(2)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_case_and_accents() {
        assert_eq!(fold("Équipe Général"), "equipe general");
        assert_eq!(rank("Général", "gene"), Some(0));
    }

    #[test]
    fn ranks_prefix_then_word_then_substring() {
        assert_eq!(rank("dev-backend", "dev"), Some(0));
        assert_eq!(rank("dev-backend", "back"), Some(1));
        assert_eq!(rank("Léa Bernard", "bern"), Some(1));
        assert_eq!(rank("dev-backend", "ack"), Some(2));
        assert_eq!(rank("design", "xyz"), None);
    }
}
