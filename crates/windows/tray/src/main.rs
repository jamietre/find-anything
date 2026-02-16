//! find-tray: Windows system tray app for find-anything.
//!
//! Starts at login (registered by `find-watch install`), shows service status,
//! file counts, and provides quick actions for scan / start / stop.

// On non-Windows this binary is a stub.
#[cfg(not(windows))]
fn main() {
    eprintln!("find-tray is only supported on Windows.");
    std::process::exit(1);
}

#[cfg(windows)]
mod menu;
#[cfg(windows)]
mod poller;
#[cfg(windows)]
mod service_ctl;

#[cfg(windows)]
use std::path::PathBuf;
#[cfg(windows)]
use std::sync::mpsc;

#[cfg(windows)]
use anyhow::{Context, Result};
#[cfg(windows)]
use find_common::config::ClientConfig;
#[cfg(windows)]
use tray_icon::{
    menu::MenuEvent,
    TrayIcon, TrayIconBuilder, TrayIconEvent,
};
#[cfg(windows)]
use winit::{
    application::ApplicationHandler,
    event::Event,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
};

/// Events sent from the poller thread to the main thread.
#[cfg(windows)]
#[derive(Debug)]
pub enum AppEvent {
    StatusUpdate {
        service_running: bool,
        file_count: Option<u64>,
        source_count: Option<usize>,
    },
}

#[cfg(windows)]
fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "find_tray=info".into()),
        )
        .init();

    let config_path = parse_config_arg();
    let config_str = std::fs::read_to_string(&config_path)
        .with_context(|| format!("reading config {}", config_path.display()))?;
    let config: ClientConfig =
        toml::from_str(&config_str).context("parsing client config")?;

    let server_url = config.server.url.trim_end_matches('/').to_string();
    let token = config.server.token.clone();

    // Build event loop with user-event type for cross-thread messaging.
    let event_loop = EventLoop::<AppEvent>::with_user_event()
        .build()
        .context("creating event loop")?;

    let proxy = event_loop.create_proxy();

    // Spawn background poller; it sends AppEvent via the mpsc channel,
    // which we bridge to the winit proxy.
    let (tx, rx) = mpsc::channel::<AppEvent>();
    poller::spawn(tx, server_url, token);

    // Bridge the mpsc channel to the winit proxy in a helper thread.
    std::thread::spawn(move || {
        while let Ok(event) = rx.recv() {
            if proxy.send_event(event).is_err() {
                break;
            }
        }
    });

    let tray_menu = menu::TrayMenu::new().context("building tray menu")?;

    let active_icon = load_icon(include_bytes!("../assets/icon_active.ico"))
        .context("loading active icon")?;
    let stopped_icon = load_icon(include_bytes!("../assets/icon_stopped.ico"))
        .context("loading stopped icon")?;

    let tray_icon = TrayIconBuilder::new()
        .with_tooltip("Find Anything")
        .with_icon(active_icon.clone())
        .with_menu(Box::new(tray_menu.menu.clone()))
        .build()
        .context("building tray icon")?;

    let mut app = TrayApp {
        tray_icon,
        tray_menu,
        active_icon,
        stopped_icon,
        config_path,
        service_running: false,
        should_quit: false,
    };

    event_loop
        .run_app(&mut app)
        .context("running event loop")?;

    Ok(())
}

#[cfg(windows)]
struct TrayApp {
    tray_icon: TrayIcon,
    tray_menu: menu::TrayMenu,
    active_icon: tray_icon::Icon,
    stopped_icon: tray_icon::Icon,
    config_path: PathBuf,
    service_running: bool,
    should_quit: bool,
}

#[cfg(windows)]
impl ApplicationHandler<AppEvent> for TrayApp {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // No windows to create; the tray icon is already set up.
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        _event: winit::event::WindowEvent,
    ) {
        // No windows owned by this app.
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::StatusUpdate {
                service_running,
                file_count,
                source_count,
            } => {
                self.service_running = service_running;
                self.tray_menu
                    .update_status(service_running, file_count, source_count);

                // Swap tray icon based on service state.
                let icon = if service_running {
                    self.active_icon.clone()
                } else {
                    self.stopped_icon.clone()
                };
                let _ = self.tray_icon.set_icon(Some(icon));

                // Update tooltip.
                let tooltip = if service_running {
                    "Find Anything \u{2014} Watcher Running"
                } else {
                    "Find Anything \u{2014} Watcher Stopped"
                };
                let _ = self.tray_icon.set_tooltip(Some(tooltip));
            }
        }

        if self.should_quit {
            event_loop.exit();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Poll tray icon events (clicks).
        while let Ok(_tray_event) = TrayIconEvent::receiver().try_recv() {
            // Left-click: could show menu, but most platforms auto-show on right-click.
        }

        // Poll menu events.
        while let Ok(menu_event) = MenuEvent::receiver().try_recv() {
            self.handle_menu_event(&menu_event, event_loop);
        }

        if self.should_quit {
            event_loop.exit();
            return;
        }

        // Wake up every 100 ms so menu events feel responsive.
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            std::time::Instant::now() + std::time::Duration::from_millis(100),
        ));
    }
}

#[cfg(windows)]
impl TrayApp {
    fn handle_menu_event(
        &mut self,
        event: &MenuEvent,
        event_loop: &ActiveEventLoop,
    ) {
        if event.id == self.tray_menu.quit_id() {
            self.should_quit = true;
            event_loop.exit();
        } else if event.id == self.tray_menu.scan_id() {
            self.run_scan();
        } else if event.id == self.tray_menu.toggle_id() {
            self.toggle_service();
        } else if event.id == self.tray_menu.config_id() {
            self.open_config();
        }
    }

    fn run_scan(&self) {
        let scan_exe = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("find-scan.exe")))
            .unwrap_or_else(|| PathBuf::from("find-scan.exe"));

        let _ = std::process::Command::new(&scan_exe)
            .arg("--config")
            .arg(&self.config_path)
            .spawn();
    }

    fn toggle_service(&self) {
        if self.service_running {
            if let Err(e) = service_ctl::stop_service() {
                tracing::warn!("failed to stop service: {e}");
            }
        } else {
            if let Err(e) = service_ctl::start_service() {
                tracing::warn!("failed to start service: {e}");
            }
        }
    }

    fn open_config(&self) {
        // ShellExecute "open" on the config file opens it in the default editor.
        use std::os::windows::ffi::OsStrExt;
        let path_wide: Vec<u16> = self
            .config_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let verb: Vec<u16> = "open\0".encode_utf16().collect();

        unsafe {
            windows_sys::Win32::UI::Shell::ShellExecuteW(
                0,
                verb.as_ptr(),
                path_wide.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
            );
        }
    }
}

#[cfg(windows)]
fn load_icon(bytes: &[u8]) -> Result<tray_icon::Icon> {
    // Decode the ICO file and use the first (largest) image as RGBA.
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Ico)
        .context("decoding ICO file")?;
    let img = img.into_rgba8();
    let (w, h) = img.dimensions();
    tray_icon::Icon::from_rgba(img.into_raw(), w, h).context("creating tray icon from RGBA")
}

#[cfg(windows)]
fn parse_config_arg() -> PathBuf {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--config" {
            if let Some(path) = args.next() {
                return PathBuf::from(path);
            }
        }
    }
    // Default config path for Windows.
    dirs_next()
        .map(|p| p.join("find-anything").join("client.toml"))
        .unwrap_or_else(|| PathBuf::from("client.toml"))
}

#[cfg(windows)]
fn dirs_next() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
}
