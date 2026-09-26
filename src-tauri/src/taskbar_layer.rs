use std::time::Duration;

const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOACTIVATE: u32 = 0x0010;

#[link(name = "user32")]
extern "system" {
    fn SetWindowPos(
        window: *mut std::ffi::c_void,
        insert_after: *mut std::ffi::c_void,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> i32;
}

pub fn restore_after_deactivation(window: tauri::Window) {
    tauri::async_runtime::spawn(async move {
        // Explorer raises the taskbar after the window loses focus. Reassert its
        // existing topmost position once that activation has finished.
        tokio::time::sleep(Duration::from_millis(100)).await;
        if !window.is_visible().unwrap_or(false) {
            return;
        }
        let Ok(hwnd) = window.hwnd() else {
            return;
        };
        // HWND_TOPMOST is -1. SWP_NOACTIVATE keeps focus with the clicked app.
        let result = unsafe {
            SetWindowPos(
                hwnd.0,
                -1_isize as *mut std::ffi::c_void,
                0,
                0,
                0,
                0,
                SWP_NOSIZE | SWP_NOMOVE | SWP_NOACTIVATE,
            )
        };
        if result == 0 {
            eprintln!(
                "could not restore main window above taskbar: {}",
                std::io::Error::last_os_error()
            );
        }
    });
}
