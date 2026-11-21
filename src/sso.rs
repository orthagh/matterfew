//! Connexion « via Mattermost » : une petite fenêtre de navigateur intégré
//! (WebView2 sous Windows) affiche la vraie page de connexion du serveur.
//! L'utilisateur s'y connecte comme d'habitude (GitLab, mot de passe, double
//! authentification…), puis on récupère le cookie de session `MMAUTHTOKEN`
//! que Mattermost dépose une fois la connexion réussie.
//!
//! Cette fenêtre tourne dans un processus séparé (`mm-gpui --sso-login URL`)
//! pour ne pas mélanger sa boucle d'événements avec celle de GPUI. Le jeton
//! est transmis au processus principal sur la sortie standard.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, bail};

const TOKEN_PREFIX: &str = "MMAUTHTOKEN ";

/// Dossier où le navigateur intégré garde ses cookies (pour ne pas avoir à
/// se reconnecter à GitLab à chaque fois).
pub fn webview_data_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("mm-gpui").join("webview"))
}

/// Efface la session du navigateur intégré (à la déconnexion).
pub fn clear_webview_data() {
    if let Some(dir) = webview_data_dir() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Côté application : lance la fenêtre de connexion et attend le jeton.
/// Bloquant, à appeler depuis un thread d'arrière-plan.
pub fn login_in_browser(server_url: &str) -> Result<String> {
    let exe = std::env::current_exe().context("chemin de l'application introuvable")?;
    let mut child = Command::new(exe)
        .args(["--sso-login", server_url])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("impossible d'ouvrir la fenêtre de connexion")?;
    let stdout = child.stdout.take().context("sortie de la fenêtre de connexion")?;
    let token = BufReader::new(stdout)
        .lines()
        .map_while(|l| l.ok())
        .find_map(|line| line.strip_prefix(TOKEN_PREFIX).map(str::to_string));
    let _ = child.wait();
    match token {
        Some(token) if !token.is_empty() => Ok(token),
        _ => bail!("Connexion annulée."),
    }
}

/// Côté fenêtre de connexion : point d'entrée de `--sso-login URL`.
pub fn run_login_window(server_url: &str) -> ! {
    use tao::dpi::LogicalSize;
    use tao::event::{Event, StartCause, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::window::WindowBuilder;
    use wry::{WebContext, WebViewBuilder};

    let server = server_url.trim_end_matches('/').to_string();
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Connexion à Mattermost")
        .with_inner_size(LogicalSize::new(560.0, 760.0))
        .build(&event_loop)
        .expect("fenêtre de connexion");

    let mut context = WebContext::new(webview_data_dir());
    let webview = WebViewBuilder::new_with_web_context(&mut context)
        // Évite la page « Où souhaitez-vous voir ceci ? » du client web.
        .with_initialization_script("try { localStorage.setItem('__landingPageSeen__', 'true'); } catch (e) {}")
        .with_url(format!("{server}/login"))
        .build(&window)
        .expect("navigateur intégré (WebView2) indisponible");

    let started = Instant::now();
    event_loop.run(move |event, _, control_flow| {
        let _keep_alive = (&context, &window);
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(400));
        match event {
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                // On attend d'avoir quitté les pages de connexion : un vieux
                // cookie resté dans le navigateur ne doit pas être pris pour
                // une connexion réussie.
                let url = webview.url().unwrap_or_default();
                let path = url.strip_prefix(&server).unwrap_or("/login");
                let on_login_pages = ["/login", "/oauth", "/signup", "/reset_password", "/mfa", "/do_verify_email"]
                    .iter()
                    .any(|p| path.starts_with(p));
                if !url.starts_with(&server) || on_login_pages || started.elapsed() < Duration::from_secs(1) {
                    return;
                }
                if let Ok(cookies) = webview.cookies_for_url(&server)
                    && let Some(cookie) = cookies.iter().find(|c| c.name() == "MMAUTHTOKEN") {
                        let mut out = std::io::stdout();
                        let _ = writeln!(out, "{TOKEN_PREFIX}{}", cookie.value());
                        let _ = out.flush();
                        std::process::exit(0);
                    }
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => std::process::exit(2),
            _ => {}
        }
    })
}
