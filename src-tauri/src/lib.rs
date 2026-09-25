mod commands;
pub mod config;
pub mod provider;
pub mod quote;
mod service;
mod shortcuts;
pub mod stock_index;
pub mod tencent;
mod tray;
mod window_control;
mod window_placement;
use std::sync::Arc;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let state = Arc::new(commands::AppState::new(
                config::ConfigStore::open_default()?
            )?);
            let provider = provider::TencentQuoteProvider::new()?;
            let service = Arc::new(service::QuoteService::new(provider, Arc::clone(&state)));
            app.manage(state);
            if let Err(error) = tray::install(app) {
                eprintln!("could not initialize tray: {error}");
                WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html".into()))
                    .title("StockOverlay 设置（托盘不可用）")
                    .inner_size(720.0, 560.0)
                    .min_inner_size(480.0, 360.0)
                    .center()
                    .build()?;
            }
            window_control::install(app);
            if let Err(error) = shortcuts::install(app) {
                eprintln!("could not initialize shortcuts: {error}");
            }
            let placement_ready = match window_placement::install(app) {
                Ok(()) => true,
                Err(error) => {
                    eprintln!("could not restore window placement: {error}");
                    false
                }
            };
            if placement_ready {
                if let Err(error) = shortcuts::restore_lock(app.handle()) {
                    eprintln!("could not restore lock state: {error}");
                }
            }
            Arc::clone(&service).start(app.handle().clone());
            app.manage(service);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                match event {
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        if tray::available(window.app_handle()) {
                            api.prevent_close();
                            if let Err(error) = window.hide() {
                                eprintln!("could not hide the main window: {error}");
                            }
                        } else {
                            api.prevent_close();
                            tray::exit_app(window.app_handle());
                        }
                    }
                    tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
                        window_placement::schedule_save(window.app_handle());
                    }
                    tauri::WindowEvent::ScaleFactorChanged { .. } => {
                        if let Err(error) = window_placement::ensure_visible(window.app_handle()) {
                            eprintln!("could not adapt to display scale change: {error}");
                        }
                    }
                    _ => {}
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::load_config,
            commands::save_config,
            commands::get_quote_snapshot,
            commands::get_window_mode,
            commands::set_content_min_size,
            commands::get_shortcut_status,
            commands::change_shortcut,
            commands::search_stocks,
            commands::get_watchlist_entries,
            commands::add_stock,
            commands::remove_stock,
            commands::reorder_stocks
        ])
        .run(tauri::generate_context!())
        .expect("failed to run StockOverlay");
}
