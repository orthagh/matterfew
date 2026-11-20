//! Notifications de bureau. Sous Windows : notification avec l'identité de
//! l'application, dont le clic ouvre le canal (voir `desktop.rs`).

use crate::desktop::{self, ClickTarget};

pub fn show(title: &str, body: &str, target: Option<ClickTarget>) {
    // Corps tronqué : les notifications Windows coupent de toute façon les longs textes.
    let body: String = body.chars().take(200).collect();
    if let Err(err) = desktop::show_notification(title, &body, target) {
        eprintln!("notification impossible : {err:#}");
    }
}
