#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod autolaunch;
mod config;
mod http;
mod keystrokes;
mod net;
mod power;
mod profiles;
mod single_instance;
mod state;
mod tray;
mod types;
mod ws;

use std::net::Ipv4Addr;
use std::sync::{mpsc, Arc, RwLock};
use std::time::Duration;

use single_instance::ClaimResult;
use state::{AppState, StateEvent};
use tray::TrayCmd;

const PORT: u16 = 7337;

#[cfg(all(target_os = "windows", not(debug_assertions)))]
const AUTO_OPEN_PAIRING_QR_ON_STARTUP: bool = true;
#[cfg(not(all(target_os = "windows", not(debug_assertions))))]
const AUTO_OPEN_PAIRING_QR_ON_STARTUP: bool = false;

#[cfg(not(all(target_os = "windows", not(debug_assertions))))]
const PRINT_PAIRING_QR: bool = true;
#[cfg(all(target_os = "windows", not(debug_assertions)))]
const PRINT_PAIRING_QR: bool = false;

enum StartupSignal {
    Ready,
    Failed,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sofamote=info".into()),
        )
        .init();

    let listener = match single_instance::claim_primary_listener(PORT) {
        ClaimResult::Primary(listener) => listener,
        ClaimResult::Exit(0) => return,
        ClaimResult::Exit(code) => std::process::exit(code),
    };

    let cfg = config::load_or_create();
    let token = cfg.token.clone();
    let lan_ip = net::get_lan_ip();
    let pairing_url = Arc::new(RwLock::new(format!(
        "http://{lan_ip}:{PORT}/?t={token}"
    )));

    if PRINT_PAIRING_QR {
        let url = pairing_url.read().expect("pairing_url lock poisoned").clone();
        print_qr(&url);
    }

    let app_state = AppState::new(cfg.clone());
    let should_open_pairing_qr = AUTO_OPEN_PAIRING_QR_ON_STARTUP && !cfg.has_shown_pairing_qr;

    // Unbounded channel: tray main-thread → async tokio task.
    // UnboundedSender::send() is sync-safe (non-blocking).
    let (tray_tx, tray_rx) = tokio::sync::mpsc::unbounded_channel::<TrayCmd>();

    // Shutdown signal
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let (startup_tx, startup_rx) = mpsc::channel::<StartupSignal>();

    // Resume notification: Win32 callback (system-managed thread) → tokio task.
    let (resume_tx, resume_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    // Registration handle must outlive the runtime so the OS can keep calling
    // back into us; held on the main thread, dropped at end of main().
    let _resume_registration = power::register_resume_notifier(resume_tx);

    // Start tokio runtime on a background thread
    let state_bg = Arc::clone(&app_state);
    let pairing_url_bg = Arc::clone(&pairing_url);
    let token_bg = token.clone();
    let server_thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async move {
            run_server(
                state_bg,
                tray_rx,
                pairing_url_bg,
                token_bg,
                listener,
                shutdown_rx,
                startup_tx,
                resume_rx,
            )
            .await;
        });
    });

    // Sync OS auto-launch state with config
    autolaunch::set_auto_launch(cfg.auto_launch).ok();

    // Build system tray on the main thread (required by OS message pump)
    let (tray_handle, menu_ids) =
        tray::build_tray(Arc::clone(&pairing_url), cfg.is_active, cfg.auto_launch)
            .expect("failed to create system tray");

    // Track local state for sync main-thread use
    let mut active = cfg.is_active;
    let mut auto_launch_state = cfg.auto_launch;
    let mut state_rx = app_state.subscribe();
    let mut pairing_qr_pending = should_open_pairing_qr;

    // Closure that always reads the latest pairing URL when opening the QR.
    let pairing_url_for_qr = Arc::clone(&pairing_url);
    let current_qr_url = move || -> String {
        let url = pairing_url_for_qr
            .read()
            .expect("pairing_url lock poisoned")
            .clone();
        let base = url.split_once('?').map_or(url.as_str(), |(b, _)| b);
        format!("{base}qr.png")
    };

    // Main thread event loop.
    //
    // On Windows, tray-icon creates a hidden HWND on the calling thread.
    // That window must receive Win32 messages via PeekMessage/DispatchMessage
    // for right-click and menu events to fire — a plain sleep loop is not enough.
    run_event_loop(|| {
        if pairing_qr_pending {
            match startup_rx.try_recv() {
                Ok(StartupSignal::Ready) => {
                    pairing_qr_pending = false;
                    if open::that(current_qr_url()).is_ok() {
                        app_state.mark_pairing_qr_shown();
                    }
                }
                Ok(StartupSignal::Failed) | Err(mpsc::TryRecvError::Disconnected) => {
                    pairing_qr_pending = false;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }

        // Apply state broadcasts from async tasks
        while let Ok(event) = state_rx.try_recv() {
            match event {
                StateEvent::ActiveChanged(v) => {
                    active = v;
                    tray_handle.set_active(active);
                }
                StateEvent::PairingUrlRefreshed => {
                    tray_handle.refresh_pairing_url();
                }
            }
        }

        // Process tray menu clicks
        if let Some(event) = tray::poll_menu_event() {
            if event.id == menu_ids.active {
                active = !active;
                tray_tx.send(TrayCmd::SetActive(active)).ok();
            } else if event.id == menu_ids.autolaunch {
                auto_launch_state = !auto_launch_state;
                tray_handle.set_auto_launch(auto_launch_state);
                tray_tx.send(TrayCmd::SetAutoLaunch(auto_launch_state)).ok();
            } else if event.id == menu_ids.show_qr {
                open::that(current_qr_url()).ok();
            } else if event.id == menu_ids.quit {
                return false; // exit loop
            }
        }

        true // continue
    });

    // Graceful shutdown
    drop(tray_handle);
    shutdown_tx.send(()).ok();
    // Give the server up to 1.5s to shut down, then exit regardless
    let _ = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        std::process::exit(0);
    });
    server_thread.join().ok();
    // _resume_registration drops here, which unregisters the Win32 callback
    // and releases the boxed sender (causing the resume task to exit cleanly).
}

