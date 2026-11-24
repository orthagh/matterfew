//! Intégration au bureau Windows, sur le modèle du client de bureau officiel :
//! - notifications avec l'identité de l'application (nom, icône) et clic qui
//!   ouvre le bon canal ;
//! - pastille sur l'icône de la barre des tâches (mentions, non-lus) ;
//! - icône dans la zone de notification, avec un menu, et fermeture de la
//!   fenêtre qui la réduit dans cette zone ;
//! - clignotement de la barre des tâches à l'arrivée d'une notification.
//!
//! Linux (GNOME, KDE) et macOS : notifications cliquables (`notify-rust`),
//! pastille du lanceur ou du Dock, icône StatusNotifierItem ou de la barre
//! des menus, une seule instance (socket Unix).

use std::sync::OnceLock;

/// Canal et équipe à ouvrir quand on clique sur une notification.
#[derive(Debug, Clone)]
pub struct ClickTarget {
    /// Vide pour un message direct (pas d'équipe).
    pub team_id: String,
    pub channel_id: String,
}

/// Action demandée depuis l'icône de la zone de notification.
#[derive(Debug, Clone, Copy)]
pub enum TrayAction {
    Show,
    Quit,
}

type Pipe<T> = (async_channel::Sender<T>, async_channel::Receiver<T>);

fn clicks_pipe() -> &'static Pipe<ClickTarget> {
    static PIPE: OnceLock<Pipe<ClickTarget>> = OnceLock::new();
    PIPE.get_or_init(async_channel::unbounded)
}

fn tray_pipe() -> &'static Pipe<TrayAction> {
    static PIPE: OnceLock<Pipe<TrayAction>> = OnceLock::new();
    PIPE.get_or_init(async_channel::unbounded)
}

/// Clics sur les notifications, à écouter depuis l'interface.
pub fn clicks() -> async_channel::Receiver<ClickTarget> {
    clicks_pipe().1.clone()
}

/// Actions de l'icône de la zone de notification.
pub fn tray_actions() -> async_channel::Receiver<TrayAction> {
    tray_pipe().1.clone()
}

#[cfg(windows)]
pub use windows_impl::*;

#[cfg(not(windows))]
pub use unix_impl::*;

