// ╔══════════════════════════════════════════════════════════════════╗
// ║  src-tauri/src/main.rs                                           ║
// ║                                                                  ║
// ║  main.rs  = binary crate (the executable entry point)           ║
// ║  lib.rs   = library crate  named  epic_tauri_lib                ║
// ║                                                                  ║
// ║  They are TWO SEPARATE crates in one Cargo package.             ║
// ║  main.rs accesses lib.rs items via  epic_tauri_lib::            ║
// ║  lib.rs  accesses its own items via  crate::                    ║
// ╚══════════════════════════════════════════════════════════════════╝

// Prevents additional console window on Windows in release — DO NOT REMOVE
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Mutex, atomic::AtomicBool};

use tauri::Manager;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

// ── Import everything from the library crate (lib.rs = epic_tauri_lib) ───────
use epic_tauri_lib::AppState;
use epic_tauri_lib::db::core::DbManager;
use epic_tauri_lib::tracker::app_tracker::app_track::start_active_window_tracker;
use epic_tauri_lib::env_config::AppEnvConfig;
use epic_tauri_lib::commands;
use epic_tauri_lib::streaming;
use epic_tauri_lib::auth_commands;
use epic_tauri_lib::logger;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // ── 1. Logger ───────────────────────────────────────────────────
            if let Err(e) = logger::init_logger(&app.handle()) {
                eprintln!("Failed to initialize logger: {}", e);
            }
            log::info!("Initializing EPIC application…");

            // ── 2. Load environment config ──────────────────────────────────
            //    Reads APP_ENV → loads .env.{dev|qa|uat|prod}
            let env_cfg = AppEnvConfig::load();
            log::info!(
                "[ENV] Environment: {} | bypass_api: {}",
                env_cfg.app_env,
                env_cfg.bypass_api
            );
            app.manage(env_cfg);

            // ── 3. Database ─────────────────────────────────────────────────
            let app_handle = app.handle().clone();

            // Explicit Arc<DbManager> type so the compiler knows what
            // .clone() produces when passed to start_active_window_tracker
            let db: Arc<DbManager> =
                match DbManager::open_or_create(&app.handle()) {
                    Ok(d) => {
                        log::info!("Database initialized successfully");
                        Arc::new(d)
                    }
                    Err(e) => {
                        log::error!("Failed to initialize database: {}", e);
                        return Err(e.into());
                    }
                };

            // ── 4. Active window / app tracker ──────────────────────────────
            let running = Arc::new(AtomicBool::new(true));
            start_active_window_tracker(db.clone(), running.clone());
            log::info!("Active window tracker started");

            // ── 5. App state ────────────────────────────────────────────────
            let app_state = AppState {
                db:         db.clone(),
                tracker:    Mutex::new(None),
                pipeline:   Mutex::new(None),
                app_handle: app_handle.clone(),
            };
            app.manage(app_state);

            // ── 6. System tray ──────────────────────────────────────────────
            let show_item = MenuItemBuilder::with_id("show", "Show Window").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit",  "Quit").build(app)?;
            let separator = PredefinedMenuItem::separator(app)?;

            let menu = MenuBuilder::new(app)
                .items(&[&show_item, &separator, &quit_item])
                .build()?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().expect("no icon found"))
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            // ── 7. Hide on close → minimize to tray ────────────────────────
            if let Some(window) = app.get_webview_window("main") {
                let window_clone = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        log::info!("[APP] Window close → stopping pipeline…");
                        epic_tauri_lib::streaming::pipeline::stop_pipeline();
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        let _ = window_clone.hide();
                        api.prevent_close();
                    }
                });
            }

            log::info!("Application initialized — ready");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // ── Existing commands ────────────────────────────────────────────
            commands::checkin,
            commands::checkout,
            commands::get_status,
            commands::get_hook_stats,
            commands::record_key,
            commands::record_mouse_move,
            commands::record_mouse_click,
            commands::toggle_break,
            commands::get_startup_status,
            commands::resume_tracking,
            commands::get_total_active_seconds,
            streaming::pipeline::start_pipeline,
            streaming::pipeline::stop_pipeline,
            streaming::pipeline::pipeline_status,

            // ── New auth / onboarding commands ───────────────────────────────
            auth_commands::validate_organization,
            auth_commands::login_user,
            auth_commands::get_current_user,
            auth_commands::logout,
            auth_commands::get_app_env,
            commands::check_auth_state,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    log::info!("Application shutting down");
}