#[expect(clippy::too_many_arguments)]
async fn run_server(
    state: Arc<AppState>,
    mut tray_rx: tokio::sync::mpsc::UnboundedReceiver<TrayCmd>,
    pairing_url: Arc<RwLock<String>>,
    token: String,
    initial_listener: std::net::TcpListener,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    startup_tx: mpsc::Sender<StartupSignal>,
    mut resume_rx: tokio::sync::mpsc::UnboundedReceiver<()>,
) {
    // Task: apply tray commands to app state
    let state_for_cmds = Arc::clone(&state);
    tokio::spawn(async move {
        while let Some(cmd) = tray_rx.recv().await {
            match cmd {
                TrayCmd::SetActive(v) => state_for_cmds.set_active(v).await,
                TrayCmd::SetAutoLaunch(v) => {
                    state_for_cmds.set_auto_launch(v).await;
                    tokio::task::spawn_blocking(move || autolaunch::set_auto_launch(v))
                        .await
                        .ok();
                }
            }
        }
    });

    // Task: refresh pairing URL on system resume from sleep/hibernation.
    // The previously-shown LAN IP is preferred when it's still bound to any
    // interface, so the phone's cached pairing URL keeps working without a re-scan.
    let pairing_url_for_resume = Arc::clone(&pairing_url);
    let state_for_resume = Arc::clone(&state);
    let token_for_resume = token.clone();
    tokio::spawn(async move {
        while resume_rx.recv().await.is_some() {
            // Coalesce notification bursts (PBT_APMRESUMESUSPEND + PBT_APMRESUMEAUTOMATIC).
            while resume_rx.try_recv().is_ok() {}

            tracing::info!("system resume detected; refreshing pairing URL");
            // Give the OS a moment to bring the network back up.
            tokio::time::sleep(Duration::from_millis(750)).await;

            if refresh_pairing_url(&pairing_url_for_resume, &token_for_resume) {
                let url = pairing_url_for_resume
                    .read()
                    .expect("pairing_url lock poisoned")
                    .clone();
                tracing::info!("pairing URL changed -> {}", url);
            }
            state_for_resume.tx.send(StateEvent::PairingUrlRefreshed).ok();
        }
    });

    let router = http::build_router(state, Arc::clone(&pairing_url));

    let mut listener = match tokio::net::TcpListener::from_std(initial_listener) {
        Ok(l) => l,
        Err(e) => {
            startup_tx.send(StartupSignal::Failed).ok();
            tracing::error!("cannot adopt listener on port {PORT}: {e}");
            return;
        }
    };

    startup_tx.send(StartupSignal::Ready).ok();
    tracing::info!("Listening on port {PORT}");

    // Supervisor loop: if axum::serve ever returns (shouldn't normally happen,
    // but defends against catastrophic listener failure after sleep/resume),
    // log and rebind on 0.0.0.0:PORT with exponential backoff.
    let mut backoff_secs: u64 = 1;
    loop {
        let serve_fut = axum::serve(listener, router.clone()).into_future();
        tokio::pin!(serve_fut);

        tokio::select! {
            biased;
            _ = &mut shutdown_rx => {
                tracing::info!("shutting down");
                return;
            }
            res = &mut serve_fut => {
                match res {
                    Ok(()) => tracing::warn!("axum::serve completed; rebinding in {backoff_secs}s"),
                    Err(e) => tracing::error!("axum::serve exited: {e}; rebinding in {backoff_secs}s"),
                }
            }
        }

        // Backoff before rebind, but honor shutdown
        tokio::select! {
            biased;
            _ = &mut shutdown_rx => return,
            _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
        }

        // Try to rebind the listener (cap backoff at 30s per attempt)
        listener = loop {
            match tokio::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, PORT)).await {
                Ok(l) => break l,
                Err(e) => {
                    tracing::error!(
                        "rebind on port {PORT} failed: {e}; retrying in {backoff_secs}s"
                    );
                    tokio::select! {
                        biased;
                        _ = &mut shutdown_rx => return,
                        _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
                    }
                    backoff_secs = (backoff_secs * 2).min(30);
                }
            }
        };
        backoff_secs = 1;
        tracing::info!("rebound listener on port {PORT}");
    }
}