#[cfg(windows)]
mod windows_impl {
    use super::{ClickTarget, clicks_pipe};
    use crate::icons;
    use anyhow::{Context as _, Result};
    use windows::Win32::Foundation::{HWND, TRUE};
    use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    };
    use windows::Win32::UI::Shell::{ITaskbarList3, SetCurrentProcessExplicitAppUserModelID, TaskbarList};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateIconIndirect, DestroyIcon, FLASHW_TIMERNOFG, FLASHW_TRAY, FLASHWINFO, FlashWindowEx, HICON, ICONINFO,
        SW_HIDE, SW_RESTORE, SetForegroundWindow, ShowWindow,
    };
    use windows::core::{HSTRING, PCWSTR};

    /// Identifiant de l'application pour Windows (notifications, barre des tâches).
    pub const AUMID: &str = "Matterfew.Client";
    use super::DISPLAY_NAME;

    /// Fermer la fenêtre la masque dans la zone de notification.
    pub const CLOSE_TO_TRAY: bool = true;

    /// À appeler au démarrage : identité de l'application auprès de Windows.
    /// Windows n'affiche les notifications d'une application non installée
    /// que si un raccourci du menu Démarrer porte son identifiant : on crée
    /// (ou met à jour) ce raccourci, et on déclare nom et icône dans le
    /// registre utilisateur. Aucun droit administrateur n'est nécessaire.
    pub fn init() {
        unsafe {
            let _ = SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(AUMID));
        }
        if let Err(err) = register_identity() {
            eprintln!("identité Windows non enregistrée : {err:#}");
        }
        if let Err(err) = start_menu_shortcut() {
            eprintln!("raccourci du menu Démarrer non créé : {err:#}");
        }
    }

    /// Raccourci « Matterfew » du menu Démarrer, portant l'identifiant.
    fn start_menu_shortcut() -> Result<()> {
        use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
        use windows::Win32::System::Com::IPersistFile;
        use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PropVariantClear};
        use windows::Win32::System::Variant::VT_LPWSTR;
        use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
        use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
        use windows::core::Interface;

        let exe = std::env::current_exe()?;
        let programs =
            dirs::data_dir().context("dossier AppData introuvable")?.join(r"Microsoft\Windows\Start Menu\Programs");
        let lnk = programs.join(format!("{DISPLAY_NAME}.lnk"));
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            link.SetPath(&HSTRING::from(exe.as_os_str()))?;
            if let Some(dir) = exe.parent() {
                link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()))?;
            }
            link.SetDescription(&HSTRING::from(DISPLAY_NAME))?;

            // Valeur texte (VT_LPWSTR) allouée par COM, libérée par PropVariantClear.
            let wide: Vec<u16> = AUMID.encode_utf16().chain(std::iter::once(0)).collect();
            let buffer = windows::Win32::System::Com::CoTaskMemAlloc(wide.len() * 2) as *mut u16;
            anyhow::ensure!(!buffer.is_null(), "mémoire insuffisante");
            std::ptr::copy_nonoverlapping(wide.as_ptr(), buffer, wide.len());
            let mut value = PROPVARIANT::default();
            (*value.Anonymous.Anonymous).vt = VT_LPWSTR;
            (*value.Anonymous.Anonymous).Anonymous.pwszVal = windows::core::PWSTR(buffer);

            let store: IPropertyStore = link.cast()?;
            let result = store.SetValue(&PKEY_AppUserModel_ID, &value).and_then(|_| store.Commit());
            let _ = PropVariantClear(&mut value);
            result?;

            let file: IPersistFile = link.cast()?;
            file.Save(&HSTRING::from(lnk.as_os_str()), true)?;
        }
        Ok(())
    }

    fn register_identity() -> Result<()> {
        let dir = dirs::data_local_dir().context("dossier local introuvable")?.join("matterfew");
        std::fs::create_dir_all(&dir)?;
        let icon = dir.join("icon.png");
        image::save_buffer(&icon, &icons::app_icon(256), 256, 256, image::ExtendedColorType::Rgba8)?;
        let key = windows_registry::CURRENT_USER.create(format!(r"Software\Classes\AppUserModelId\{AUMID}"))?;
        key.set_string("DisplayName", DISPLAY_NAME)?;
        key.set_string("IconUri", icon.to_string_lossy().as_ref())?;
        key.set_string("IconBackgroundColor", "FF1C58D9")?;
        Ok(())
    }

    /// Notification Windows ; un clic envoie `target` à l'interface. Muette :
    /// le son choisi dans les paramètres est joué par l'application.
    ///
    /// Les notifications affichées restent référencées ici : si l'objet était
    /// libéré, Windows ne pourrait plus nous prévenir du clic. Une seule
    /// notification par canal (étiquette = canal), comme le client officiel.
    pub fn show_notification(title: &str, body: &str, target: Option<ClickTarget>) -> Result<()> {
        use std::sync::Mutex;
        use windows::Data::Xml::Dom::XmlDocument;
        use windows::Foundation::TypedEventHandler;
        use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

        static LIVE: Mutex<Vec<ToastNotification>> = Mutex::new(Vec::new());

        let xml = XmlDocument::new()?;
        xml.LoadXml(&HSTRING::from(format!(
            r#"<toast><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual><audio silent="true"/></toast>"#,
            xml_escape(title),
            xml_escape(body)
        )))?;
        let toast = ToastNotification::CreateToastNotification(&xml)?;
        if let Some(target) = &target {
            toast.SetTag(&HSTRING::from(&target.channel_id))?;
            toast.SetGroup(&HSTRING::from("matterfew"))?;
        }
        let sender = clicks_pipe().0.clone();
        toast.Activated(&TypedEventHandler::new(move |_, _| {
            if let Some(target) = &target {
                let _ = sender.send_blocking(target.clone());
            }
            Ok(())
        }))?;
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID))?.Show(&toast)?;

        let mut live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
        live.push(toast);
        if live.len() > 30 {
            live.remove(0);
        }
        Ok(())
    }

    fn xml_escape(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }

    /// Titre de la fenêtre principale (sert à retrouver l'instance déjà lancée).
    pub fn window_title() -> String {
        t!("app.window_title").to_string()
    }

    /// Une seule instance : si l'application tourne déjà (éventuellement
    /// réduite dans la zone de notification), on ramène sa fenêtre au premier
    /// plan et on renvoie `false` pour que celle-ci s'arrête.
    pub fn single_instance() -> bool {
        use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
        use windows::Win32::System::Threading::CreateMutexW;
        use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
        unsafe {
            // Le verrou vit jusqu'à la fin du processus (jamais refermé).
            let Ok(_mutex) = CreateMutexW(None, true, &HSTRING::from(format!(r"Local\{AUMID}.instance"))) else {
                return true;
            };
            if GetLastError() != ERROR_ALREADY_EXISTS {
                return true;
            }
            if let Ok(hwnd) = FindWindowW(PCWSTR::null(), &HSTRING::from(window_title())) {
                show_window(hwnd.0 as isize);
            }
            false
        }
    }

    /// Poignée Win32 de la fenêtre GPUI.
    pub fn hwnd(window: &gpui_kit::Window) -> Option<isize> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        match HasWindowHandle::window_handle(window).ok()?.as_raw() {
            RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
            _ => None,
        }
    }

    fn to_hwnd(h: isize) -> HWND {
        HWND(h as *mut core::ffi::c_void)
    }

    /// Fait clignoter le bouton de la barre des tâches jusqu'au retour sur la fenêtre.
    pub fn flash(hwnd: isize) {
        let info = FLASHWINFO {
            cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
            hwnd: to_hwnd(hwnd),
            dwFlags: FLASHW_TRAY | FLASHW_TIMERNOFG,
            uCount: 3,
            dwTimeout: 0,
        };
        unsafe {
            let _ = FlashWindowEx(&info);
        }
    }

    /// The window is active again. Nothing to do here: `FLASHW_TIMERNOFG`
    /// already stops the flashing when the window comes to the foreground.
    pub fn attention_seen(_hwnd: isize) {}

    /// The channel was read. Nothing to take back here: toasts leave on their own.
    pub fn close_notifications(_channel_id: &str) {}

    pub fn hide_window(hwnd: isize) {
        unsafe {
            let _ = ShowWindow(to_hwnd(hwnd), SW_HIDE);
        }
    }

    pub fn show_window(hwnd: isize) {
        unsafe {
            let _ = ShowWindow(to_hwnd(hwnd), SW_RESTORE);
            let _ = SetForegroundWindow(to_hwnd(hwnd));
        }
    }

    /// Icône Windows à partir de pixels RGBA.
    unsafe fn hicon_from_rgba(rgba: &[u8], size: i32) -> Result<HICON> {
        let bgra: Vec<u8> = rgba.chunks(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
        let mask_bits = vec![0u8; (size * size / 8) as usize];
        unsafe {
            let color = CreateBitmap(size, size, 1, 32, Some(bgra.as_ptr().cast()));
            let mask = CreateBitmap(size, size, 1, 1, Some(mask_bits.as_ptr().cast()));
            let info = ICONINFO { fIcon: TRUE, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
            let icon = CreateIconIndirect(&info);
            let _ = DeleteObject(color.into());
            let _ = DeleteObject(mask.into());
            Ok(icon?)
        }
    }

    /// Pastille de la barre des tâches (rien si tout est lu).
    /// À appeler depuis le thread de l'interface.
    pub fn set_taskbar_badge(hwnd: isize, mentions: i64, unread: bool, expired: bool) {
        thread_local! {
            static TASKBAR: Option<ITaskbarList3> = unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER)
                    .ok()
                    .filter(|t: &ITaskbarList3| t.HrInit().is_ok())
            };
        }
        TASKBAR.with(|taskbar| {
            let Some(taskbar) = taskbar else { return };
            unsafe {
                match icons::overlay_badge(16, mentions, unread, expired) {
                    Some(px) => {
                        let Ok(icon) = hicon_from_rgba(&px, 16) else { return };
                        let label = if expired {
                            t!("tray.session_expired").to_string()
                        } else if mentions > 0 {
                            if mentions == 1 {
                                t!("tray.mentions.one").to_string()
                            } else {
                                t!("tray.mentions.other", count = mentions).to_string()
                            }
                        } else {
                            t!("tray.unread").to_string()
                        };
                        let _ = taskbar.SetOverlayIcon(to_hwnd(hwnd), icon, &HSTRING::from(label));
                        let _ = DestroyIcon(icon);
                    }
                    None => {
                        let _ = taskbar.SetOverlayIcon(to_hwnd(hwnd), HICON::default(), PCWSTR::null());
                    }
                }
            }
        });
    }

    pub use super::sys_tray::Tray;
}

