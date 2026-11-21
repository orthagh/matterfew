//! Construction de la barre latérale à partir des catégories Mattermost.
//!
//! Reproduit le comportement du client web :
//! - catégories dans l'ordre de l'utilisateur (Favoris, Canaux, Messages
//!   directs, catégories personnelles) ;
//! - tri par catégorie : `alpha` et `""` → alphabétique (canaux en sourdine
//!   à la fin), `recent` → activité récente, `manual` → ordre enregistré ;
//! - section « Non lus » optionnelle (préférence `show_unread_section`) ;
//! - nombre de messages directs visibles limité (`limit_visible_dms_gms`) ;
//! - une catégorie repliée n'affiche que ses canaux non lus et le canal actif.
//!
//! Module sans dépendance à l'interface, pour pouvoir le tester.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::api::{Channel, Preference, SidebarCategory};

/// État d'un canal utile à la barre latérale.
#[derive(Debug, Clone)]
pub struct ChannelState {
    pub channel: Channel,
    /// Messages racine non lus.
    pub unread_messages: bool,
    pub mentions: i64,
    pub muted: bool,
    /// Niveau de notification de bureau propre au canal ("default", "all"…).
    pub notify_desktop: String,
}

impl ChannelState {
    /// Règle du client web : une mention suffit, sinon il faut des
    /// messages non lus dans un canal qui n'est pas en sourdine.
    pub fn shows_unread(&self) -> bool {
        self.mentions > 0 || (!self.muted && self.unread_messages)
    }
}

/// Préférences de la barre latérale.
#[derive(Debug, Clone, PartialEq)]
pub struct SidebarPrefs {
    pub show_unread_section: bool,
    pub dm_limit: usize,
}

impl Default for SidebarPrefs {
    fn default() -> Self {
        Self { show_unread_section: false, dm_limit: 40 }
    }
}

impl SidebarPrefs {
    pub fn from_preferences(prefs: &[Preference]) -> Self {
        let mut out = Self::default();
        for p in prefs.iter().filter(|p| p.category == "sidebar_settings") {
            match p.name.as_str() {
                "show_unread_section" => out.show_unread_section = p.value == "true",
                "limit_visible_dms_gms" => {
                    if let Ok(n) = p.value.parse::<usize>() {
                        out.dm_limit = n.max(1);
                    }
                }
                _ => {}
            }
        }
        out
    }
}

/// Une section affichée dans la barre latérale.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// Identifiant de la catégorie, ou `"unreads"` pour la section Non lus.
    pub id: String,
    pub title: String,
    /// La section peut être repliée (pas la section Non lus).
    pub collapsible: bool,
    pub collapsed: bool,
    pub channel_ids: Vec<String>,
}

pub struct Input<'a, F: Fn(&Channel) -> String> {
    pub channels: &'a HashMap<String, ChannelState>,
    pub categories: &'a [SidebarCategory],
    pub prefs: &'a SidebarPrefs,
    pub current: Option<&'a str>,
    /// Canal resté dans « Non lus » tant qu'on ne l'a pas quitté
    /// (comme le client web, pour qu'il ne saute pas sous le curseur).
    pub sticky_unread: Option<&'a str>,
    pub title: F,
}

