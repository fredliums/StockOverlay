use crate::{
    config::{AppConfig, ConfigStore},
    quote::{QuoteSnapshot, QuoteState, QuoteStatus, Symbol},
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::{State, WebviewWindow};

pub struct AppState {
    pub(crate) config: ConfigStore,
    pub(crate) snapshot: Mutex<QuoteSnapshot>,
}

impl AppState {
    pub fn new(config: ConfigStore) -> Result<Self, crate::config::ConfigError> {
        let statuses = config
            .get()?
            .stocks
            .iter()
            .filter_map(|stock| Symbol::from_config_code(stock))
            .map(|symbol| {
                (
                    symbol.key(),
                    QuoteStatus {
                        state: QuoteState::Loading,
                        last_success_at: None,
                        message: None,
                    },
                )
            })
            .collect();
        Ok(Self {
            config,
            snapshot: Mutex::new(QuoteSnapshot {
                revision: 0,
                quotes: Vec::new(),
                statuses,
            }),
        })
    }
}

#[derive(Debug, Serialize)]
pub struct CommandError {
    code: &'static str,
    message: String,
}

impl CommandError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

fn require_app_window(window: &WebviewWindow) -> Result<(), CommandError> {
    match window.label() {
        "main" | "settings" => Ok(()),
        _ => Err(CommandError::new(
            "unauthorized",
            "This window cannot access application settings or quotes.",
        )),
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigPatch {
    pub window: Option<WindowPatch>,
    pub display: Option<DisplayPatch>,
    pub refresh_interval: Option<u32>,
}

impl ConfigPatch {
    fn apply(self, config: &mut AppConfig) {
        if let Some(window) = self.window {
            window.apply(config);
        }
        if let Some(display) = self.display {
            display.apply(config);
        }
        if let Some(interval) = self.refresh_interval {
            config.refresh_interval = interval;
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WindowPatch {
    x: Option<i32>,
    y: Option<i32>,
    width: Option<u32>,
    height: Option<u32>,
    always_on_top: Option<bool>,
}

impl WindowPatch {
    fn apply(self, config: &mut AppConfig) {
        if let Some(value) = self.x {
            config.window.x = value;
        }
        if let Some(value) = self.y {
            config.window.y = value;
        }
        if let Some(value) = self.width {
            config.window.width = value;
        }
        if let Some(value) = self.height {
            config.window.height = value;
        }
        if let Some(value) = self.always_on_top {
            config.window.always_on_top = value;
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DisplayPatch {
    font_size: Option<u8>,
    background_opacity: Option<f64>,
    text_opacity: Option<f64>,
    show_name: Option<bool>,
    show_code: Option<bool>,
    show_price: Option<bool>,
    show_change: Option<bool>,
    show_change_percent: Option<bool>,
    show_turnover: Option<bool>,
    show_high: Option<bool>,
    show_low: Option<bool>,
    show_turnover_rate: Option<bool>,
    #[serde(rename = "showPE")]
    show_pe: Option<bool>,
    show_volume_ratio: Option<bool>,
    show_order_ratio: Option<bool>,
    order_book_depth: Option<u8>,
    red_for_rise: Option<bool>,
}

impl DisplayPatch {
    fn apply(self, config: &mut AppConfig) {
        let display = &mut config.display;
        macro_rules! set_if_present {
            ($($field:ident),+ $(,)?) => {
                $(if let Some(value) = self.$field { display.$field = value; })+
            };
        }
        set_if_present!(
            font_size,
            background_opacity,
            text_opacity,
            show_name,
            show_code,
            show_price,
            show_change,
            show_change_percent,
            show_turnover,
            show_high,
            show_low,
            show_turnover_rate,
            show_pe,
            show_volume_ratio,
            show_order_ratio,
            order_book_depth,
            red_for_rise,
        );
    }
}

#[tauri::command]
pub fn load_config(
    window: WebviewWindow,
    state: State<'_, Arc<AppState>>,
) -> Result<AppConfig, CommandError> {
    require_app_window(&window)?;
    state
        .config
        .get()
        .map_err(|error| CommandError::new("config_read", error.to_string()))
}

/// Apply only submitted fields to the current Rust config. Full stale configs are rejected.
#[tauri::command]
pub fn save_config(
    window: WebviewWindow,
    state: State<'_, Arc<AppState>>,
    patch: ConfigPatch,
) -> Result<AppConfig, CommandError> {
    require_app_window(&window)?;
    state
        .config
        .update(|config| patch.apply(config))
        .map_err(|error| CommandError::new("config_save", error.to_string()))
}

#[tauri::command]
pub fn get_quote_snapshot(
    window: WebviewWindow,
    state: State<'_, Arc<AppState>>,
) -> Result<QuoteSnapshot, CommandError> {
    require_app_window(&window)?;
    state
        .snapshot
        .lock()
        .map(|snapshot| snapshot.clone())
        .map_err(|_| CommandError::new("quote_state", "Quote state is unavailable."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separate_patches_preserve_previous_changes() {
        let mut current = AppConfig::default();
        let first: ConfigPatch = serde_json::from_value(serde_json::json!({
            "display": { "showCode": true }
        }))
        .unwrap();
        let second: ConfigPatch = serde_json::from_value(serde_json::json!({
            "display": { "showPE": true },
            "refreshInterval": 3000
        }))
        .unwrap();
        first.apply(&mut current);
        second.apply(&mut current);
        assert!(current.display.show_code);
        assert!(current.display.show_pe);
        assert_eq!(current.refresh_interval, 3_000);
        current.validate().unwrap();
    }

    #[test]
    fn rejects_whole_config_or_unknown_fields() {
        assert!(serde_json::from_value::<ConfigPatch>(serde_json::json!({
            "stocks": ["sh600519"]
        }))
        .is_err());
        assert!(serde_json::from_value::<ConfigPatch>(serde_json::json!({
            "display": { "showPee": true }
        }))
        .is_err());
    }
}
