use tauri::Manager;

pub mod commands;
pub mod error;
pub mod ids;
pub mod logging;
pub mod panic;
pub mod persistence;
pub mod platform;
pub mod state;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir().expect("resolve app data dir");
            crate::platform::paths::ensure_data_dirs(&data_dir)?;
            let guard = crate::logging::init(&data_dir)?;
            // Keep the WorkerGuard alive for the process lifetime via Tauri state.
            app.manage(std::sync::Arc::new(std::sync::Mutex::new(Some(guard))));
            crate::panic::install_hook(data_dir.clone());
            app.manage(std::sync::Arc::new(std::sync::Mutex::new(
                crate::state::AppState::default(),
            )));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            crate::commands::system::get_app_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
