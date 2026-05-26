pub mod commands;
pub mod database;
pub mod llm;
pub mod loader;
pub mod models;
pub mod parser;
pub mod report;
pub mod validator;

#[cfg(debug_assertions)]
use tauri::Manager;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::scan_directory,
            commands::run_pre_checks,
            commands::test_connection,
            commands::start_loading,
            commands::stop_loading,
            commands::parse_db_info,
            commands::test_llm_connection,
            commands::get_report,
            commands::save_report,
        ])
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }
            let _ = app;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
