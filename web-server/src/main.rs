use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use cc_switch_lib::{mode, AppState, Database};
use cc_switch_web::{router, WebState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let token = std::env::var("CC_SWITCH_WEB_TOKEN")
        .map_err(|_| "Set CC_SWITCH_WEB_TOKEN to a random token of at least 32 characters")?;
    let bind: SocketAddr = std::env::var("CC_SWITCH_WEB_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8080".into())
        .parse()?;
    let web_root =
        PathBuf::from(std::env::var("CC_SWITCH_WEB_ROOT").unwrap_or_else(|_| "web/dist".into()));
    if !web_root.join("index.html").is_file() {
        return Err("Web assets missing. Run pnpm build:web, or set CC_SWITCH_WEB_ROOT".into());
    }
    // Validate credentials and reserve the port before opening or migrating data.
    // No Tauri Builder, display connection, window, tray, or desktop plugin is started.
    WebState::validate_token(&token)?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    let (app, first_start) = tokio::task::spawn_blocking(|| {
        let first_start = cc_switch_lib::web_storage::initialize()?;
        let db = Arc::new(Database::init()?);
        cc_switch_lib::web_support::initialize_outbound_proxy(&db)?;
        mode::operation::recover_on_startup(&db);
        Ok::<_, cc_switch_lib::AppError>((AppState::new(db), first_start))
    })
    .await??;
    let web_modes = mode::state::load(&cc_switch_lib::live::engine::DeviceStore::for_device())?;
    let restore_proxy = web_modes.apps.values().any(|app| {
        let mode = app.mode_state();
        mode.is_proxy() || mode.attached
    });
    // A direct-only Web instance must not adopt a desktop process's placeholders.
    if !first_start && restore_proxy {
        mode::controller::startup(&app).await;
    }
    let failures = mode::controller::startup_attach_failures_snapshot();
    if !failures.is_empty() {
        eprintln!(
            "Warning: {} application(s) could not restore proxy mode and fell back to direct mode.",
            failures.len()
        );
    }
    let state = WebState::new(app.clone(), &token)?;
    cc_switch_lib::web_support::start_background_tasks(&app);
    drop(token);
    println!("CC Switch Web listening on http://{bind}");
    let shutdown_state = state.clone();
    let result = axum::serve(listener, router(state, web_root))
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            shutdown_state.shutdown();
        })
        .await;
    // Use the same detach path as desktop exit, so clients retain a working direct config.
    mode::controller::detach_all(&app).await;
    if app.proxy_service.is_running().await {
        app.proxy_service.stop().await?;
    }
    result?;
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate.recv() => {},
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}
