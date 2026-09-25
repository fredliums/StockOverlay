use crate::commands::AppState;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use tauri::{App, AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize};

const SAVE_DELAY: Duration = Duration::from_millis(650);
const MONITOR_CHECK: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct PlacementState {
    generation: AtomicU64,
    content_minimum: Mutex<(u32, u32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl Rect {
    fn intersection_area(self, other: Self) -> u64 {
        let right = (self.x as i64 + self.width as i64).min(other.x as i64 + other.width as i64);
        let bottom = (self.y as i64 + self.height as i64).min(other.y as i64 + other.height as i64);
        let width = (right - (self.x as i64).max(other.x as i64)).max(0) as u64;
        let height = (bottom - (self.y as i64).max(other.y as i64)).max(0) as u64;
        width * height
    }

    fn fit(self, work: Self) -> Self {
        let width = self.width.min(work.width);
        let height = self.height.min(work.height);
        let max_x = work
            .x
            .saturating_add(work.width.saturating_sub(width) as i32);
        let max_y = work
            .y
            .saturating_add(work.height.saturating_sub(height) as i32);
        Self {
            x: self.x.clamp(work.x, max_x),
            y: self.y.clamp(work.y, max_y),
            width,
            height,
        }
    }
}

fn work_rect(monitor: &tauri::Monitor) -> Rect {
    let area = monitor.work_area();
    Rect {
        x: area.position.x,
        y: area.position.y,
        width: area.size.width,
        height: area.size.height,
    }
}

fn best_work_area(bounds: Rect, monitors: &[tauri::Monitor], primary: &tauri::Monitor) -> Rect {
    monitors
        .iter()
        .map(work_rect)
        .filter_map(|work| {
            let area = bounds.intersection_area(work);
            (area > 0).then_some((area, work))
        })
        .max_by_key(|(area, _)| *area)
        .map(|(_, work)| work)
        .unwrap_or_else(|| work_rect(primary))
}

fn primary_scale(window: &tauri::WebviewWindow) -> tauri::Result<f64> {
    Ok(window
        .primary_monitor()?
        .map(|monitor| monitor.scale_factor())
        .unwrap_or(window.scale_factor()?))
}

pub fn install(app: &mut App) -> Result<(), String> {
    app.manage(PlacementState::default());
    let handle = app.handle().clone();
    restore_from_config(&handle)?;
    tauri::async_runtime::spawn(async move {
        let mut timer = tokio::time::interval(MONITOR_CHECK);
        let mut previous = Vec::new();
        loop {
            timer.tick().await;
            let Some(window) = handle.get_webview_window("main") else {
                break;
            };
            let Ok(monitors) = window.available_monitors() else {
                continue;
            };
            let fingerprint: Vec<_> = monitors
                .iter()
                .map(|monitor| (work_rect(monitor), monitor.scale_factor().to_bits()))
                .collect();
            if !previous.is_empty() && fingerprint != previous {
                if let Err(error) = ensure_visible(&handle) {
                    eprintln!("could not recover window after monitor change: {error}");
                }
            }
            previous = fingerprint;
        }
    });
    Ok(())
}

fn restore_from_config(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window is unavailable.".to_string())?;
    let config = app
        .state::<Arc<AppState>>()
        .config
        .get()
        .map_err(|error| error.to_string())?;
    let Some(primary) = window
        .primary_monitor()
        .map_err(|error| error.to_string())?
    else {
        return Ok(());
    };
    let monitors = window
        .available_monitors()
        .map_err(|error| error.to_string())?;
    let scale = primary.scale_factor();
    let requested = Rect {
        x: (config.window.x as f64 * scale).round() as i32,
        y: (config.window.y as f64 * scale).round() as i32,
        width: (config.window.width as f64 * scale).round() as u32,
        height: (config.window.height as f64 * scale).round() as u32,
    };
    let work = best_work_area(requested, &monitors, &primary);
    apply_bounds(&window, requested.fit(work), work)
}

pub fn ensure_visible(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window is unavailable.".to_string())?;
    let Some(primary) = window
        .primary_monitor()
        .map_err(|error| error.to_string())?
    else {
        return Ok(());
    };
    let monitors = window
        .available_monitors()
        .map_err(|error| error.to_string())?;
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let size = window.inner_size().map_err(|error| error.to_string())?;
    let current = Rect {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };
    let work = best_work_area(current, &monitors, &primary);
    let corrected = current.fit(work);
    if corrected != current {
        apply_bounds(&window, corrected, work)?;
        schedule_save(app);
    } else {
        update_min_size(&window, work)?;
    }
    Ok(())
}

fn apply_bounds(window: &tauri::WebviewWindow, bounds: Rect, work: Rect) -> Result<(), String> {
    update_min_size(window, work)?;
    window
        .set_size(PhysicalSize::new(bounds.width, bounds.height))
        .map_err(|error| error.to_string())?;
    window
        .set_position(PhysicalPosition::new(bounds.x, bounds.y))
        .map_err(|error| error.to_string())
}

fn update_min_size(window: &tauri::WebviewWindow, work: Rect) -> Result<(), String> {
    let scale = window.scale_factor().map_err(|error| error.to_string())?;
    let (content_width, content_height) = *window
        .state::<PlacementState>()
        .content_minimum
        .lock()
        .map_err(|_| "Window content minimum is unavailable.".to_string())?;
    window
        .set_min_size(Some(LogicalSize::new(
            (240.0_f64.max(content_width as f64)).min(work.width as f64 / scale),
            (80.0_f64.max(content_height as f64)).min(work.height as f64 / scale),
        )))
        .map_err(|error| error.to_string())
}

pub fn set_content_min_size(app: &AppHandle, width: u32, height: u32) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window is unavailable.".to_string())?;
    let Some(monitor) = window
        .current_monitor()
        .map_err(|error| error.to_string())?
    else {
        return Ok(());
    };
    *app.state::<PlacementState>()
        .content_minimum
        .lock()
        .map_err(|_| "Window content minimum is unavailable.".to_string())? =
        (width.min(4_096), height.min(2_160));
    update_min_size(&window, work_rect(&monitor))
}

pub fn schedule_save(app: &AppHandle) {
    let state = app.state::<PlacementState>();
    let generation = state.generation.fetch_add(1, Ordering::AcqRel) + 1;
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SAVE_DELAY).await;
        if handle
            .state::<PlacementState>()
            .generation
            .load(Ordering::Acquire)
            != generation
        {
            return;
        }
        if let Err(error) = save_current(&handle) {
            eprintln!("could not save window placement: {error}");
        }
    });
}

