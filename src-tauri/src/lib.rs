pub mod api;
pub mod commands;
pub mod config;
pub mod db;
pub mod services;

use config::Config;
use db::init_db;
use sqlx::SqlitePool;
use tauri::Manager;

// Type wrappers for Tauri state
pub struct LogDb(pub SqlitePool);
pub struct StartTime(pub i64);

impl std::ops::Deref for LogDb {
    type Target = SqlitePool;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = Config::load();
    let start_time = chrono::Utc::now().timestamp();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(move |app| {
            let config = config.clone();

            // Initialize database
            let db_path = config.database.path.clone();
            let log_db_path = config.database.log_path.clone();

            tauri::async_runtime::block_on(async {
                // Ensure data directory exists
                if let Some(parent) = db_path.parent() {
                    std::fs::create_dir_all(parent).ok();
                }

                let db = init_db(&db_path).await.expect("Failed to init database");
                let log_db = init_db(&log_db_path)
                    .await
                    .expect("Failed to init log database");

                app.manage(db.clone());
                app.manage(LogDb(log_db.clone()));
                app.manage(StartTime(start_time));

                // Start HTTP server for proxy
                let state = api::AppState {
                    db: db.clone(),
                    log_db: log_db.clone(),
                };

                let router = api::create_router(state);
                let addr = format!("{}:{}", config.server.host, config.server.port);

            let log_db_clone = log_db.clone();
            tokio::spawn(async move {
                // Bind listener with better error handling
                let listener = match tokio::net::TcpListener::bind(&addr).await {
                    Ok(listener) => {
                        tracing::info!("Gateway HTTP server listening on {}", addr);
                        listener
                    }
                    Err(e) => {
                        tracing::error!("Failed to bind to {}: {}", addr, e);
                        panic!("Cannot bind to address {}: {}", addr, e);
                    }
                };

                // Log gateway startup
                let _ = crate::services::stats::record_system_log(
                    &log_db_clone,
                    "info",
                    "gateway_started",
                    &format!("CCG Gateway started on {}", addr),
                    None,
                    None,
                ).await;

                if let Err(e) = axum::serve(listener, router).await {
                    tracing::error!("Gateway server error: {}", e);
                }
            });
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_providers,
            commands::get_provider,
            commands::create_provider,
            commands::update_provider,
            commands::delete_provider,
            commands::reorder_providers,
            commands::reset_provider_failures,
            commands::get_gateway_settings,
            commands::update_gateway_settings,
            commands::get_timeout_settings,
            commands::update_timeout_settings,
            commands::get_cli_settings,
            commands::update_cli_settings,
            commands::get_request_logs,
            commands::get_request_log_detail,
            commands::clear_request_logs,
            commands::get_system_logs,
            commands::clear_system_logs,
            commands::get_system_status,
            commands::get_mcps,
            commands::get_mcp,
            commands::create_mcp,
            commands::update_mcp,
            commands::delete_mcp,
            commands::get_prompts,
            commands::get_prompt,
            commands::create_prompt,
            commands::update_prompt,
            commands::delete_prompt,
            commands::get_daily_stats,
            commands::get_provider_stats,
            commands::get_session_projects,
            commands::get_project_sessions,
            commands::get_session_messages,
            commands::delete_session,
            commands::delete_project,
            commands::get_webdav_settings,
            commands::update_webdav_settings,
            commands::test_webdav_connection,
            commands::export_to_local,
            commands::import_from_local,
            commands::export_to_webdav,
            commands::list_webdav_backups,
            commands::import_from_webdav,
            commands::delete_webdav_backup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
