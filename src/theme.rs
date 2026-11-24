//! Thème Mattermost de l'utilisateur.
//!
//! Mattermost enregistre le thème dans les préférences (catégorie `theme`,
//! nom = identifiant d'équipe, ou vide pour toutes les équipes), sous forme
//! d'un JSON de couleurs : `sidebarBg`, `centerChannelBg`, `linkColor`…
//! On le lit pour que le client ait les mêmes couleurs que le client web.
//! À défaut, thème « Denim » (celui par défaut de Mattermost).

use gpui_kit::component::scroll::ScrollbarMode;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;
use serde::Deserialize;

use crate::api::Preference;

/// Couleurs du thème Mattermost (noms identiques au JSON).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MattermostTheme {
    pub sidebar_bg: String,
    pub sidebar_text: String,
    pub sidebar_unread_text: String,
    pub sidebar_text_hover_bg: String,
    pub sidebar_text_active_border: String,
    pub sidebar_text_active_color: String,
    pub sidebar_header_bg: String,
    pub sidebar_header_text_color: String,
    pub sidebar_team_bar_bg: String,
    pub online_indicator: String,
    pub away_indicator: String,
    pub dnd_indicator: String,
    pub mention_bg: String,
    pub mention_color: String,
    pub center_channel_bg: String,
    pub center_channel_color: String,
    pub new_message_separator: String,
    pub link_color: String,
    pub button_bg: String,
    pub button_color: String,
    pub error_text_color: String,
    pub mention_highlight_bg: String,
    pub mention_highlight_link: String,
}

impl Default for MattermostTheme {
    /// Thème « Denim ».
    fn default() -> Self {
        let s = |v: &str| v.to_string();
        Self {
            sidebar_bg: s("#1e325c"),
            sidebar_text: s("#ffffff"),
            sidebar_unread_text: s("#ffffff"),
            sidebar_text_hover_bg: s("#28427b"),
            sidebar_text_active_border: s("#5d89ea"),
            sidebar_text_active_color: s("#ffffff"),
            sidebar_header_bg: s("#192a4d"),
            sidebar_header_text_color: s("#ffffff"),
            sidebar_team_bar_bg: s("#14213e"),
            online_indicator: s("#3db887"),
            away_indicator: s("#ffbc1f"),
            dnd_indicator: s("#d24b4e"),
            mention_bg: s("#ffffff"),
            mention_color: s("#1e325c"),
            center_channel_bg: s("#ffffff"),
            center_channel_color: s("#3f4350"),
            new_message_separator: s("#cc8f00"),
            link_color: s("#386fe5"),
            button_bg: s("#1c58d9"),
            button_color: s("#ffffff"),
            error_text_color: s("#d24b4e"),
            mention_highlight_bg: s("#ffd470"),
            mention_highlight_link: s("#1b1d22"),
        }
    }
}

impl MattermostTheme {
    /// Thème de l'équipe s'il existe, sinon thème global, sinon Denim.
    pub fn from_preferences(prefs: &[Preference], team_id: Option<&str>) -> Self {
        let find = |name: &str| {
            prefs
                .iter()
                .find(|p| p.category == "theme" && p.name == name)
                .and_then(|p| serde_json::from_str::<MattermostTheme>(&p.value).ok())
        };
        team_id.and_then(find).or_else(|| find("")).unwrap_or_default()
    }
}

/// Police d'interface de Windows 11 : un client natif adopte la police du
/// système plutôt que d'imiter celle du client web.
const NATIVE_FONT: &str = "Segoe UI Variable Text";

/// Linux: GPUI draws a variable font at its regular weight only, and the
/// system font often is one (Ubuntu Sans, Cantarell). When the interface font
/// has no separate bold face, returns an installed family that has one, so
/// that unread channels and bold text stand out.
#[cfg(target_os = "linux")]
fn bold_capable_family(cx: &App) -> Option<SharedString> {
    let text_system = cx.text_system();
    let has_bold = |family: &SharedString| {
        let regular = font(family.clone());
        let bold = Font { weight: FontWeight::BOLD, ..regular.clone() };
        text_system.resolve_font(&regular) != text_system.resolve_font(&bold)
    };
    if has_bold(&Theme::global(cx).font_family) {
        return None;
    }
    let installed = text_system.all_font_names();
    ["Noto Sans", "Liberation Sans", "DejaVu Sans"]
        .into_iter()
        .filter(|name| installed.iter().any(|n| n == name))
        .map(SharedString::from)
        .find(has_bold)
}

