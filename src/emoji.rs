//! Conversion des codes emoji Mattermost (`:smile:`) en caractères Unicode.

/// Préfixe des images Twemoji dans le Markdown préparé (`mm-twemoji:1f600`).
pub const TWEMOJI_SCHEME: &str = "mm-twemoji:";

/// Emoji pour un nom Mattermost. Les noms de Mattermost ne suivent pas tous
/// ceux de GitHub (« rolling_on_the_floor_laughing » contre « rofl ») : après
/// le code court, on essaie le nom Unicode, puis les suffixes de ton de peau
/// (`_tone2`, `_medium_skin_tone`…).
pub fn find(name: &str) -> Option<String> {
    let name = name.trim_matches(':');
    if let Some(e) = emojis::get_by_shortcode(name) {
        return Some(e.as_str().to_string());
    }
    let by_name = |n: &str| {
        let wanted = n.replace(['_', '-'], " ");
        emojis::iter().find(|e| e.name().eq_ignore_ascii_case(&wanted)).or_else(|| emojis::get_by_shortcode(n)).or_else(
            || {
                // Noms d'emoji-data : « male-teacher » (« man_teacher »),
                // « woman-raising-hand » (« woman_raising_hand »).
                let alt = if let Some(rest) = n.strip_prefix("female-") {
                    format!("woman_{rest}")
                } else if let Some(rest) = n.strip_prefix("male-") {
                    format!("man_{rest}")
                } else {
                    n.to_string()
                };
                emojis::get_by_shortcode(&alt.replace('-', "_"))
            },
        )
    };
    if let Some(e) = by_name(name) {
        return Some(e.as_str().to_string());
    }
    // Ton de peau : « thumbsup_tone3 », « wave_medium_skin_tone ».
    const TONES: [(&str, emojis::SkinTone); 10] = [
        ("_tone1", emojis::SkinTone::Light),
        ("_tone2", emojis::SkinTone::MediumLight),
        ("_tone3", emojis::SkinTone::Medium),
        ("_tone4", emojis::SkinTone::MediumDark),
        ("_tone5", emojis::SkinTone::Dark),
        ("_medium_light_skin_tone", emojis::SkinTone::MediumLight),
        ("_medium_dark_skin_tone", emojis::SkinTone::MediumDark),
        ("_light_skin_tone", emojis::SkinTone::Light),
        ("_dark_skin_tone", emojis::SkinTone::Dark),
        ("_medium_skin_tone", emojis::SkinTone::Medium),
    ];
    for (suffix, tone) in TONES {
        if let Some(base) = name.strip_suffix(suffix)
            && let Some(e) = by_name(base)
        {
            return Some(e.with_skin_tone(tone).unwrap_or(e).as_str().to_string());
        }
    }
    // Noms d'emoji-data repris par Mattermost : « unicorn_face » (« unicorn »).
    if let Some(base) = name.strip_suffix("_face")
        && let Some(e) = by_name(base)
    {
        return Some(e.as_str().to_string());
    }
    None
}

/// Emoji Unicode pour un nom Mattermost, ou `:nom:` s'il est inconnu
/// (emoji personnalisé du serveur, par exemple).
pub fn lookup(name: &str) -> String {
    find(name).unwrap_or_else(|| format!(":{name}:"))
}

/// Image Twemoji (SVG) d'un emoji, avec ou sans sélecteur de variante.
pub fn twemoji_svg(emoji: &str) -> Option<&'static str> {
    use twemoji_assets::svg::SvgTwemojiAsset;
    let asset =
        SvgTwemojiAsset::from_emoji(emoji).or_else(|| SvgTwemojiAsset::from_emoji(&emoji.replace('\u{fe0f}', "")))?;
    Some(**asset)
}

/// « 😄 » → « 1f604 » (pour une adresse d'image), et l'inverse.
pub fn to_hex(emoji: &str) -> String {
    emoji.chars().map(|c| format!("{:x}", c as u32)).collect::<Vec<_>>().join("-")
}

pub fn from_hex(hex: &str) -> String {
    hex.split('-').filter_map(|h| u32::from_str_radix(h, 16).ok()).filter_map(char::from_u32).collect()
}

/// Un caractère qui s'affiche en emoji par défaut (et non en texte comme ©).
fn emoji_presentation(c: char) -> bool {
    let c = c as u32;
    c >= 0x1F000 || (0x2600..=0x27BF).contains(&c) || (0x2B00..=0x2BFF).contains(&c) || (0x2300..=0x23FF).contains(&c)
}