/// Icône de la zone de notification (Windows) ou de la barre des menus
/// (macOS), avec la bibliothèque `tray-icon`.
#[cfg(any(windows, target_os = "macos"))]
mod sys_tray {
    use super::{DISPLAY_NAME, TrayAction, tray_pipe};
    use crate::icons;

    /// Icône de la zone de notification. À créer sur le thread principal.
    pub struct Tray {
        icon: tray_icon::TrayIcon,
        state: (i64, bool),
    }

    impl Tray {
        pub fn new() -> Option<Self> {
            use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
            use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

            // macOS : AppKit n'accepte l'icône que depuis le fil principal (les
            // tests de bout en bout construisent l'interface sur un autre fil).
            #[cfg(target_os = "macos")]
            objc2::MainThreadMarker::new()?;

            let menu = Menu::new();
            let open = MenuItem::with_id("open", t!("tray.open"), true, None);
            let quit = MenuItem::with_id("quit", t!("tray.quit"), true, None);
            menu.append_items(&[&open, &PredefinedMenuItem::separator(), &quit]).ok()?;

            let icon = tray_icon::Icon::from_rgba(icons::tray_icon(32, 0, false), 32, 32).ok()?;
            let tray = TrayIconBuilder::new()
                .with_icon(icon)
                .with_tooltip(DISPLAY_NAME)
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(false)
                .build()
                .ok()?;

            let sender = tray_pipe().0.clone();
            TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
                let left_up = matches!(
                    event,
                    TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
                        | TrayIconEvent::DoubleClick { button: MouseButton::Left, .. }
                );
                if left_up {
                    let _ = sender.send_blocking(TrayAction::Show);
                }
            }));
            let sender = tray_pipe().0.clone();
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                let action = match event.id.0.as_str() {
                    "open" => TrayAction::Show,
                    "quit" => TrayAction::Quit,
                    _ => return,
                };
                let _ = sender.send_blocking(action);
            }));
            Some(Self { icon: tray, state: (0, false) })
        }

        /// Met à jour l'icône et l'infobulle selon les mentions et non-lus.
        pub fn update(&mut self, mentions: i64, unread: bool) {
            if self.state == (mentions, unread) {
                return;
            }
            self.state = (mentions, unread);
            if let Ok(icon) = tray_icon::Icon::from_rgba(icons::tray_icon(32, mentions, unread), 32, 32) {
                let _ = self.icon.set_icon(Some(icon));
            }
            let tooltip = match (mentions, unread) {
                (0, false) => DISPLAY_NAME.to_string(),
                (0, true) => format!("{DISPLAY_NAME} — {}", t!("tray.unread_lower")),
                (1, _) => format!("{DISPLAY_NAME} — {}", t!("tray.mentions.one")),
                (n, _) => format!("{DISPLAY_NAME} — {}", t!("tray.mentions.other", count = n)),
            };
            let _ = self.icon.set_tooltip(Some(tooltip));
        }
    }
}

