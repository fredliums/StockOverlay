use crate::{commands::AppState, tray};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Default)]
pub struct WindowControl {
    inner: Mutex<ControlState>,
}

#[derive(Default)]
struct ControlState {
    locked: bool,
    recovery_ready: bool,
}

pub fn install(app: &mut tauri::App) {
    app.manage(WindowControl::default());
}

pub fn current_mode(app: &AppHandle) -> Result<bool, String> {
    Ok(app
        .state::<WindowControl>()
        .inner
        .lock()
        .map_err(|_| "Window mode is unavailable.".to_string())?
        .locked)
}

pub fn set_recovery_ready(app: &AppHandle, ready: bool) -> Result<(), String> {
    let control = app.state::<WindowControl>();
    let mut state = control
        .inner
        .lock()
        .map_err(|_| "Window mode is unavailable.".to_string())?;
    state.recovery_ready = ready;
    tray::set_lock_menu(app, ready, state.locked).map_err(|error| error.to_string())
}

pub fn toggle_lock(app: &AppHandle) -> Result<bool, String> {
    let control = app.state::<WindowControl>();
    let mut state = control
        .inner
        .lock()
        .map_err(|_| "Window mode is unavailable.".to_string())?;
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window is unavailable.".to_string())?;
    let config = app.state::<Arc<AppState>>();

    if state.locked {
        window
            .set_ignore_cursor_events(false)
            .map_err(|error| error.to_string())?;
        state.locked = false;
        if let Err(error) = window.set_resizable(true) {
            let _ = window.show();
            return Err(format!("Could not restore resize: {error}"));
        }
        let saved = config
            .config
            .update(|current| current.window.locked = false);
        tray::set_lock_menu(app, state.recovery_ready, false).map_err(|error| error.to_string())?;
        app.emit("window:mode", false)
            .map_err(|error| error.to_string())?;
        saved.map_err(|error| error.to_string())?;
        Ok(false)
    } else {
        if !state.recovery_ready {
            window.show().map_err(|error| error.to_string())?;
            return Err("Lock shortcut is unavailable; click-through was not enabled.".into());
        }
        if let Err(error) = window.set_resizable(false) {
            let _ = window.show();
            return Err(format!("Could not disable resize: {error}"));
        }
        if let Err(error) = window.set_ignore_cursor_events(true) {
            let _ = window.set_resizable(true);
            let _ = window.show();
            return Err(format!("Could not enable click-through: {error}"));
        }
        if let Err(error) = config.config.update(|current| current.window.locked = true) {
            let _ = window.set_ignore_cursor_events(false);
            let _ = window.set_resizable(true);
            let _ = window.show();
            return Err(format!("Could not save lock state: {error}"));
        }
        state.locked = true;
        if let Err(error) = tray::set_lock_menu(app, true, true)
            .map_err(|error| error.to_string())
            .and_then(|_| {
                app.emit("window:mode", true)
                    .map_err(|error| error.to_string())
            })
        {
            state.locked = false;
            let _ = window.set_ignore_cursor_events(false);
            let _ = window.set_resizable(true);
            let _ = config
                .config
                .update(|current| current.window.locked = false);
            let _ = tray::set_lock_menu(app, true, false);
            let _ = app.emit("window:mode", false);
            let _ = window.show();
            return Err(format!("Could not publish lock state: {error}"));
        }
        Ok(true)
    }
}