/// Palette prête à l'emploi pour le rendu.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub sidebar_bg: Hsla,
    pub sidebar_text: Hsla,
    pub sidebar_unread: Hsla,
    pub sidebar_hover: Hsla,
    pub sidebar_active_border: Hsla,
    pub sidebar_active_text: Hsla,
    pub sidebar_header_bg: Hsla,
    pub sidebar_header_text: Hsla,
    /// Fond de la barre des équipes du thème (la barre est dérivée de la barre latérale).
    #[allow(dead_code)]
    pub team_bar_bg: Hsla,
    pub online: Hsla,
    pub away: Hsla,
    pub dnd: Hsla,
    pub mention_bg: Hsla,
    pub mention_color: Hsla,
    pub center_bg: Hsla,
    pub center_color: Hsla,
    pub new_separator: Hsla,
    pub link: Hsla,
    pub button_bg: Hsla,
    pub button_color: Hsla,
    pub error: Hsla,
    pub mention_highlight_bg: Hsla,
}

impl Palette {
    pub fn from_theme(t: &MattermostTheme) -> Self {
        let d = MattermostTheme::default();
        // Une couleur illisible dans le thème : on garde celle de Denim.
        let c = |v: &str, fallback: &str| parse_color(v).or_else(|| parse_color(fallback)).unwrap_or(black());
        Self {
            sidebar_bg: c(&t.sidebar_bg, &d.sidebar_bg),
            sidebar_text: c(&t.sidebar_text, &d.sidebar_text),
            sidebar_unread: c(&t.sidebar_unread_text, &d.sidebar_unread_text),
            sidebar_hover: c(&t.sidebar_text_hover_bg, &d.sidebar_text_hover_bg),
            sidebar_active_border: c(&t.sidebar_text_active_border, &d.sidebar_text_active_border),
            sidebar_active_text: c(&t.sidebar_text_active_color, &d.sidebar_text_active_color),
            sidebar_header_bg: c(&t.sidebar_header_bg, &d.sidebar_header_bg),
            sidebar_header_text: c(&t.sidebar_header_text_color, &d.sidebar_header_text_color),
            team_bar_bg: c(&t.sidebar_team_bar_bg, &d.sidebar_team_bar_bg),
            online: c(&t.online_indicator, &d.online_indicator),
            away: c(&t.away_indicator, &d.away_indicator),
            dnd: c(&t.dnd_indicator, &d.dnd_indicator),
            mention_bg: c(&t.mention_bg, &d.mention_bg),
            mention_color: c(&t.mention_color, &d.mention_color),
            center_bg: c(&t.center_channel_bg, &d.center_channel_bg),
            center_color: c(&t.center_channel_color, &d.center_channel_color),
            new_separator: c(&t.new_message_separator, &d.new_message_separator),
            link: c(&t.link_color, &d.link_color),
            button_bg: c(&t.button_bg, &d.button_bg),
            button_color: c(&t.button_color, &d.button_color),
            error: c(&t.error_text_color, &d.error_text_color),
            mention_highlight_bg: c(&t.mention_highlight_bg, &d.mention_highlight_bg),
        }
    }

    /// Texte secondaire (heures, descriptions) : couleur du texte à 64 %.
    pub fn muted(&self) -> Hsla {
        self.center_color.opacity(0.64)
    }
    /// Bordures discrètes de la zone centrale.
    pub fn border(&self) -> Hsla {
        self.center_color.opacity(0.12)
    }
    /// Fond au survol d'un message.
    pub fn hover(&self) -> Hsla {
        self.center_color.opacity(0.04)
    }
    pub fn is_dark(&self) -> bool {
        self.center_bg.l < 0.5
    }

    /// Aligne les composants de GPUI Kit (champs, boutons, menus) sur le thème.
    pub fn apply(&self, cx: &mut App) {
        let mode = if self.is_dark() { ThemeMode::Dark } else { ThemeMode::Light };
        if Theme::global(cx).mode != mode {
            Theme::change(mode, None, cx);
        }
        // `Theme::update` resynchronise les jetons de style dérivés des
        // couleurs (sinon les boutons garderaient leur couleur d'origine).
        let p = *self;
        // Police native de Windows 11 si elle est installée (sinon, celle du système).
        let native = cx.text_system().all_font_names().iter().any(|n| n == NATIVE_FONT);
        #[cfg(target_os = "linux")]
        let with_bold = bold_capable_family(cx);
        Theme::update(cx, move |theme| {
            p.write_colors(theme);
            // Barres de défilement invisibles hors défilement.
            theme.scrollbar_mode = ScrollbarMode::Scrolling;
            if native {
                theme.font_family = NATIVE_FONT.into();
            }
            #[cfg(target_os = "linux")]
            if let Some(family) = &with_bold {
                theme.font_family = family.clone();
            }
        });
    }

