mod commands;
pub mod config;
pub mod provider;
pub mod quote;
mod service;
pub mod tencent;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let state = Arc::new(commands::AppState::new(config::ConfigStore::open_default()?));
            let provider = provider::TencentQuoteProvider::new()?;
            let service = Arc::new(service::QuoteService::new(provider, Arc::clone(&state)));
            app.manage(state);
            Arc::clone(&service).start(app.handle().clone());
            app.manage(service);
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