pub fn build<F: Fn(&Channel) -> String>(input: &Input<'_, F>) -> Vec<Section> {
    let is_current = |id: &str| input.current == Some(id);
    let is_unread = |id: &str| input.channels.get(id).is_some_and(ChannelState::shows_unread);

    let mut sections = Vec::new();

    // Section « Non lus ».
    let mut in_unreads: HashSet<String> = HashSet::new();
    if input.prefs.show_unread_section {
        let mut ids: Vec<&ChannelState> = input
            .channels
            .values()
            .filter(|c| c.shows_unread() || input.sticky_unread == Some(c.channel.id.as_str()))
            .collect();
        ids.sort_by_key(|x| std::cmp::Reverse(x.channel.recency()));
        in_unreads = ids.iter().map(|c| c.channel.id.clone()).collect();
        if !ids.is_empty() {
            sections.push(Section {
                id: "unreads".into(),
                title: "Non lus".into(),
                collapsible: false,
                collapsed: false,
                channel_ids: ids.into_iter().map(|c| c.channel.id.clone()).collect(),
            });
        }
    }

    // Canaux qui n'apparaissent dans aucune catégorie (ex. message direct
    // tout juste créé) : rangés dans la catégorie par défaut de leur type.
    let categorized: HashSet<&str> =
        input.categories.iter().flat_map(|c| c.channel_ids.iter().map(String::as_str)).collect();
    let orphans: Vec<&ChannelState> =
        input.channels.values().filter(|c| !categorized.contains(c.channel.id.as_str())).collect();

    for category in input.categories {
        let is_dm_category = category.kind == "direct_messages";
        let mut members: Vec<&ChannelState> =
            category.channel_ids.iter().filter_map(|id| input.channels.get(id)).collect();
        for orphan in &orphans {
            let is_dm = orphan.channel.is_direct() || orphan.channel.is_group();
            if (is_dm_category && is_dm) || (category.kind == "channels" && !is_dm) {
                members.push(orphan);
            }
        }
        members.retain(|c| !in_unreads.contains(&c.channel.id));

        sort_members(&mut members, &category.sorting, &input.title);

        // Limite du nombre de messages directs : les non lus et le canal
        // actif restent toujours visibles.
        if is_dm_category {
            let unread_count = members.iter().filter(|c| c.shows_unread()).count();
            let limit = input.prefs.dm_limit.max(unread_count);
            let mut kept = 0;
            members.retain(|c| {
                let keep = kept < limit || c.shows_unread() || is_current(&c.channel.id);
                if keep {
                    kept += 1;
                }
                keep
            });
        }

        if category.collapsed {
            members.retain(|c| is_unread(&c.channel.id) || is_current(&c.channel.id));
        }

        // Favoris vides : pas de section (comme le client web).
        if category.kind == "favorites" && members.is_empty() && !category.collapsed {
            continue;
        }

        sections.push(Section {
            id: category.id.clone(),
            title: category_title(category),
            collapsible: true,
            collapsed: category.collapsed,
            channel_ids: members.into_iter().map(|c| c.channel.id.clone()).collect(),
        });
    }

    sections
}

fn category_title(category: &SidebarCategory) -> String {
    match category.kind.as_str() {
        "favorites" => "Favoris".into(),
        "channels" => "Canaux".into(),
        "direct_messages" => "Messages directs".into(),
        _ => category.display_name.clone(),
    }
}

fn sort_members<F: Fn(&Channel) -> String>(members: &mut Vec<&ChannelState>, sorting: &str, title: &F) {
    match sorting {
        "manual" => {}
        "recent" => members.sort_by_key(|x| std::cmp::Reverse(x.channel.recency())),
        // "alpha" et "" : alphabétique, canaux en sourdine à la fin.
        _ => {
            let keys: HashMap<String, String> =
                members.iter().map(|c| (c.channel.id.clone(), title(&c.channel).to_lowercase())).collect();
            members.sort_by(|a, b| {
                a.muted.cmp(&b.muted).then_with(|| natural_cmp(&keys[&a.channel.id], &keys[&b.channel.id]))
            });
        }
    }
}

