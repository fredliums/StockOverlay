use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    App, AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder,
};

const VISIBILITY: &str = "visibility";
const LOCK: &str = "lock";
const SETTINGS: &str = "settings";
const EXIT: &str = "exit";

pub struct TrayState {
    lock: MenuItem<tauri::Wry>,
}

pub fn install(app: &mut App) -> tauri::Result<()> {
    let visibility = MenuItem::with_id(app, VISIBILITY, "显示 / 隐藏", true, None::<&str>)?;
    // T17 enables this after the recovery shortcut and click-through control are ready.
    let lock = MenuItem::with_id(app, LOCK, "锁定 / 解锁", false, None::<&str>)?;
    let settings = MenuItem::with_id(app, SETTINGS, "设置", true, None::<&str>)?;
    let exit = MenuItem::with_id(app, EXIT, "退出", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&visibility, &lock, &settings, &separator, &exit])?;
    let mut tray = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("StockOverlay")
        .on_menu_event(|app, event| {
            let result: Result<(), String> = match event.id.as_ref() {
                VISIBILITY => toggle_main_visibility(app).map_err(|error| error.to_string()),
                LOCK => crate::window_control::toggle_lock(app).map(|_| ()),
                SETTINGS => open_settings(app).map_err(|error| error.to_string()),
                EXIT => {
                    if let Err(error) = crate::window_placement::save_current(app) {
                        eprintln!("could not save final window placement: {error}");
                    }
                    crate::shortcuts::unregister_all(app);
                    app.exit(0);
                    Ok(())
                }
                _ => Ok(()),
            };
            if let Err(error) = result {
                eprintln!("tray action failed: {error}");
                let _ = app.emit("window:error", error);
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    app.manage(TrayState { lock });
    Ok(())
}

pub(crate) fn set_lock_menu(app: &AppHandle, available: bool, locked: bool) -> tauri::Result<()> {
    if let Some(tray) = app.try_state::<TrayState>() {
        tray.lock.set_enabled(available)?;
        tray.lock.set_text(if locked { "解锁" } else { "锁定" })?;
    }
    Ok(())
}

pub(crate) fn set_lock_error(app: &AppHandle, error: &str) -> tauri::Result<()> {
    if let Some(tray) = app.try_state::<TrayState>() {
        tray.lock.set_enabled(false)?;
        tray.lock.set_text(format!("锁定不可用：{error}"))?;
    }
    Ok(())
}

pub(crate) fn toggle_main_visibility(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible()? {
            window.hide()
        } else {
            crate::window_placement::ensure_visible(app)
                .map_err(|error| tauri::Error::Anyhow(std::io::Error::other(error).into()))?;
            window.show()?;
            window.set_focus()
        }
    } else {
        Ok(())
    }
}

fn open_settings(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("settings") {
        window.show()?;
        return window.set_focus();
    }
    WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html".into()))
        .title("StockOverlay 设置")
        .inner_size(720.0, 560.0)
        .min_inner_size(480.0, 360.0)
        .center()
        .build()?;
    Ok(())
}