/// Re-detects the LAN IP and rewrites the shared pairing URL if it changed.
/// Sticky: keeps the previously-shown IP whenever it is still bound to any
/// live interface, so the phone's cached URL keeps working across network changes.
fn refresh_pairing_url(pairing_url: &Arc<RwLock<String>>, token: &str) -> bool {
    let current = pairing_url
        .read()
        .expect("pairing_url lock poisoned")
        .clone();
    let previous_ip = extract_ip_from_pairing_url(&current);
    let new_ip = net::pick_lan_ip(previous_ip);
    let new_url = format!("http://{new_ip}:{PORT}/?t={token}");
    if new_url == current {
        return false;
    }
    *pairing_url.write().expect("pairing_url lock poisoned") = new_url;
    true
}

fn extract_ip_from_pairing_url(url: &str) -> Option<std::net::IpAddr> {
    let after_scheme = url.strip_prefix("http://")?;
    let host_port = after_scheme.split('/').next()?;
    let ip_str = host_port.split(':').next()?;
    ip_str.parse().ok()
}

// On Windows: run a proper Win32 message pump so tray-icon's hidden HWND receives
// WM_RBUTTONUP and related messages that trigger the context menu.
// On other platforms: a plain sleep loop is sufficient.
#[cfg(target_os = "windows")]
fn run_event_loop<F: FnMut() -> bool>(mut tick: F) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_QUIT,
    };
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        loop {
            // Drain all pending Win32 messages so tray-icon's window can process them
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_QUIT {
                    return;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if !tick() {
                return;
            }
            std::thread::sleep(Duration::from_millis(16));
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn run_event_loop<F: FnMut() -> bool>(mut tick: F) {
    loop {
        if !tick() {
            return;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
}

fn print_qr(url: &str) {
    use qrcode::render::unicode;
    use qrcode::QrCode;

    if let Ok(code) = QrCode::new(url.as_bytes()) {
        let rendered = code
            .render::<unicode::Dense1x2>()
            .dark_color(unicode::Dense1x2::Light)
            .light_color(unicode::Dense1x2::Dark)
            .build();
        println!("\n{rendered}");
    }
    let base = url.split_once('?').map_or(url, |(b, _)| b);
    println!("Pairing URL : {url}");
    println!("QR image    : {base}qr.png\n");
}
