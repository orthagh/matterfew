//! Faut-il afficher une notification de bureau pour un nouveau message ?
//!
//! Reprend la logique du client web officiel (`shouldSkipNotification`,
//! `webapp/channels/src/actions/notification_actions.tsx`) :
//! 1. jamais pour ses propres messages (sauf s'ils viennent d'un webhook) ni
//!    pour les messages système ;
//! 2. rien pour un canal en sourdine, ni en « Ne pas déranger » / absent ;
//! 3. niveau du canal (`all` / `mention` / `none`), à défaut celui de
//!    l'utilisateur ; un groupe réglé « par défaut » avec un utilisateur en
//!    « mentions » compte comme « tous » ;
//! 4. en « mentions », hors message direct : seulement si l'on est mentionné
//!    ou si l'on suit le fil ;
//! 5. avec les fils réduits, une réponse ne notifie que si l'on suit le fil
//!    (ou si l'on est mentionné) ;
//! 6. rien si la fenêtre a le focus et que le canal (ou le fil) est affiché.

#[derive(Debug, Clone, Default)]
pub struct Input<'a> {
    pub from_me: bool,
    pub from_webhook: bool,
    pub is_system: bool,
    /// "D", "G", "O" ou "P".
    pub channel_type: &'a str,
    pub channel_muted: bool,
    /// Réglage du canal : "default", "all", "mention", "none" (ou vide).
    pub channel_level: &'a str,
    /// Réglage de l'utilisateur : "all", "mention", "none" (vide = "all").
    pub user_level: &'a str,
    /// Statut de l'utilisateur courant : "online", "away", "dnd", "ooo"…
    pub my_status: &'a str,
    pub mentions_me: bool,
    pub follows_thread: bool,
    pub is_reply: bool,
    pub collapsed_threads: bool,
    pub window_active: bool,
    pub channel_visible: bool,
    /// Le fil du message est ouvert dans le panneau latéral.
    pub thread_visible: bool,
}

pub fn should_notify(i: &Input) -> bool {
    if i.from_me && !i.from_webhook {
        return false;
    }
    if i.is_system || i.channel_muted {
        return false;
    }
    if matches!(i.my_status, "dnd" | "ooo") {
        return false;
    }

    let user_level = if i.user_level.is_empty() { "all" } else { i.user_level };
    let channel_default = i.channel_level.is_empty() || i.channel_level == "default";
    let level = if channel_default {
        if i.channel_type == "G" && user_level == "mention" { "all" } else { user_level }
    } else {
        i.channel_level
    };

    let direct = i.channel_type == "D";
    match level {
        "none" => return false,
        "mention" if !direct && !(i.mentions_me || i.follows_thread) => return false,
        _ => {}
    }
    if i.collapsed_threads && i.is_reply && !(i.follows_thread || i.mentions_me) {
        return false;
    }

    if i.window_active {
        let visible = if i.collapsed_threads && i.is_reply { i.thread_visible } else { i.channel_visible };
        if visible {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{Input, should_notify};

    fn base() -> Input<'static> {
        Input { channel_type: "O", channel_level: "default", user_level: "mention", my_status: "online", ..Default::default() }
    }

    #[test]
    fn mention_level_needs_a_mention_except_in_dm() {
        assert!(!should_notify(&base()));
        assert!(should_notify(&Input { mentions_me: true, ..base() }));
        assert!(should_notify(&Input { channel_type: "D", ..base() }));
    }

    #[test]
    fn group_default_with_user_mention_counts_as_all() {
        assert!(should_notify(&Input { channel_type: "G", ..base() }));
        assert!(!should_notify(&Input { channel_type: "G", channel_level: "mention", ..base() }));
    }

    #[test]
    fn channel_setting_overrides_user_setting() {
        assert!(should_notify(&Input { channel_level: "all", ..base() }));
        assert!(!should_notify(&Input { channel_level: "none", channel_type: "D", ..base() }));
    }

    #[test]
    fn muted_dnd_own_and_system_are_silent() {
        let mentioned = Input { mentions_me: true, ..base() };
        assert!(!should_notify(&Input { channel_muted: true, ..mentioned.clone() }));
        assert!(!should_notify(&Input { my_status: "dnd", ..mentioned.clone() }));
        assert!(!should_notify(&Input { my_status: "ooo", ..mentioned.clone() }));
        assert!(!should_notify(&Input { from_me: true, ..mentioned.clone() }));
        assert!(should_notify(&Input { from_me: true, from_webhook: true, ..mentioned.clone() }));
        assert!(!should_notify(&Input { is_system: true, ..mentioned }));
    }

    #[test]
    fn collapsed_threads_reply_needs_follow() {
        let reply = Input { user_level: "all", is_reply: true, collapsed_threads: true, ..base() };
        assert!(!should_notify(&reply));
        assert!(should_notify(&Input { follows_thread: true, ..reply.clone() }));
        // Sans fils réduits, une réponse se comporte comme un message normal.
        assert!(should_notify(&Input { collapsed_threads: false, ..reply }));
    }

    #[test]
    fn nothing_for_what_is_on_screen() {
        let dm = Input { channel_type: "D", window_active: true, ..base() };
        assert!(!should_notify(&Input { channel_visible: true, ..dm.clone() }));
        assert!(should_notify(&dm));
        // Fenêtre en arrière-plan : notification même pour le canal affiché.
        assert!(should_notify(&Input { channel_visible: true, window_active: false, ..dm.clone() }));
        // Réponse dans un fil ouvert : pas de notification.
        let reply = Input { is_reply: true, collapsed_threads: true, follows_thread: true, ..dm };
        assert!(!should_notify(&Input { thread_visible: true, ..reply.clone() }));
        assert!(should_notify(&Input { channel_visible: true, ..reply }));
    }
}
