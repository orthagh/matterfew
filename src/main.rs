// En version optimisée, pas de console noire à côté de la fenêtre sous Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[macro_use]
extern crate rust_i18n;

// Catalogues de traduction (locales/*.yml), anglais par défaut.
i18n!("locales", fallback = "en");

mod api;
mod autolink;
mod backend;
mod cache;
mod credentials;
mod demo;
mod desktop;
mod editor;
mod emoji;
mod fuzzy;
mod icons;
mod locale;
mod markdown;
mod migrate;
mod notify;
mod notify_rules;
mod pdf;
mod rich;
mod settings;
mod sidebar;
mod sound;
mod spell;
mod sso;
mod theme;
mod thumbs;
mod ui;

use gpui_kit::*;

// Icônes Lucide utilisées par l'application, en plus de celles des composants
// (seules celles-ci sont embarquées dans l'exécutable).
gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [
        Bold,
        Italic,
        Strikethrough,
        Code,
        Link,
        Heading,
        TextQuote,
        List,
        ListOrdered,
        SquareCode,
        Paperclip,
        SendHorizontal,
        Eye,
        EyeOff,
        Reply,
        Ellipsis,
        EllipsisVertical,
        Star,
        Bell,
        BellOff,
        Pin,
        Bookmark,
        Copy,
        Pencil,
        Trash,
        MailOpen,
        Hash,
        Lock,
        Users,
        ChevronDown,
        ChevronRight,
        X,
        LogOut,
        Moon,
        CircleMinus,
        CircleCheck,
        Circle,
        ArrowUpDown,
        CheckCheck,
        Search,
        FaceSlightlySmilingPlus,
        FaceSlightlySmiling,
        MessagesSquare,
        AtSign,
        Download,
        ExternalLink,
        Mail,
        Clock,
        Shield,
        FolderOpen,
        Type,
        Images,
        FileText,
        Link2,
        UserPlus,
        Info,
        PencilLine,
        SpellCheck,
        StarFill,
        Menu,
        ChevronUp,
        SquareTerminal,
        Plus,
        FolderPlus,
        Compass,
        SquarePen,
        BellRing,
        Archive,
        TriangleAlert,
        RotateCcw,
        Forward,
        ArrowUp,
        Settings,
        UserMinus,
        Keyboard,
        WifiOff,
        Monitor,
        PanelLeft,
        SlidersHorizontal,
        Volume2
    ]
);

/// Icônes de l'application + icônes par défaut de GPUI Kit.
struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<std::borrow::Cow<'static, [u8]>>> {
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

fn main() {
    locale::init();
    // Fenêtre de connexion « via Mattermost » (processus séparé, voir sso.rs).
    let args: Vec<String> = std::env::args().collect();
    if let Some(ix) = args.iter().position(|a| a == "--sso-login") {
        let url = args.get(ix + 1).cloned().unwrap_or_default();
        sso::run_login_window(&url);
    }
    // Déjà lancée : on ramène l'instance existante au premier plan.
    // Données de l'ancien nom (« mm-gpui ») : reprises une fois.
    if !args.iter().any(|a| a == "--demo") {
        migrate::run();
    }
    if !args.iter().any(|a| a == "--demo") && !desktop::single_instance() {
        return;
    }

    // Démo : MM_DEMO_SIZE=« 760x800 » pour voir la mise en page étroite.
    let (w, h): (f32, f32) = std::env::var("MM_DEMO_SIZE")
        .ok()
        .filter(|_| args.iter().any(|a| a == "--demo"))
        .and_then(|s| s.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))))
        .unwrap_or((1200., 800.));

    gpui_kit::application().with_assets(AppAssets).run(move |cx| {
        gpui_kit::init(cx);
        // Identité de l'application pour Windows (notifications, barre des tâches).
        desktop::init();
        // Thème clair ou sombre selon le réglage du système.
        gpui_kit::component::Theme::sync_system_appearance(None, cx);

        let bounds = Bounds::centered(None, size(px(w), px(h)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions { title: Some(desktop::window_title().into()), ..Default::default() }),
            ..Default::default()
        };

        gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| ui::ChatApp::new(window, cx)))
            .expect("impossible d'ouvrir la fenêtre");

        // Fermer la fenêtre quitte l'application.
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}
