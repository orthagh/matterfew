//! Notifications de bureau natives (toasts Windows, D-Bus sous Linux…).

pub fn show(title: &str, body: &str) {
    // Corps tronqué : les toasts Windows coupent de toute façon les longs textes.
    let body: String = body.chars().take(200).collect();
    if let Err(err) = notify_rust::Notification::new()
        .summary(title)
        .body(&body)
        .show()
    {
        eprintln!("notification impossible : {err}");
    }
}