pub(crate) fn save_current(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window is unavailable.".to_string())?;
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let size = window.inner_size().map_err(|error| error.to_string())?;
    let position_scale = primary_scale(&window).map_err(|error| error.to_string())?;
    let size_scale = window.scale_factor().map_err(|error| error.to_string())?;
    let x = (position.x as f64 / position_scale).round() as i32;
    let y = (position.y as f64 / position_scale).round() as i32;
    let width = (size.width as f64 / size_scale).round() as u32;
    let height = (size.height as f64 / size_scale).round() as u32;
    let config = &app.state::<Arc<AppState>>().config;
    let current = config.get().map_err(|error| error.to_string())?;
    if (
        current.window.x,
        current.window.y,
        current.window.width,
        current.window.height,
    ) == (x, y, width, height)
    {
        return Ok(());
    }
    let updated = config
        .update(|config| {
            config.window.x = x;
            config.window.y = y;
            config.window.width = width;
            config.window.height = height;
        })
        .map_err(|error| error.to_string())?;
    if let Err(error) = app.emit("config:update", updated) {
        eprintln!("could not emit window placement update: {error}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Rect;

    #[test]
    fn moves_offscreen_window_into_work_area_and_shrinks_oversize_window() {
        let work = Rect {
            x: 100,
            y: 50,
            width: 1200,
            height: 700,
        };
        assert_eq!(
            Rect {
                x: 5000,
                y: 5000,
                width: 560,
                height: 180
            }
            .fit(work),
            Rect {
                x: 740,
                y: 570,
                width: 560,
                height: 180
            }
        );
        assert_eq!(
            Rect {
                x: -100,
                y: -100,
                width: 2000,
                height: 1000
            }
            .fit(work),
            work
        );
    }

    #[test]
    fn intersection_uses_actual_visible_pixels() {
        let screen = Rect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        assert_eq!(
            Rect {
                x: 1900,
                y: 100,
                width: 560,
                height: 180
            }
            .intersection_area(screen),
            3600
        );
        assert_eq!(
            Rect {
                x: -600,
                y: 100,
                width: 560,
                height: 180
            }
            .intersection_area(screen),
            0
        );
    }
}
