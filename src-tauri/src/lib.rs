mod commands;
pub mod config;
pub mod provider;
pub mod quote;
mod service;
pub mod stock_index;
pub mod tencent;
mod tray;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            tray::install(app)?;
            let state = Arc::new(commands::AppState::new(
                config::ConfigStore::open_default()?
            )?);
            let provider = provider::TencentQuoteProvider::new()?;
            let service = Arc::new(service::QuoteService::new(provider, Arc::clone(&state)));
            app.manage(state);
            Arc::clone(&service).start(app.handle().clone());
            app.manage(service);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if let Err(error) = window.hide() {
                        eprintln!("could not hide the main window: {error}");
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::load_config,
            commands::save_config,
            commands::get_quote_snapshot,
            commands::search_stocks,
            commands::add_stock,
            commands::remove_stock,
            commands::reorder_stocks
        ])
        .run(tauri::generate_context!())
        .expect("failed to run StockOverlay");
}
