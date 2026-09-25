use crate::{
    config::{AppConfig, ConfigError, ConfigStore},
    provider::TencentQuoteProvider,
    quote::{QuoteSnapshot, QuoteState, QuoteStatus, Symbol},
    service::{QuoteService, QUOTE_UPDATE_EVENT},
    stock_index::{self, StockEntry},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
use tauri::{Emitter, Manager, State, WebviewWindow};

pub struct AppState {
    pub(crate) config: ConfigStore,
    pub(crate) snapshot: Mutex<QuoteSnapshot>,
    watchlist_lock: Mutex<()>,
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
            watchlist_lock: Mutex::new(()),
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
    let config = state
        .config
        .update(|config| patch.apply(config))
        .map_err(|error| CommandError::new("config_save", error.to_string()))?;
    publish_config_change(&window, &config);
    Ok(config)
}

pub(crate) fn publish_config_change(window: &WebviewWindow, config: &AppConfig) {
    if let Err(error) = window.emit("config:update", config.clone()) {
        eprintln!("could not emit config update: {error}");
    }
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

#[tauri::command]
pub fn get_window_mode(window: WebviewWindow) -> Result<bool, CommandError> {
    require_app_window(&window)?;
    crate::window_control::current_mode(&window.app_handle())
        .map_err(|error| CommandError::new("window_mode", error))
}

#[tauri::command]
pub async fn open_settings(window: WebviewWindow) -> Result<(), CommandError> {
    require_app_window(&window)?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || crate::tray::open_settings(&app))
        .await
        .map_err(|error| CommandError::new("settings_window", error.to_string()))?
        .map_err(|error| CommandError::new("settings_window", error))
}

#[tauri::command]
pub fn get_shortcut_status(
    window: WebviewWindow,
) -> Result<crate::shortcuts::ShortcutStatus, CommandError> {
    require_app_window(&window)?;
    crate::shortcuts::status(&window.app_handle())
        .map_err(|error| CommandError::new("shortcut_status", error))
}

#[tauri::command]
pub fn change_shortcut(
    window: WebviewWindow,
    action: String,
    key: String,
) -> Result<crate::shortcuts::ShortcutStatus, CommandError> {
    require_app_window(&window)?;
    let status = crate::shortcuts::change(&window.app_handle(), &action, &key)
        .map_err(|error| CommandError::new("shortcut_change", error))?;
    if let Ok(config) = window.state::<Arc<AppState>>().config.get() {
        publish_config_change(&window, &config);
    }
    Ok(status)
}

#[tauri::command]
pub fn search_stocks(
    window: WebviewWindow,
    query: String,
) -> Result<Vec<StockEntry>, CommandError> {
    require_app_window(&window)?;
    Ok(stock_index::search(&query))
}

#[tauri::command]
pub fn get_watchlist_entries(
    window: WebviewWindow,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<StockEntry>, CommandError> {
    require_app_window(&window)?;
    let config = state
        .config
        .get()
        .map_err(|error| CommandError::new("config_read", error.to_string()))?;
    Ok(config
        .stocks
        .iter()
        .filter_map(|code| stock_index::find_active(code).cloned())
        .collect())
}

#[tauri::command]
pub fn set_content_min_size(
    window: WebviewWindow,
    width: u32,
    height: u32,
) -> Result<(), CommandError> {
    if window.label() != "main" {
        return Err(CommandError::new(
            "unauthorized",
            "Only the overlay can set its minimum size.",
        ));
    }
    crate::window_placement::set_content_min_size(&window.app_handle(), width, height)
        .map_err(|error| CommandError::new("window_size", error))
}

fn add_stock_to_state(
    state: &AppState,
    code: &str,
) -> Result<(AppConfig, QuoteSnapshot), CommandError> {
    let _watchlist_guard = state
        .watchlist_lock
        .lock()
        .map_err(|_| CommandError::new("stock_state", "Watchlist is unavailable."))?;
    if stock_index::find_active(code).is_none() {
        let former = code.strip_prefix("bj").unwrap_or(code);
        if let Some(stock) = stock_index::search(former)
            .into_iter()
            .find(|stock| stock.legacy_code.as_deref() == Some(former))
        {
            return Err(CommandError::new(
                "legacy_stock_code",
                format!(
                    "Old BSE code {former}; use {} ({}) instead.",
                    stock.subscription_code(),
                    stock.name
                ),
            ));
        }
        return Err(CommandError::new(
            "invalid_stock",
            format!("Unknown or invalid stock code {code}."),
        ));
    }
    let config = state
        .config
        .update_checked(|config| {
            if config.stocks.iter().any(|stock| stock == code) {
                return Err(ConfigError::Invalid(format!(
                    "stock {code} is already selected"
                )));
            }
            config.stocks.push(code.into());
            Ok(())
        })
        .map_err(|error| CommandError::new("stock_add", error.to_string()))?;
    let snapshot = sync_watchlist_snapshot(state, &config.stocks)?;
    Ok((config, snapshot))
}

fn remove_stock_from_state(
    state: &AppState,
    code: &str,
) -> Result<(AppConfig, QuoteSnapshot), CommandError> {
    let _watchlist_guard = state
        .watchlist_lock
        .lock()
        .map_err(|_| CommandError::new("stock_state", "Watchlist is unavailable."))?;
    let config = state
        .config
        .update_checked(|config| {
            let Some(index) = config.stocks.iter().position(|stock| stock == code) else {
                return Err(ConfigError::Invalid(format!(
                    "stock {code} is not selected"
                )));
            };
            config.stocks.remove(index);
            Ok(())
        })
        .map_err(|error| CommandError::new("stock_remove", error.to_string()))?;
    let snapshot = sync_watchlist_snapshot(state, &config.stocks)?;
    Ok((config, snapshot))
}

fn reorder_stocks_in_state(
    state: &AppState,
    codes: Vec<String>,
) -> Result<(AppConfig, QuoteSnapshot), CommandError> {
    let _watchlist_guard = state
        .watchlist_lock
        .lock()
        .map_err(|_| CommandError::new("stock_state", "Watchlist is unavailable."))?;
    let config = state
        .config
        .update_checked(|config| {
            let selected: HashSet<_> = config.stocks.iter().collect();
            let requested: HashSet<_> = codes.iter().collect();
            if codes.len() != config.stocks.len() || selected != requested {
                return Err(ConfigError::Invalid(
                    "stock order must contain each selected stock exactly once".into(),
                ));
            }
            config.stocks = codes;
            Ok(())
        })
        .map_err(|error| CommandError::new("stock_reorder", error.to_string()))?;
    let snapshot = sync_watchlist_snapshot(state, &config.stocks)?;
    Ok((config, snapshot))
}

fn sync_watchlist_snapshot(
    state: &AppState,
    stocks: &[String],
) -> Result<QuoteSnapshot, CommandError> {
    let mut snapshot = state
        .snapshot
        .lock()
        .map_err(|_| CommandError::new("quote_state", "Quote state is unavailable."))?;
    let next = snapshot
        .revision
        .checked_add(1)
        .ok_or_else(|| CommandError::new("quote_revision", "Quote revision is exhausted."))?;
    let mut quotes: HashMap<_, _> = snapshot
        .quotes
        .drain(..)
        .map(|quote| (quote.key(), quote))
        .collect();
    let keys: Vec<_> = stocks
        .iter()
        .filter_map(|stock| Symbol::from_config_code(stock))
        .map(|symbol| symbol.key())
        .collect();
    let selected: HashSet<_> = keys.iter().cloned().collect();
    snapshot.quotes = keys.iter().filter_map(|key| quotes.remove(key)).collect();
    snapshot.statuses.retain(|key, _| selected.contains(key));
    for key in keys {
        let status = snapshot.statuses.entry(key).or_insert(QuoteStatus {
            state: QuoteState::Loading,
            last_success_at: None,
            message: None,
        });
        if status.state == QuoteState::Invalid {
            status.state = QuoteState::Loading;
            status.message = None;
        }
    }
    snapshot.revision = next;
    Ok(snapshot.clone())
}

fn publish_watchlist_change(
    window: &WebviewWindow,
    service: &QuoteService<TencentQuoteProvider>,
    snapshot: QuoteSnapshot,
) {
    if let Err(error) = window.emit(QUOTE_UPDATE_EVENT, snapshot) {
        eprintln!("could not emit watchlist update: {error}");
    }
    service.request_refresh();
}

#[tauri::command]
pub fn add_stock(
    window: WebviewWindow,
    state: State<'_, Arc<AppState>>,
    service: State<'_, Arc<QuoteService<TencentQuoteProvider>>>,
    code: String,
) -> Result<AppConfig, CommandError> {
    require_app_window(&window)?;
    let (config, snapshot) = add_stock_to_state(&state, &code)?;
    publish_watchlist_change(&window, &service, snapshot);
    publish_config_change(&window, &config);
    Ok(config)
}

#[tauri::command]
pub fn remove_stock(
    window: WebviewWindow,
    state: State<'_, Arc<AppState>>,
    service: State<'_, Arc<QuoteService<TencentQuoteProvider>>>,
    code: String,
) -> Result<AppConfig, CommandError> {
    require_app_window(&window)?;
    let (config, snapshot) = remove_stock_from_state(&state, &code)?;
    publish_watchlist_change(&window, &service, snapshot);
    publish_config_change(&window, &config);
    Ok(config)
}

#[tauri::command]
pub fn reorder_stocks(
    window: WebviewWindow,
    state: State<'_, Arc<AppState>>,
    service: State<'_, Arc<QuoteService<TencentQuoteProvider>>>,
    codes: Vec<String>,
) -> Result<AppConfig, CommandError> {
    require_app_window(&window)?;
    let (config, snapshot) = reorder_stocks_in_state(&state, codes)?;
    publish_watchlist_change(&window, &service, snapshot);
    publish_config_change(&window, &config);
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tencent::parse_response;
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let number = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "StockOverlay-watchlist-test-{}-{number}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn add_rejects_duplicate_unknown_and_former_beijing_codes() {
        let directory = TestDirectory::new();
        let path = directory.path().join("config.json");
        let state = AppState::new(ConfigStore::open(path.clone()).unwrap()).unwrap();
        let (config, snapshot) = add_stock_to_state(&state, "sh600519").unwrap();
        assert_eq!(config.stocks, ["sh600519"]);
        assert_eq!(snapshot.statuses["SH:600519"].state, QuoteState::Loading);
        assert_eq!(
            add_stock_to_state(&state, "sh600519").unwrap_err().code,
            "stock_add"
        );
        assert_eq!(
            add_stock_to_state(&state, "sh999999").unwrap_err().code,
            "invalid_stock"
        );
        let old = add_stock_to_state(&state, "bj834021").unwrap_err();
        assert_eq!(old.code, "legacy_stock_code");
        assert!(old.message.contains("bj920021"));
        assert_eq!(state.config.get().unwrap().stocks, ["sh600519"]);
        assert_eq!(
            ConfigStore::open(path).unwrap().get().unwrap().stocks,
            ["sh600519"]
        );
    }

    #[test]
    fn reorder_and_remove_persist_and_prune_cached_quotes() {
        let directory = TestDirectory::new();
        let path = directory.path().join("config.json");
        let state = AppState::new(ConfigStore::open(path.clone()).unwrap()).unwrap();
        add_stock_to_state(&state, "sh600519").unwrap();
        add_stock_to_state(&state, "sz000001").unwrap();
        add_stock_to_state(&state, "bj920021").unwrap();
        let sh = parse_response(
            include_bytes!("../tests/fixtures/tencent/sh600519.gbk"),
            &[Symbol::from_config_code("sh600519").unwrap()],
        )
        .quotes
        .remove(0);
        let sz = parse_response(
            include_bytes!("../tests/fixtures/tencent/sz000001.gbk"),
            &[Symbol::from_config_code("sz000001").unwrap()],
        )
        .quotes
        .remove(0);
        state.snapshot.lock().unwrap().quotes = vec![sh, sz];
        let order = vec!["sz000001".into(), "sh600519".into(), "bj920021".into()];
        let (config, snapshot) = reorder_stocks_in_state(&state, order.clone()).unwrap();
        assert_eq!(config.stocks, order);
        assert_eq!(
            snapshot
                .quotes
                .iter()
                .map(|quote| quote.key())
                .collect::<Vec<_>>(),
            ["SZ:000001", "SH:600519"]
        );
        assert!(reorder_stocks_in_state(
            &state,
            vec!["sz000001".into(), "sz000001".into(), "bj920021".into()]
        )
        .is_err());
        let (config, snapshot) = remove_stock_from_state(&state, "sh600519").unwrap();
        assert_eq!(config.stocks, ["sz000001", "bj920021"]);
        assert_eq!(snapshot.quotes.len(), 1);
        assert_eq!(snapshot.quotes[0].key(), "SZ:000001");
        assert!(!snapshot.statuses.contains_key("SH:600519"));
        assert!(remove_stock_from_state(&state, "sh600519").is_err());
        assert_eq!(
            ConfigStore::open(path).unwrap().get().unwrap().stocks,
            ["sz000001", "bj920021"]
        );
    }

    #[test]
    fn concurrent_adds_keep_the_persisted_watchlist_and_snapshot_aligned() {
        let directory = TestDirectory::new();
        let path = directory.path().join("config.json");
        let state = Arc::new(AppState::new(ConfigStore::open(path.clone()).unwrap()).unwrap());
        std::thread::scope(|scope| {
            for code in ["sh600519", "sz000001"] {
                let state = Arc::clone(&state);
                scope.spawn(move || add_stock_to_state(&state, code).unwrap());
            }
        });
        let config = state.config.get().unwrap();
        let snapshot = state.snapshot.lock().unwrap();
        assert_eq!(config.stocks.len(), 2);
        assert_eq!(snapshot.statuses.len(), 2);
        assert_eq!(snapshot.revision, 2);
        assert_eq!(
            ConfigStore::open(path).unwrap().get().unwrap().stocks,
            config.stocks
        );
    }

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