/// Nom affiché de l'application (raccourcis, infobulles).
const DISPLAY_NAME: &str = "Matterfew";

/// Linux (GNOME, KDE…) et macOS.
#[cfg(not(windows))]
mod unix_impl {
    use super::{ClickTarget, TrayAction, clicks_pipe, tray_pipe};

    /// Pas de fermeture vers la zone de notification hors Windows : fermer la
    /// fenêtre quitte l'application, comme la plupart des applications GNOME.
    pub const CLOSE_TO_TRAY: bool = false;

    pub fn window_title() -> String {
        t!("app.window_title").to_string()
    }

    /// Identifiant de l'application (bundle macOS, nom D-Bus, Flatpak).
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    const APP_ID: &str = "io.github.orthagh.Matterfew";

    pub fn init() {
        // macOS : les notifications portent le nom et l'icône de l'application.
        #[cfg(target_os = "macos")]
        let _ = notify_rust::set_application(APP_ID);
    }

    /// Une seule instance : une prise locale (socket Unix) dans le dossier
    /// d'exécution de l'utilisateur. Si une instance répond, on lui demande
    /// de revenir au premier plan, et on s'arrête.
    pub fn single_instance() -> bool {
        use std::io::{Read as _, Write as _};
        use std::os::unix::net::{UnixListener, UnixStream};

        let dir = dirs::runtime_dir().or_else(dirs::cache_dir).unwrap_or_else(std::env::temp_dir);
        let path = dir.join("matterfew.sock");
        if let Ok(mut stream) = UnixStream::connect(&path) {
            let _ = stream.write_all(b"show");
            return false;
        }
        // Prise orpheline (instance arrêtée brutalement) : on la remplace.
        let _ = std::fs::remove_file(&path);
        let Ok(listener) = UnixListener::bind(&path) else { return true };
        let sender = tray_pipe().0.clone();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut buf = [0u8; 8];
                let _ = stream.read(&mut buf);
                let _ = sender.send_blocking(TrayAction::Show);
            }
        });
        true
    }

    /// Notification du bureau ; un clic ouvre le canal (actions XDG sous
    /// Linux, Centre de notifications sous macOS).
    pub fn show_notification(title: &str, body: &str, target: Option<ClickTarget>) -> anyhow::Result<()> {
        let mut notification = notify_rust::Notification::new();
        notification.summary(title).body(body).appname(super::DISPLAY_NAME);
        #[cfg(target_os = "linux")]
        {
            notification.action("default", &t!("tray.open"));
            notification.hint(notify_rust::Hint::DesktopEntry(linux::desktop_entry()));
            // Le son choisi dans les paramètres est joué par l'application.
            notification.hint(notify_rust::Hint::SuppressSound(true));
        }
        let handle = notification.show()?;
        if let Some(target) = target {
            #[cfg(target_os = "linux")]
            linux::remember_notification(&target.channel_id, handle.id());
            let sender = clicks_pipe().0.clone();
            std::thread::spawn(move || {
                handle.wait_for_action(|action| {
                    if action == "default" {
                        let _ = sender.send_blocking(target);
                    }
                });
            });
        }
        Ok(())
    }

    /// Pas de poignée de fenêtre ici : une valeur fixe, pour que l'interface
    /// mette à jour la pastille et signale les mentions comme sous Windows.
    pub fn hwnd(_window: &gpui_kit::Window) -> Option<isize> {
        Some(0)
    }

    /// Une mention arrive alors que la fenêtre n'a pas le focus.
    pub fn flash(_hwnd: isize) {
        #[cfg(target_os = "linux")]
        linux::urgent();
        #[cfg(target_os = "macos")]
        macos::bounce();
    }

    /// The window is active again: the launcher stops asking for attention.
    pub fn attention_seen(_hwnd: isize) {
        #[cfg(target_os = "linux")]
        linux::calm();
    }

    /// The channel was read: its notifications leave the desktop's list.
    #[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
    pub fn close_notifications(channel_id: &str) {
        #[cfg(target_os = "linux")]
        linux::close_notifications(channel_id);
    }

    pub fn hide_window(_hwnd: isize) {}
    pub fn show_window(_hwnd: isize) {}

    /// Pastille de l'icône du lanceur (Linux) ou du Dock (macOS).
    pub fn set_taskbar_badge(_hwnd: isize, mentions: i64, unread: bool, expired: bool) {
        #[cfg(target_os = "linux")]
        linux::badge(mentions, unread, expired);
        #[cfg(target_os = "macos")]
        macos::badge(mentions, unread, expired);
    }

    #[cfg(target_os = "macos")]
    pub use super::sys_tray::Tray;
    #[cfg(target_os = "linux")]
    pub use linux::Tray;

    /// Linux : pastille du lanceur (« LauncherEntry », compris par KDE Plasma,
    /// le dock d'Ubuntu et Dash to Dock) et icône StatusNotifierItem (KDE ;
    /// GNOME avec l'extension AppIndicator, installée d'office sur Ubuntu).
    #[cfg(target_os = "linux")]
    mod linux {
        use super::super::{TrayAction, tray_pipe};
        use crate::icons;
        use std::collections::HashMap;
        use std::sync::mpsc::{Sender, channel};
        use std::sync::{Mutex, OnceLock};

        /// Fichier `.desktop` de l'application, selon l'emballage.
        pub fn desktop_entry() -> String {
            if let Ok(id) = std::env::var("FLATPAK_ID") {
                return id;
            }
            if let Ok(name) = std::env::var("SNAP_NAME") {
                return format!("{name}_matterfew");
            }
            "matterfew".to_string()
        }

        /// Launcher entry update: the mention count, or the urgency flag alone.
        /// The launcher merges the properties it receives, so an update only
        /// carries what it changes.
        enum Update {
            Count(i64),
            Urgent(bool),
            /// Not a launcher update, but it shares the thread's bus connection.
            CloseNotification(u32),
        }

        /// Notifications still listed by the desktop, by channel.
        fn open_notifications() -> &'static Mutex<HashMap<String, Vec<u32>>> {
            static OPEN: OnceLock<Mutex<HashMap<String, Vec<u32>>>> = OnceLock::new();
            OPEN.get_or_init(Default::default)
        }

        pub fn remember_notification(channel_id: &str, id: u32) {
            let mut open = open_notifications().lock().unwrap_or_else(|e| e.into_inner());
            open.entry(channel_id.to_string()).or_default().push(id);
        }

        /// The dock also counts the notifications left in the desktop's list,
        /// so a channel that was read takes its own back.
        pub fn close_notifications(channel_id: &str) {
            let ids = open_notifications().lock().unwrap_or_else(|e| e.into_inner()).remove(channel_id);
            if let (Some(ids), Some(tx)) = (ids, launcher()) {
                for id in ids {
                    let _ = tx.send(Update::CloseNotification(id));
                }
            }
        }

        /// Les signaux D-Bus partent d'un fil dédié (sa propre boucle tokio et
        /// sa connexion au bus de session), pour ne jamais bloquer l'interface.
        fn launcher() -> &'static Option<Sender<Update>> {
            static SENDER: OnceLock<Option<Sender<Update>>> = OnceLock::new();
            SENDER.get_or_init(|| {
                let (tx, rx) = channel::<Update>();
                let uri = format!("application://{}.desktop", desktop_entry());
                std::thread::Builder::new()
                    .name("launcher-entry".into())
                    .spawn(move || {
                        let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
                            return;
                        };
                        rt.block_on(async move {
                            let Ok(conn) = zbus::Connection::session().await else { return };
                            while let Ok(update) = rx.recv() {
                                let mut props: HashMap<&str, zbus::zvariant::Value> = HashMap::new();
                                match update {
                                    Update::Count(count) => {
                                        props.insert("count", count.into());
                                        props.insert("count-visible", (count > 0).into());
                                    }
                                    Update::Urgent(urgent) => {
                                        props.insert("urgent", urgent.into());
                                    }
                                    Update::CloseNotification(id) => {
                                        let _ = conn
                                            .call_method(
                                                Some("org.freedesktop.Notifications"),
                                                "/org/freedesktop/Notifications",
                                                Some("org.freedesktop.Notifications"),
                                                "CloseNotification",
                                                &id,
                                            )
                                            .await;
                                        continue;
                                    }
                                }
                                let _ = conn
                                    .emit_signal(
                                        None::<&str>,
                                        "/io/github/orthagh/Matterfew",
                                        "com.canonical.Unity.LauncherEntry",
                                        "Update",
                                        &(uri.as_str(), props),
                                    )
                                    .await;
                            }
                        });
                    })
                    .ok()
                    .map(|_| tx)
            })
        }

        pub fn badge(mentions: i64, unread: bool, expired: bool) {
            if let Some(tx) = launcher() {
                // The launcher only shows a number: the mentions; "1" for an
                // expired session, nothing for plain unread channels.
                let count = if expired { 1 } else { mentions };
                let _ = tx.send(Update::Count(count));
                // Nothing left to read: the launcher stops asking for attention.
                if count == 0 {
                    let _ = tx.send(Update::Urgent(false));
                }
                let _ = unread;
            }
        }

        pub fn urgent() {
            if let Some(tx) = launcher() {
                let _ = tx.send(Update::Urgent(true));
            }
        }

        pub fn calm() {
            if let Some(tx) = launcher() {
                let _ = tx.send(Update::Urgent(false));
            }
        }

        /// Icône StatusNotifierItem.
        struct SniTray {
            mentions: i64,
            unread: bool,
        }

        impl ksni::Tray for SniTray {
            fn id(&self) -> String {
                "matterfew".into()
            }
            fn title(&self) -> String {
                super::super::DISPLAY_NAME.into()
            }
            fn icon_pixmap(&self) -> Vec<ksni::Icon> {
                // tray_icon : RGBA ; StatusNotifierItem : ARGB.
                let size = 32;
                let rgba = icons::tray_icon(size, self.mentions, self.unread);
                let data = rgba.as_chunks::<4>().0.iter().flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
                vec![ksni::Icon { width: size as i32, height: size as i32, data }]
            }
            fn tool_tip(&self) -> ksni::ToolTip {
                let title = if self.mentions > 0 {
                    if self.mentions == 1 {
                        t!("tray.mentions.one").to_string()
                    } else {
                        t!("tray.mentions.other", count = self.mentions).to_string()
                    }
                } else if self.unread {
                    t!("tray.unread").to_string()
                } else {
                    super::super::DISPLAY_NAME.into()
                };
                ksni::ToolTip { title, ..Default::default() }
            }
            fn activate(&mut self, _x: i32, _y: i32) {
                let _ = tray_pipe().0.send_blocking(TrayAction::Show);
            }
            fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
                use ksni::menu::StandardItem;
                vec![
                    StandardItem {
                        label: t!("tray.open").to_string(),
                        activate: Box::new(|_| {
                            let _ = tray_pipe().0.send_blocking(TrayAction::Show);
                        }),
                        ..Default::default()
                    }
                    .into(),
                    ksni::MenuItem::Separator,
                    StandardItem {
                        label: t!("tray.quit").to_string(),
                        activate: Box::new(|_| {
                            let _ = tray_pipe().0.send_blocking(TrayAction::Quit);
                        }),
                        ..Default::default()
                    }
                    .into(),
                ]
            }
        }

        pub struct Tray {
            handle: ksni::blocking::Handle<SniTray>,
        }

        impl Tray {
            pub fn new() -> Option<Self> {
                use ksni::blocking::TrayMethods as _;
                // Dans Flatpak, pas de nom D-Bus « bien connu » (interdit par le bac à sable).
                let sandboxed = std::env::var("FLATPAK_ID").is_ok();
                let handle = SniTray { mentions: 0, unread: false }.disable_dbus_name(sandboxed).spawn().ok()?;
                Some(Self { handle })
            }

            pub fn update(&mut self, mentions: i64, unread: bool) {
                self.handle.update(|t| {
                    t.mentions = mentions;
                    t.unread = unread;
                });
            }
        }
    }

    /// macOS : pastille et rebond de l'icône du Dock.
    #[cfg(target_os = "macos")]
    mod macos {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSApplication, NSRequestUserAttentionType};
        use objc2_foundation::NSString;

        pub fn badge(mentions: i64, unread: bool, expired: bool) {
            // Appelé depuis l'interface, donc le fil principal.
            let Some(mtm) = MainThreadMarker::new() else { return };
            let label = if expired {
                Some("!".to_string())
            } else if mentions > 0 {
                Some(if mentions > 99 { "99+".to_string() } else { mentions.to_string() })
            } else if unread {
                Some("•".to_string())
            } else {
                None
            };
            let tile = NSApplication::sharedApplication(mtm).dockTile();
            match label {
                Some(text) => tile.setBadgeLabel(Some(&NSString::from_str(&text))),
                None => tile.setBadgeLabel(None),
            }
        }

        pub fn bounce() {
            let Some(mtm) = MainThreadMarker::new() else { return };
            NSApplication::sharedApplication(mtm)
                .requestUserAttention(NSRequestUserAttentionType::InformationalRequest);
        }
    }
}

#[cfg(all(test, windows))]
mod manual_tests {
    /// Affiche une vraie notification (test manuel : `cargo test toast -- --ignored`).
    #[test]
    #[ignore]
    fn toast() {
        super::init();
        let target = super::ClickTarget { team_id: "t1".into(), channel_id: "general".into() };
        super::show_notification("Léa Bernard dans Général", "Test de notification matterfew", Some(target)).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(6));
    }
}
