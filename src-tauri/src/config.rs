use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    env, fs,
    fs::OpenOptions,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{atomic::AtomicU64, atomic::Ordering, Mutex},
};
use thiserror::Error;

const SCHEMA_VERSION: u32 = 1;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub schema_version: u32,
    pub window: WindowConfig,
    pub display: DisplayConfig,
    pub shortcuts: ShortcutConfig,
    /// Exchange-prefixed subscription codes such as `sh600519`.
    pub stocks: Vec<String>,
    pub refresh_interval: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            window: WindowConfig::default(),
            display: DisplayConfig::default(),
            shortcuts: ShortcutConfig::default(),
            stocks: Vec::new(),
            refresh_interval: 2_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowConfig {
    /// Logical pixel position; monitor visibility is checked when windows are created.
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub locked: bool,
    pub always_on_top: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            x: 100,
            y: 100,
            width: 560,
            height: 180,
            locked: false,
            always_on_top: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayConfig {
    pub font_size: u8,
    pub background_opacity: f64,
    pub text_opacity: f64,
    pub show_name: bool,
    pub show_code: bool,
    pub show_price: bool,
    pub show_change: bool,
    pub show_change_percent: bool,
    pub show_turnover: bool,
    pub show_high: bool,
    pub show_low: bool,
    pub show_turnover_rate: bool,
    #[serde(rename = "showPE")]
    pub show_pe: bool,
    pub show_volume_ratio: bool,
    pub show_order_ratio: bool,
    pub order_book_depth: u8,
    pub red_for_rise: bool,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            font_size: 18,
            background_opacity: 0.0,
            text_opacity: 1.0,
            show_name: true,
            show_code: false,
            show_price: true,
            show_change: false,
            show_change_percent: true,
            show_turnover: false,
            show_high: false,
            show_low: false,
            show_turnover_rate: false,
            show_pe: false,
            show_volume_ratio: false,
            show_order_ratio: false,
            order_book_depth: 1,
            red_for_rise: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutConfig {
    pub toggle_lock: String,
    pub toggle_visibility: String,
}

impl Default for ShortcutConfig {
    fn default() -> Self {
        Self {
            toggle_lock: "Ctrl+Alt+L".into(),
            toggle_visibility: "Ctrl+Alt+S".into(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("configuration I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("invalid configuration: {0}")]
    Invalid(String),
    #[error("APPDATA is not available")]
    MissingAppData,
    #[error("configuration lock is poisoned")]
    LockPoisoned,
}

impl AppConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(ConfigError::Invalid(format!(
                "unsupported schema version {}",
                self.schema_version
            )));
        }
        if !(-32_768..=32_768).contains(&self.window.x)
            || !(-32_768..=32_768).contains(&self.window.y)
            || !(180..=4_096).contains(&self.window.width)
            || !(80..=2_160).contains(&self.window.height)
        {
            return Err(ConfigError::Invalid(
                "window bounds are out of range".into(),
            ));
        }
        if !(10..=48).contains(&self.display.font_size)
            || ![self.display.background_opacity, self.display.text_opacity]
                .iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        {
            return Err(ConfigError::Invalid(
                "display appearance is out of range".into(),
            ));
        }
        if ![0, 1, 3, 5].contains(&self.display.order_book_depth) {
            return Err(ConfigError::Invalid("unsupported order book depth".into()));
        }
        if ![1_000, 2_000, 3_000, 5_000].contains(&self.refresh_interval) {
            return Err(ConfigError::Invalid("unsupported refresh interval".into()));
        }
        let lock = shortcut_key(&self.shortcuts.toggle_lock)?;
        let visibility = shortcut_key(&self.shortcuts.toggle_visibility)?;
        if lock == visibility {
            return Err(ConfigError::Invalid(
                "shortcut actions must use distinct keys".into(),
            ));
        }
        let mut seen = HashSet::new();
        for stock in &self.stocks {
            if !valid_stock_code(stock) {
                return Err(ConfigError::Invalid(format!("invalid stock code {stock}")));
            }
            if !seen.insert(stock) {
                return Err(ConfigError::Invalid(format!(
                    "duplicate stock code {stock}"
                )));
            }
        }
        Ok(())
    }
}

/// Basic shape validation. The search index later confirms that a code is listed.
fn valid_stock_code(value: &str) -> bool {
    let (market, code) = match value.get(..2).zip(value.get(2..)) {
        Some(parts) => parts,
        None => return false,
    };
    if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    match market {
        "sh" | "sz" => true,
        "bj" => code.starts_with("920"),
        _ => false,
    }
}

fn shortcut_key(value: &str) -> Result<String, ConfigError> {
    let parts: Vec<_> = value.split('+').collect();
    if parts.len() < 2 {
        return Err(ConfigError::Invalid(format!("invalid shortcut {value}")));
    }
    let key = parts.last().unwrap().to_ascii_uppercase();
    let valid_key = (key.len() == 1 && key.bytes().all(|byte| byte.is_ascii_alphanumeric()))
        || key
            .strip_prefix('F')
            .and_then(|number| number.parse::<u8>().ok())
            .is_some_and(|number| (1..=24).contains(&number));
    let mut modifiers = HashSet::new();
    for part in &parts[..parts.len() - 1] {
        let modifier = part.to_ascii_uppercase();
        if !["CTRL", "ALT", "SHIFT", "SUPER"].contains(&modifier.as_str())
            || !modifiers.insert(modifier)
        {
            return Err(ConfigError::Invalid(format!("invalid shortcut {value}")));
        }
    }
    if !valid_key {
        return Err(ConfigError::Invalid(format!("invalid shortcut {value}")));
    }
    let mut modifiers: Vec<_> = modifiers.into_iter().collect();
    modifiers.sort();
    Ok(format!("{}+{key}", modifiers.join("+")))
}

pub fn config_path() -> Result<PathBuf, ConfigError> {
    env::var_os("APPDATA")
        .map(|root| PathBuf::from(root).join("StockOverlay").join("config.json"))
        .ok_or(ConfigError::MissingAppData)
}

/// Owns the only config writer. A failed update leaves the in-memory value unchanged.
pub struct ConfigStore {
    path: PathBuf,
    current: Mutex<AppConfig>,
}

impl ConfigStore {
    pub fn open(path: PathBuf) -> Result<Self, ConfigError> {
        let config = match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<AppConfig>(&bytes) {
                Ok(config) if config.validate().is_ok() => config,
                _ => {
                    preserve_invalid_file(&path, &bytes)?;
                    let defaults = AppConfig::default();
                    write_atomic(&path, &defaults)?;
                    defaults
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let defaults = AppConfig::default();
                write_atomic(&path, &defaults)?;
                defaults
            }
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            path,
            current: Mutex::new(config),
        })
    }

    pub fn open_default() -> Result<Self, ConfigError> {
        Self::open(config_path()?)
    }

    pub fn get(&self) -> Result<AppConfig, ConfigError> {
        Ok(self
            .current
            .lock()
            .map_err(|_| ConfigError::LockPoisoned)?
            .clone())
    }

    pub fn update(&self, apply: impl FnOnce(&mut AppConfig)) -> Result<AppConfig, ConfigError> {
        self.update_checked(|config| {
            apply(config);
            Ok(())
        })
    }

    pub fn update_checked(
        &self,
        apply: impl FnOnce(&mut AppConfig) -> Result<(), ConfigError>,
    ) -> Result<AppConfig, ConfigError> {
        let mut current = self.current.lock().map_err(|_| ConfigError::LockPoisoned)?;
        let mut next = current.clone();
        apply(&mut next)?;
        next.validate()?;
        write_atomic(&self.path, &next)?;
        *current = next.clone();
        Ok(next)
    }
}

fn preserve_invalid_file(path: &Path, bytes: &[u8]) -> Result<(), ConfigError> {
    for suffix in 1..=1_000 {
        let backup = path.with_file_name(format!("config.invalid.{suffix}.json"));
        match OpenOptions::new().write(true).create_new(true).open(backup) {
            Ok(mut file) => {
                file.write_all(bytes)?;
                file.sync_all()?;
                return Ok(());
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(ConfigError::Invalid(
        "too many invalid config backups".into(),
    ))
}

fn write_atomic(path: &Path, config: &AppConfig) -> Result<(), ConfigError> {
    let parent = path
        .parent()
        .ok_or_else(|| ConfigError::Invalid("configuration path has no parent directory".into()))?;
    fs::create_dir_all(parent)?;
    let bytes = serde_json::to_vec_pretty(config)
        .map_err(|error| ConfigError::Invalid(error.to_string()))?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temp = path.with_file_name(format!("config.{}.{}.tmp", std::process::id(), sequence));
    let result = (|| -> io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(ConfigError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!(
                "StockOverlay-config-test-{}-{sequence}",
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
    fn creates_defaults_and_persists_valid_changes() {
        let directory = TestDirectory::new();
        let path = directory.path().join("config.json");
        let store = ConfigStore::open(path.clone()).unwrap();
        assert_eq!(store.get().unwrap(), AppConfig::default());
        store
            .update(|config| {
                config.display.show_pe = true;
                config.stocks.push("bj920021".into());
            })
            .unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("\"showPE\": true"));
        assert_eq!(
            ConfigStore::open(path).unwrap().get().unwrap(),
            store.get().unwrap()
        );
    }

    #[test]
    fn backs_up_invalid_json_before_restoring_defaults() {
        let directory = TestDirectory::new();
        let path = directory.path().join("config.json");
        fs::write(&path, b"{broken json").unwrap();
        let store = ConfigStore::open(path).unwrap();
        assert_eq!(store.get().unwrap(), AppConfig::default());
        assert_eq!(
            fs::read(directory.path().join("config.invalid.1.json")).unwrap(),
            b"{broken json"
        );
    }

    #[test]
    fn backs_up_incompatible_schema_before_restoring_defaults() {
        let directory = TestDirectory::new();
        let path = directory.path().join("config.json");
        let mut incompatible = AppConfig::default();
        incompatible.schema_version = 2;
        let original = serde_json::to_vec(&incompatible).unwrap();
        fs::write(&path, &original).unwrap();

        assert_eq!(
            ConfigStore::open(path).unwrap().get().unwrap(),
            AppConfig::default()
        );
        assert_eq!(
            fs::read(directory.path().join("config.invalid.1.json")).unwrap(),
            original
        );
    }

    #[test]
    fn validates_codes_shortcuts_and_numeric_ranges() {
        let mut config = AppConfig::default();
        config.stocks.push("bj830001".into());
        assert!(config.validate().is_err());
        config.stocks = vec!["sh600519".into(), "sh600519".into()];
        assert!(config.validate().is_err());
        config.stocks.pop();
        config.shortcuts.toggle_visibility = "alt+ctrl+l".into();
        assert!(config.validate().is_err());
        config.shortcuts.toggle_visibility = "Ctrl+Alt+S".into();
        config.display.background_opacity = f64::NAN;
        assert!(config.validate().is_err());
        config.display.background_opacity = 0.0;
        config.refresh_interval = 1_500;
        assert!(config.validate().is_err());
    }

    #[test]
    fn concurrent_updates_keep_both_settings() {
        let directory = TestDirectory::new();
        let path = directory.path().join("config.json");
        let store = Arc::new(ConfigStore::open(path.clone()).unwrap());
        let first = Arc::clone(&store);
        let second = Arc::clone(&store);
        let a = std::thread::spawn(move || first.update(|config| config.display.show_code = true));
        let b = std::thread::spawn(move || second.update(|config| config.refresh_interval = 5_000));
        a.join().unwrap().unwrap();
        b.join().unwrap().unwrap();
        let persisted = ConfigStore::open(path).unwrap().get().unwrap();
        assert!(persisted.display.show_code);
        assert_eq!(persisted.refresh_interval, 5_000);
    }

    #[test]
    fn invalid_update_does_not_replace_saved_config() {
        let directory = TestDirectory::new();
        let path = directory.path().join("config.json");
        let store = ConfigStore::open(path.clone()).unwrap();
        let original = fs::read(&path).unwrap();
        assert!(store
            .update(|config| config.display.order_book_depth = 2)
            .is_err());
        assert_eq!(store.get().unwrap(), AppConfig::default());
        assert_eq!(fs::read(path).unwrap(), original);
    }

    #[cfg(windows)]
    #[test]
    fn failed_replace_keeps_old_file_and_memory_value() {
        use std::os::windows::fs::OpenOptionsExt;

        let directory = TestDirectory::new();
        let path = directory.path().join("config.json");
        let store = ConfigStore::open(path.clone()).unwrap();
        let original = fs::read(&path).unwrap();
        let exclusive_handle = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();

        assert!(store
            .update(|config| config.display.show_code = true)
            .is_err());
        assert_eq!(store.get().unwrap(), AppConfig::default());
        drop(exclusive_handle);
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(
            fs::read_dir(directory.path()).unwrap().count(),
            1,
            "failed writes must remove their temporary file"
        );
    }
}
