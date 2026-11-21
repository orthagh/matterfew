//! Intégration au bureau Windows, sur le modèle du client de bureau officiel :
//! - notifications avec l'identité de l'application (nom, icône) et clic qui
//!   ouvre le bon canal ;
//! - pastille sur l'icône de la barre des tâches (mentions, non-lus) ;
//! - icône dans la zone de notification, avec un menu, et fermeture de la
//!   fenêtre qui la réduit dans cette zone ;
//! - clignotement de la barre des tâches à l'arrivée d'une notification.
//!
//! Hors Windows, ces fonctions ne font rien (les notifications passent alors
//! par `notify-rust`).

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
pub use other::*;

#[cfg(windows)]
mod windows_impl {
    use super::{ClickTarget, TrayAction, clicks_pipe, tray_pipe};
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
    pub const AUMID: &str = "MmGpui.Client";
    const DISPLAY_NAME: &str = "Mattermost (mm-gpui)";

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

    /// Raccourci « Mattermost (mm-gpui) » du menu Démarrer, portant l'identifiant.
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
        let dir = dirs::data_local_dir().context("dossier local introuvable")?.join("mm-gpui");
        std::fs::create_dir_all(&dir)?;
        let icon = dir.join("icon.png");
        image::save_buffer(&icon, &icons::app_icon(256), 256, 256, image::ExtendedColorType::Rgba8)?;
        let key = windows_registry::CURRENT_USER.create(format!(r"Software\Classes\AppUserModelId\{AUMID}"))?;
        key.set_string("DisplayName", DISPLAY_NAME)?;
        key.set_string("IconUri", icon.to_string_lossy().as_ref())?;
        key.set_string("IconBackgroundColor", "FF1C58D9")?;
        Ok(())
    }

    /// Notification Windows ; un clic envoie `target` à l'interface.
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
            r#"<toast><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual></toast>"#,
            xml_escape(title),
            xml_escape(body)
        )))?;
        let toast = ToastNotification::CreateToastNotification(&xml)?;
        if let Some(target) = &target {
            toast.SetTag(&HSTRING::from(&target.channel_id))?;
            toast.SetGroup(&HSTRING::from("mm-gpui"))?;
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
    pub const WINDOW_TITLE: &str = "Mattermost — client GPUI";

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
            if let Ok(hwnd) = FindWindowW(PCWSTR::null(), &HSTRING::from(WINDOW_TITLE)) {
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
                            "Session expirée".to_string()
                        } else if mentions > 0 {
                            format!("{mentions} mention(s)")
                        } else {
                            "Messages non lus".to_string()
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

    /// Icône de la zone de notification. À créer sur le thread principal.
    pub struct Tray {
        icon: tray_icon::TrayIcon,
        state: (i64, bool),
    }

    impl Tray {
        pub fn new() -> Option<Self> {
            use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
            use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

            let menu = Menu::new();
            let open = MenuItem::with_id("open", "Ouvrir", true, None);
            let quit = MenuItem::with_id("quit", "Quitter", true, None);
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
                (0, true) => format!("{DISPLAY_NAME} — messages non lus"),
                (n, _) => format!("{DISPLAY_NAME} — {n} mention(s)"),
            };
            let _ = self.icon.set_tooltip(Some(tooltip));
        }
    }
}

#[cfg(not(windows))]
mod other {
    use super::ClickTarget;

    pub const WINDOW_TITLE: &str = "Mattermost — client GPUI";
    pub fn single_instance() -> bool {
        true
    }
    pub fn init() {}
    pub fn show_notification(title: &str, body: &str, _target: Option<ClickTarget>) -> anyhow::Result<()> {
        notify_rust::Notification::new().summary(title).body(body).show()?;
        Ok(())
    }
    pub fn hwnd(_window: &gpui_kit::Window) -> Option<isize> {
        None
    }
    pub fn flash(_hwnd: isize) {}
    pub fn hide_window(_hwnd: isize) {}
    pub fn show_window(_hwnd: isize) {}
    pub fn set_taskbar_badge(_hwnd: isize, _mentions: i64, _unread: bool, _expired: bool) {}

    pub struct Tray;
    impl Tray {
        pub fn new() -> Option<Self> {
            None
        }
        pub fn update(&mut self, _mentions: i64, _unread: bool) {}
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
        super::show_notification("Léa Bernard dans Général", "Test de notification mm-gpui", Some(target)).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(6));
    }
}