    fn write_colors(&self, theme: &mut Theme) {
        theme.background = self.center_bg;
        theme.foreground = self.center_color;
        theme.border = self.border();
        theme.input = self.center_color.opacity(0.16);
        theme.ring = self.button_bg.opacity(0.5);
        theme.muted = self.center_color.opacity(0.06);
        theme.muted_foreground = self.muted();
        theme.popover = self.center_bg;
        theme.popover_foreground = self.center_color;
        theme.primary = self.button_bg;
        theme.primary_hover = self.button_bg.opacity(0.88);
        theme.primary_active = self.button_bg.opacity(0.78);
        theme.primary_foreground = self.button_color;
        theme.button_primary = self.button_bg;
        theme.button_primary_hover = self.button_bg.opacity(0.88);
        theme.button_primary_active = self.button_bg.opacity(0.78);
        theme.button_primary_foreground = self.button_color;
        theme.link = self.link;
        theme.link_hover = self.link.opacity(0.8);
        theme.danger = self.error;
        theme.accent = self.center_color.opacity(0.08);
        theme.accent_foreground = self.center_color;
        theme.selection = self.button_bg.opacity(0.25);
        theme.caret = self.center_color;
        theme.radius = px(4.);
        theme.radius_lg = px(8.);
    }
}

/// `#rgb`, `#rrggbb`, `rgb(r,g,b)` ou `rgba(r,g,b,a)`.
pub fn parse_color(value: &str) -> Option<Hsla> {
    let v = value.trim();
    if let Some(hex) = v.strip_prefix('#') {
        let hex = match hex.len() {
            3 => hex.chars().flat_map(|c| [c, c]).collect::<String>(),
            6 => hex.to_string(),
            _ => return None,
        };
        let n = u32::from_str_radix(&hex, 16).ok()?;
        return Some(rgb(n).into());
    }
    let inner = v.strip_prefix("rgba(").or_else(|| v.strip_prefix("rgb("))?.strip_suffix(')')?;
    let parts: Vec<f32> = inner.split(',').map(|p| p.trim().parse::<f32>()).collect::<Result<_, _>>().ok()?;
    let (r, g, b) = (*parts.first()?, *parts.get(1)?, *parts.get(2)?);
    let a = parts.get(3).copied().unwrap_or(1.0);
    Some(Rgba { r: r / 255.0, g: g / 255.0, b: b / 255.0, a }.into())
}

#[cfg(test)]
mod tests {
    // Imports explicites : `use super::*` ramènerait la macro `test` de GPUI.
    use super::{MattermostTheme, Preference, parse_color};

    #[test]
    fn parses_colors() {
        let white = parse_color("#ffffff").unwrap();
        assert!((white.l - 1.0).abs() < 0.01);
        assert_eq!(parse_color("#fff"), parse_color("#ffffff"));
        let half = parse_color("rgba(0, 0, 0, 0.5)").unwrap();
        assert!((half.a - 0.5).abs() < 0.01);
        assert!(parse_color("bleu").is_none());
    }

    #[test]
    fn team_theme_overrides_global() {
        let prefs = vec![
            Preference {
                user_id: String::new(),
                category: "theme".into(),
                name: "".into(),
                value: r##"{"sidebarBg":"#111111"}"##.into(),
            },
            Preference {
                user_id: String::new(),
                category: "theme".into(),
                name: "team1".into(),
                value: r##"{"sidebarBg":"#222222"}"##.into(),
            },
        ];
        assert_eq!(MattermostTheme::from_preferences(&prefs, Some("team1")).sidebar_bg, "#222222");
        assert_eq!(MattermostTheme::from_preferences(&prefs, Some("autre")).sidebar_bg, "#111111");
        // Les couleurs absentes du JSON gardent la valeur Denim.
        assert_eq!(MattermostTheme::from_preferences(&prefs, None).link_color, "#386fe5");
    }
}
