use crate::{commands::AppState, config::shortcut_key, tray, window_control};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::{App, AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutStatus {
    pub lock_registered: bool,
    pub visibility_registered: bool,
    pub lock_error: Option<String>,
    pub visibility_error: Option<String>,
}

#[derive(Default)]
pub struct ShortcutRuntime {
    inner: Mutex<ShortcutStateData>,
}

#[derive(Default)]
struct ShortcutStateData {
    lock: Option<String>,
    visibility: Option<String>,
    stale: Vec<String>,
    status: ShortcutStatus,
}

#[derive(Clone, Copy)]
enum Action {
    Lock,
    Visibility,
}

impl Action {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "toggleLock" => Ok(Self::Lock),
            "toggleVisibility" => Ok(Self::Visibility),
            _ => Err("Unknown shortcut action.".into()),
        }
    }
}

fn register(app: &AppHandle, action: Action, key: &str) -> Result<(), String> {
    app.global_shortcut()
        .on_shortcut(key, move |app, _, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            let result = match action {
                Action::Lock => window_control::toggle_lock(app).map(|_| ()),
                Action::Visibility => {
                    tray::toggle_main_visibility(app).map_err(|error| error.to_string())
                }
            };
            if let Err(error) = result {
                eprintln!("shortcut action failed: {error}");
                let _ = app.emit("window:error", error);
            }
        })
        .map_err(|error| error.to_string())
}

pub fn install(app: &mut App) -> Result<(), String> {
    app.manage(ShortcutRuntime::default());
    let config = app
        .state::<Arc<AppState>>()
        .config
        .get()
        .map_err(|error| error.to_string())?;
    let runtime = app.state::<ShortcutRuntime>();
    let mut state = runtime
        .inner
        .lock()
        .map_err(|_| "Shortcut state is unavailable.".to_string())?;
    match register(app.handle(), Action::Lock, &config.shortcuts.toggle_lock) {
        Ok(()) => {
            state.lock = Some(config.shortcuts.toggle_lock);
            state.status.lock_registered = true;
        }
        Err(error) => state.status.lock_error = Some(error),
    }
    match register(
        app.handle(),
        Action::Visibility,
        &config.shortcuts.toggle_visibility,
    ) {
        Ok(()) => {
            state.visibility = Some(config.shortcuts.toggle_visibility);
            state.status.visibility_registered = true;
        }
        Err(error) => state.status.visibility_error = Some(error),
    }
    let lock_ready = state.status.lock_registered;
    let lock_error = state.status.lock_error.clone();
    drop(state);
    window_control::set_recovery_ready(app.handle(), lock_ready)?;
    if let Some(error) = lock_error {
        tray::set_lock_error(app.handle(), &error).map_err(|error| error.to_string())?;
    }
    if config.window.locked && lock_ready {
        if let Err(error) = window_control::toggle_lock(app.handle()) {
            eprintln!("could not restore lock state: {error}");
        }
    }
    Ok(())
}

pub fn status(app: &AppHandle) -> Result<ShortcutStatus, String> {
    Ok(app
        .state::<ShortcutRuntime>()
        .inner
        .lock()
        .map_err(|_| "Shortcut state is unavailable.".to_string())?
        .status
        .clone())
}

pub fn change(app: &AppHandle, action_name: &str, new_key: &str) -> Result<ShortcutStatus, String> {
    let action = Action::parse(action_name)?;
    let config_store = &app.state::<Arc<AppState>>().config;
    let mut candidate = config_store.get().map_err(|error| error.to_string())?;
    match action {
        Action::Lock => candidate.shortcuts.toggle_lock = new_key.into(),
        Action::Visibility => candidate.shortcuts.toggle_visibility = new_key.into(),
    }
    candidate.validate().map_err(|error| error.to_string())?;

    let runtime = app.state::<ShortcutRuntime>();
    let mut state = runtime
        .inner
        .lock()
        .map_err(|_| "Shortcut state is unavailable.".to_string())?;
    let old = match action {
        Action::Lock => state.lock.clone(),
        Action::Visibility => state.visibility.clone(),
    };
    let old_matches = old.as_deref().and_then(|value| shortcut_key(value).ok())
        == Some(shortcut_key(new_key).map_err(|error| error.to_string())?);
    if old_matches {
        return Ok(state.status.clone());
    }
    state.stale.retain(|stale| {
        if let Err(error) = app.global_shortcut().unregister(stale.as_str()) {
            eprintln!("could not clean up old shortcut {stale}: {error}");
            true
        } else {
            false
        }
    });
    register(app, action, new_key)?;
    if let Err(error) = config_store.update(|config| match action {
        Action::Lock => config.shortcuts.toggle_lock = new_key.into(),
        Action::Visibility => config.shortcuts.toggle_visibility = new_key.into(),
    }) {
        let _ = app.global_shortcut().unregister(new_key);
        return Err(format!("Could not save shortcut: {error}"));
    }
    match action {
        Action::Lock => {
            state.lock = Some(new_key.into());
            state.status.lock_registered = true;
            state.status.lock_error = None;
        }
        Action::Visibility => {
            state.visibility = Some(new_key.into());
            state.status.visibility_registered = true;
            state.status.visibility_error = None;
        }
    }
    if let Some(old) = old {
        if let Err(error) = app.global_shortcut().unregister(old.as_str()) {
            state.stale.push(old);
            return Err(format!(
                "New shortcut is active, but the old shortcut remains active: {error}"
            ));
        }
    }
    if matches!(action, Action::Lock) {
        window_control::set_recovery_ready(app, true)?;
    }
    Ok(state.status.clone())
}

pub fn unregister_all(app: &AppHandle) {
    if let Err(error) = app.global_shortcut().unregister_all() {
        eprintln!("could not unregister shortcuts: {error}");
    }
}
