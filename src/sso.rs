//! "Log in via Mattermost": a small embedded browser window (WebView2 on
//! Windows, WebKitGTK on Linux) shows the server's real login page. The user
//! logs in as usual (GitLab, password, two-factor authentication…), then we
//! read the `MMAUTHTOKEN` session cookie Mattermost sets once the login
//! succeeds.
//!
//! That window runs in a separate process (`matterfew --sso-login URL`) so
//! its event loop does not mix with GPUI's. The token is handed to the main
//! process on standard output.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, bail};

const TOKEN_PREFIX: &str = "MMAUTHTOKEN ";

/// Exit code of the login window when the user closes it without logging in.
const EXIT_CLOSED: i32 = 2;

/// Folder where the embedded browser keeps its cookies (so the user does not
/// have to log in to GitLab again every time).
pub fn webview_data_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("matterfew").join("webview"))
}

/// Clears the embedded browser's session (on logout).
pub fn clear_webview_data() {
    if let Some(dir) = webview_data_dir() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Application side: opens the login window and waits for the token.
/// Blocking, to be called from a background thread.
pub fn login_in_browser(server_url: &str) -> Result<String> {
    let exe = std::env::current_exe().with_context(|| t!("errors.app_path").to_string())?;
    let mut child = Command::new(exe)
        .args(["--sso-login", server_url])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| t!("sso.open_failed").to_string())?;
    let stdout = child.stdout.take().with_context(|| t!("sso.output").to_string())?;
    let token = BufReader::new(stdout)
        .lines()
        .map_while(|l| l.ok())
        .find_map(|line| line.strip_prefix(TOKEN_PREFIX).map(str::to_string));
    let closed_by_user = child.wait().is_ok_and(|status| status.code() == Some(EXIT_CLOSED));
    match token {
        Some(token) if !token.is_empty() => Ok(token),
        // Any other exit than a window closed by the user is a crash, not a cancellation.
        _ if closed_by_user => bail!("{}", t!("sso.cancelled")),
        _ => bail!("{}", t!("sso.open_failed")),
    }
}

/// Login window side: entry point of `--sso-login URL`.
pub fn run_login_window(server_url: &str) -> ! {
    use tao::dpi::LogicalSize;
    use tao::event::{Event, StartCause, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::window::WindowBuilder;
    use wry::{WebContext, WebViewBuilder};

    let server = server_url.trim_end_matches('/').to_string();
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(t!("sso.window_title"))
        .with_inner_size(LogicalSize::new(560.0, 760.0))
        .build(&event_loop)
        .expect("login window");

    let mut context = WebContext::new(webview_data_dir());
    let builder = WebViewBuilder::new_with_web_context(&mut context)
        // Skips the web client's "Where would you like to view this?" page.
        .with_initialization_script("try { localStorage.setItem('__landingPageSeen__', 'true'); } catch (e) {}")
        .with_url(format!("{server}/login"));
    // On Linux, building from the window handle only works under X11; the GTK
    // container tao gives every window works under Wayland as well.
    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        builder.build_gtk(window.default_vbox().expect("login window has no GTK container"))
    };
    #[cfg(not(target_os = "linux"))]
    let webview = builder.build(&window);
    let webview = webview.expect("embedded browser unavailable");

    let started = Instant::now();
    event_loop.run(move |event, _, control_flow| {
        let _keep_alive = (&context, &window);
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(400));
        match event {
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                // Wait until the login pages are left: an old cookie still in
                // the browser must not be taken for a successful login.
                let url = webview.url().unwrap_or_default();
                let path = url.strip_prefix(&server).unwrap_or("/login");
                let on_login_pages = ["/login", "/oauth", "/signup", "/reset_password", "/mfa", "/do_verify_email"]
                    .iter()
                    .any(|p| path.starts_with(p));
                if !url.starts_with(&server) || on_login_pages || started.elapsed() < Duration::from_secs(1) {
                    return;
                }
                if let Ok(cookies) = webview.cookies_for_url(&server)
                    && let Some(cookie) = cookies.iter().find(|c| c.name() == "MMAUTHTOKEN")
                {
                    let mut out = std::io::stdout();
                    let _ = writeln!(out, "{TOKEN_PREFIX}{}", cookie.value());
                    let _ = out.flush();
                    std::process::exit(0);
                }
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => std::process::exit(EXIT_CLOSED),
            _ => {}
        }
    })
}