/// Découpe un texte en morceaux : (texte, false) ou (emoji, true).
/// Cherche à chaque position la plus longue séquence emoji connue
/// (drapeaux, familles, tons de peau compris).
pub fn split_emojis(text: &str) -> Vec<(&str, bool)> {
    let mut out = Vec::new();
    let mut last = 0;
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        let first = rest.chars().next().expect("position valide");
        let mut found = None;
        if first as u32 >= 0x2300 {
            // Séquences jusqu'à 10 caractères, de la plus longue à la plus courte.
            let ends: Vec<usize> = rest.char_indices().map(|(j, c)| j + c.len_utf8()).take(10).collect();
            for &end in ends.iter().rev() {
                let candidate = &rest[..end];
                if emojis::get(candidate).is_some()
                    && (emoji_presentation(first) || candidate.contains('\u{fe0f}'))
                    && twemoji_svg(candidate).is_some()
                {
                    found = Some(end);
                    break;
                }
            }
        }
        match found {
            Some(end) => {
                if last < i {
                    out.push((&text[last..i], false));
                }
                out.push((&text[i..i + end], true));
                i += end;
                last = i;
            }
            None => i += first.len_utf8(),
        }
    }
    if last < text.len() {
        out.push((&text[last..], false));
    }
    out
}

/// Le texte ne contient que des emojis (au plus 6) et des espaces : affichage en grand.
pub fn only_emojis(text: &str) -> bool {
    let parts = split_emojis(text.trim());
    let count = parts.iter().filter(|(_, e)| *e).count();
    (1..=6).contains(&count) && parts.iter().all(|(t, e)| *e || t.trim().is_empty())
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
                if let Some(e) = find(code) {
                    out.push_str(&e);
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

/// Catalogue des emojis standard par catégorie, pour le sélecteur :
/// (catégorie, [(nom, caractère)]). Calculé une seule fois.
/// Une catégorie du sélecteur : (clé de traduction du titre, [(nom, caractère)]).
pub type EmojiGroup = (&'static str, Vec<(String, String)>);

pub fn catalog() -> &'static [EmojiGroup] {
    use std::sync::OnceLock;
    static CATALOG: OnceLock<Vec<EmojiGroup>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        emojis::Group::iter()
            .map(|group| {
                let label = match group {
                    emojis::Group::SmileysAndEmotion => "emoji.smileys",
                    emojis::Group::PeopleAndBody => "emoji.people",
                    emojis::Group::AnimalsAndNature => "emoji.animals",
                    emojis::Group::FoodAndDrink => "emoji.food",
                    emojis::Group::TravelAndPlaces => "emoji.travel",
                    emojis::Group::Activities => "emoji.activities",
                    emojis::Group::Objects => "emoji.objects",
                    emojis::Group::Symbols => "emoji.symbols",
                    emojis::Group::Flags => "emoji.flags",
                };
                let list = group
                    .emojis()
                    .filter_map(|e| e.shortcode().map(|code| (code.to_string(), e.as_str().to_string())))
                    .collect();
                (label, list)
            })
            .collect()
    })
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
    fn noms_mattermost() {
        assert_eq!(lookup("rolling_on_the_floor_laughing"), "🤣");
        assert_eq!(lookup("rofl"), "🤣");
        assert_eq!(lookup("thumbsup_tone3"), "👍🏽");
        assert_eq!(lookup("unicorn_face"), "🦄");
        assert_eq!(lookup("male-teacher"), "👨‍🏫");
        assert_eq!(lookup("male-teacher_medium_skin_tone"), "👨🏽‍🏫");
        assert_eq!(lookup("female-technologist"), "👩‍💻");
        assert_eq!(lookup("woman-raising-hand"), "🙋‍♀️");
        assert_eq!(lookup("inconnu_perso"), ":inconnu_perso:");
    }

    #[test]
    fn decoupe_et_images() {
        let parts = split_emojis("ok 👍🏽 © 🇫🇷!");
        assert!(parts.contains(&("👍🏽", true)));
        assert!(parts.contains(&("🇫🇷", true)));
        assert!(!parts.iter().any(|(t, e)| *e && t.contains('©')));
        assert!(twemoji_svg("😄").is_some());
        assert_eq!(from_hex(&to_hex("👍🏽")), "👍🏽");
        assert!(only_emojis(" 🎉🎉 "));
        assert!(!only_emojis("bravo 🎉"));
    }

    #[test]
    fn keeps_unknown_and_plain_colons() {
        assert_eq!(replace_shortcodes("heure 12:30"), "heure 12:30");
        assert_eq!(replace_shortcodes(":perso_glpi:"), ":perso_glpi:");
        assert_eq!(replace_shortcodes("a: b :c"), "a: b :c");
    }
}