/// Comparaison « naturelle » : "canal2" avant "canal10".
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut n = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        n.push(c);
                        it.next();
                    }
                    n.trim_start_matches('0').to_string()
                };
                let (na, nb) = (take(&mut a), take(&mut b));
                let ord = na.len().cmp(&nb.len()).then_with(|| na.cmp(&nb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                a.next();
                b.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(id: &str, kind: &str, name: &str, recency: i64) -> Channel {
        Channel {
            id: id.into(),
            kind: kind.into(),
            name: name.into(),
            display_name: name.into(),
            purpose: String::new(),
            header: String::new(),
            total_msg_count: 0,
            total_msg_count_root: 0,
            last_post_at: recency,
            last_root_post_at: recency,
            create_at: 0,
            delete_at: 0,
        }
    }

    fn state(c: Channel, unread: bool, mentions: i64, muted: bool) -> (String, ChannelState) {
        (
            c.id.clone(),
            ChannelState { channel: c, unread_messages: unread, mentions, muted, notify_desktop: String::new() },
        )
    }

    fn category(id: &str, kind: &str, sorting: &str, ids: &[&str]) -> SidebarCategory {
        SidebarCategory {
            id: id.into(),
            kind: kind.into(),
            display_name: id.into(),
            sorting: sorting.into(),
            collapsed: false,
            muted: false,
            channel_ids: ids.iter().map(|s| s.to_string()).collect(),
            extra: Default::default(),
        }
    }

    fn run(
        channels: &HashMap<String, ChannelState>,
        categories: &[SidebarCategory],
        prefs: &SidebarPrefs,
        current: Option<&str>,
    ) -> Vec<Section> {
        build(&Input {
            channels,
            categories,
            prefs,
            current,
            sticky_unread: None,
            title: |c: &Channel| c.display_name.clone(),
        })
    }

    #[test]
    fn sorts_each_category_by_its_own_rule() {
        let channels: HashMap<_, _> = [
            state(channel("a", "O", "zeta", 1), false, 0, false),
            state(channel("b", "O", "alpha", 3), false, 0, false),
            state(channel("c", "O", "canal10", 2), false, 0, false),
            state(channel("d", "O", "canal2", 4), false, 0, false),
        ]
        .into();
        let prefs = SidebarPrefs::default();

        let alpha = run(&channels, &[category("x", "channels", "alpha", &["a", "b", "c", "d"])], &prefs, None);
        assert_eq!(alpha[0].channel_ids, ["b", "d", "c", "a"]);

        let recent = run(&channels, &[category("x", "channels", "recent", &["a", "b", "c", "d"])], &prefs, None);
        assert_eq!(recent[0].channel_ids, ["d", "b", "c", "a"]);

        let manual = run(&channels, &[category("x", "channels", "manual", &["c", "a", "d", "b"])], &prefs, None);
        assert_eq!(manual[0].channel_ids, ["c", "a", "d", "b"]);
    }

    #[test]
    fn muted_channels_go_last_alphabetically() {
        let channels: HashMap<_, _> =
            [state(channel("a", "O", "aaa", 1), false, 0, true), state(channel("b", "O", "bbb", 1), false, 0, false)]
                .into();
        let s = run(&channels, &[category("x", "channels", "", &["a", "b"])], &SidebarPrefs::default(), None);
        assert_eq!(s[0].channel_ids, ["b", "a"]);
    }

    #[test]
    fn unread_section_pulls_unread_channels_out() {
        let channels: HashMap<_, _> = [
            state(channel("a", "O", "a", 1), true, 0, false),
            state(channel("b", "O", "b", 5), false, 2, true), // sourdine mais mentionné
            state(channel("c", "O", "c", 9), true, 0, true),  // sourdine sans mention
            state(channel("d", "O", "d", 1), false, 0, false),
        ]
        .into();
        let prefs = SidebarPrefs { show_unread_section: true, dm_limit: 40 };
        let s = run(&channels, &[category("x", "channels", "alpha", &["a", "b", "c", "d"])], &prefs, None);
        assert_eq!(s[0].id, "unreads");
        assert_eq!(s[0].channel_ids, ["b", "a"]); // par activité récente
        assert_eq!(s[1].channel_ids, ["d", "c"]);
    }

    #[test]
    fn collapsed_category_keeps_unread_and_current() {
        let channels: HashMap<_, _> = [
            state(channel("a", "O", "a", 1), true, 0, false),
            state(channel("b", "O", "b", 1), false, 0, false),
            state(channel("c", "O", "c", 1), false, 0, false),
        ]
        .into();
        let mut cat = category("x", "channels", "alpha", &["a", "b", "c"]);
        cat.collapsed = true;
        let s = run(&channels, &[cat], &SidebarPrefs::default(), Some("c"));
        assert_eq!(s[0].channel_ids, ["a", "c"]);
        assert!(s[0].collapsed);
    }

    #[test]
    fn direct_message_limit_keeps_unread() {
        let channels: HashMap<_, _> = [
            state(channel("d1", "D", "u1__me", 5), false, 0, false),
            state(channel("d2", "D", "u2__me", 4), false, 0, false),
            state(channel("d3", "D", "u3__me", 3), false, 0, false),
            state(channel("d4", "D", "u4__me", 1), true, 0, false),
        ]
        .into();
        let prefs = SidebarPrefs { show_unread_section: false, dm_limit: 2 };
        let s = run(&channels, &[category("dm", "direct_messages", "recent", &["d1", "d2", "d3", "d4"])], &prefs, None);
        assert_eq!(s[0].channel_ids, ["d1", "d2", "d4"]);
    }

    #[test]
    fn empty_favorites_hidden_and_orphans_placed() {
        let channels: HashMap<_, _> = [
            state(channel("a", "O", "a", 1), false, 0, false),
            state(channel("new-dm", "D", "x__me", 1), false, 0, false),
        ]
        .into();
        let cats = [
            category("fav", "favorites", "", &[]),
            category("ch", "channels", "alpha", &["a"]),
            category("dm", "direct_messages", "recent", &[]),
        ];
        let s = run(&channels, &cats, &SidebarPrefs::default(), None);
        assert_eq!(s.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["ch", "dm"]);
        assert_eq!(s[1].channel_ids, ["new-dm"]);
    }

    #[test]
    fn reads_preferences() {
        let prefs = SidebarPrefs::from_preferences(&[
            Preference {
                user_id: String::new(),
                category: "sidebar_settings".into(),
                name: "show_unread_section".into(),
                value: "true".into(),
            },
            Preference {
                user_id: String::new(),
                category: "sidebar_settings".into(),
                name: "limit_visible_dms_gms".into(),
                value: "15".into(),
            },
        ]);
        assert_eq!(prefs, SidebarPrefs { show_unread_section: true, dm_limit: 15 });
    }
}
