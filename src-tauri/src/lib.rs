mod commands;
pub mod config;
pub mod quote;
pub mod tencent;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(commands::AppState::new(config::ConfigStore::open_default()?));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::load_config,
            commands::save_config,
            commands::get_quote_snapshot
        ])
        .run(tauri::generate_context!())
        .expect("failed to run StockOverlay");
}
