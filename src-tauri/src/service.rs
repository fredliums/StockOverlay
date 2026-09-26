use crate::{
    commands::AppState,
    config::ConfigError,
    provider::QuoteProvider,
    quote::{
        Quote, QuoteBatchResult, QuoteFailureKind, QuoteSnapshot, QuoteState, QuoteStatus, Symbol,
    },
};
use chrono::{DateTime, Datelike, FixedOffset, Timelike, Utc, Weekday};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use thiserror::Error;

pub const QUOTE_UPDATE_EVENT: &str = "quote:update";
const OUTSIDE_MARKET_INTERVAL: Duration = Duration::from_secs(60);
const TIMER_RESOLUTION: Duration = Duration::from_millis(250);

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("configuration is unavailable: {0}")]
    Config(#[from] ConfigError),
    #[error("quote snapshot lock is poisoned")]
    SnapshotUnavailable,
    #[error("quote runtime state is unavailable")]
    RuntimeUnavailable,
    #[error("quote revision has reached its maximum")]
    RevisionExhausted,
}

pub struct QuoteService<P: QuoteProvider> {
    provider: P,
    state: Arc<AppState>,
    refresh_active: AtomicBool,
    refresh_requested: AtomicBool,
    runtime: Mutex<Runtime>,
    timer_task: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

#[derive(Default)]
struct Runtime {
    subscriptions: Vec<String>,
    consecutive_failures: HashMap<String, u32>,
    invalid: HashSet<String>,
}

impl Runtime {
    fn sync_subscriptions(&mut self, stocks: &[String]) -> bool {
        if self.subscriptions == stocks {
            return false;
        }
        self.subscriptions = stocks.to_vec();
        self.invalid.clear();
        let keys: HashSet<_> = stocks
            .iter()
            .filter_map(|stock| Symbol::from_config_code(stock))
            .map(|symbol| symbol.key())
            .collect();
        self.consecutive_failures
            .retain(|key, _| keys.contains(key));
        true
    }
}

struct RefreshGuard<'a>(&'a AtomicBool);

impl Drop for RefreshGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl<P: QuoteProvider> QuoteService<P> {
    pub fn new(provider: P, state: Arc<AppState>) -> Self {
        Self {
            provider,
            state,
            refresh_active: AtomicBool::new(false),
            refresh_requested: AtomicBool::new(false),
            runtime: Mutex::new(Runtime::default()),
            timer_task: Mutex::new(None),
        }
    }

    pub fn request_refresh(&self) {
        self.refresh_requested.store(true, Ordering::Release);
    }

    /// Returns `None` when the previous round is still running.
    pub async fn refresh_once(&self) -> Result<Option<QuoteSnapshot>, ServiceError> {
        if self
            .refresh_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(None);
        }
        let _guard = RefreshGuard(&self.refresh_active);
        let config = self.state.config.get()?;
        let (excluded, subscriptions_changed) = {
            let mut runtime = self
                .runtime
                .lock()
                .map_err(|_| ServiceError::RuntimeUnavailable)?;
            let changed = runtime.sync_subscriptions(&config.stocks);
            (runtime.invalid.clone(), changed)
        };
        let symbols: Vec<_> = config
            .stocks
            .iter()
            .filter_map(|stock| Symbol::from_config_code(stock))
            .filter(|symbol| !excluded.contains(&symbol.key()))
            .collect();
        let batch = if symbols.is_empty() {
            QuoteBatchResult::default()
        } else {
            match self.provider.fetch_quotes(&symbols).await {
                Ok(batch) => batch,
                Err(error) => {
                    eprintln!("quote provider failed: {error}");
                    QuoteBatchResult {
                        quotes: Vec::new(),
                        failures: symbols
                            .iter()
                            .cloned()
                            .map(|symbol| crate::quote::QuoteFailure {
                                symbol,
                                kind: QuoteFailureKind::Network,
                            })
                            .collect(),
                    }
                }
            }
        };
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| ServiceError::RuntimeUnavailable)?;
        let mut snapshot = self
            .state
            .snapshot
            .lock()
            .map_err(|_| ServiceError::SnapshotUnavailable)?;
        let current = self.state.config.get()?;
        let changed_during_request = config.stocks != current.stocks;
        let finish_changed = runtime.sync_subscriptions(&current.stocks);
        let reset_invalid = subscriptions_changed || finish_changed;
        let next_revision = snapshot
            .revision
            .checked_add(1)
            .ok_or(ServiceError::RevisionExhausted)?;
        if reset_invalid {
            for status in snapshot.statuses.values_mut() {
                if status.state == QuoteState::Invalid {
                    status.state = QuoteState::Loading;
                    status.message = None;
                }
            }
        }
        let mut cached: HashMap<_, _> = snapshot
            .quotes
            .drain(..)
            .map(|quote| (quote.key(), quote))
            .collect();
        let ordered_keys: Vec<_> = current
            .stocks
            .iter()
            .filter_map(|stock| Symbol::from_config_code(stock))
            .map(|symbol| symbol.key())
            .collect();
        let selected: HashSet<_> = ordered_keys.iter().cloned().collect();
        let now = Utc::now();
        let successful: HashSet<_> = batch.quotes.iter().map(Quote::key).collect();
        let failed: HashSet<_> = batch
            .failures
            .iter()
            .map(|failure| failure.symbol.key())
            .collect();
        for quote in batch.quotes {
            let key = quote.key();
            if selected.contains(&key) {
                snapshot
                    .statuses
                    .insert(key.clone(), status_for_quote(&quote, &now));
                runtime.consecutive_failures.remove(&key);
                runtime.invalid.remove(&key);
                cached.insert(key, quote);
            }
        }
        if !changed_during_request {
            for failure in batch.failures.into_iter().chain(
                symbols
                    .into_iter()
                    .filter(|symbol| {
                        let key = symbol.key();
                        !successful.contains(&key) && !failed.contains(&key)
                    })
                    .map(|symbol| crate::quote::QuoteFailure {
                        symbol,
                        kind: QuoteFailureKind::MissingRecord,
                    }),
            ) {
                let key = failure.symbol.key();
                if !selected.contains(&key) || successful.contains(&key) {
                    continue;
                }
                let last_success_at = snapshot
                    .statuses
                    .get(&key)
                    .and_then(|status| status.last_success_at);
                let state = if failure.kind == QuoteFailureKind::InvalidSymbol {
                    cached.remove(&key);
                    runtime.invalid.insert(key.clone());
                    runtime.consecutive_failures.remove(&key);
                    QuoteState::Invalid
                } else {
                    let count = runtime.consecutive_failures.entry(key.clone()).or_default();
                    *count = count.saturating_add(1);
                    if *count >= 2 {
                        QuoteState::Disconnected
                    } else {
                        QuoteState::Delayed
                    }
                };
                snapshot.statuses.insert(
                    key,
                    QuoteStatus {
                        state,
                        last_success_at,
                        message: Some(failure_message(failure.kind).into()),
                    },
                );
            }
        }
        for key in &ordered_keys {
            snapshot.statuses.entry(key.clone()).or_insert(QuoteStatus {
                state: QuoteState::Loading,
                last_success_at: None,
                message: None,
            });
        }
        snapshot.quotes = ordered_keys
            .iter()
            .filter_map(|key| cached.remove(key))
            .collect();
        snapshot.statuses.retain(|key, _| selected.contains(key));
        snapshot.revision = next_revision;
        Ok(Some(snapshot.clone()))
    }

    pub fn start(self: Arc<Self>, app: AppHandle)
    where
        P: 'static,
    {
        let service = Arc::clone(&self);
        let task = tauri::async_runtime::spawn(async move {
            let mut timer = tokio::time::interval(TIMER_RESOLUTION);
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut next_due = Instant::now();
            loop {
                timer.tick().await;
                let now = Instant::now();
                let requested = service.refresh_requested.load(Ordering::Acquire);
                if now < next_due && !requested {
                    continue;
                }
                let Ok(config) = service.state.config.get() else {
                    continue;
                };
                let beijing = Utc::now().with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap());
                next_due = now + refresh_interval(beijing, config.refresh_interval);
                if service.refresh_active.load(Ordering::Acquire) {
                    continue;
                }
                service.refresh_requested.store(false, Ordering::Release);
                let service = Arc::clone(&service);
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    match service.refresh_once().await {
                        Ok(Some(snapshot)) => {
                            if let Err(error) = handle.emit(QUOTE_UPDATE_EVENT, snapshot) {
                                eprintln!("could not emit quote update: {error}");
                            }
                        }
                        Ok(None) => {}
                        Err(error) => eprintln!("quote refresh failed: {error}"),
                    }
                });
            }
        });
        if let Ok(mut current) = self.timer_task.lock() {
            if let Some(old) = current.replace(task) {
                old.abort();
            }
        } else {
            task.abort();
        }
    }

    pub fn stop(&self) {
        if let Ok(mut current) = self.timer_task.lock() {
            if let Some(task) = current.take() {
                task.abort();
            }
        }
    }
}

fn status_for_quote(quote: &Quote, now: &DateTime<Utc>) -> QuoteStatus {
    let beijing = now.with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap());
    let message = if market_open(beijing) {
        match quote.timestamp {
            None => Some("Quote time is unavailable.".into()),
            Some(timestamp) if now.timestamp_millis().saturating_sub(timestamp) > 60_000 => {
                Some("Quote time is more than 60 seconds old.".into())
            }
            _ => None,
        }
    } else {
        None
    };
    QuoteStatus {
        state: if message.is_some() {
            QuoteState::Delayed
        } else {
            QuoteState::Normal
        },
        last_success_at: Some(now.timestamp_millis()),
        message,
    }
}

fn failure_message(kind: QuoteFailureKind) -> &'static str {
    match kind {
        QuoteFailureKind::InvalidSymbol => "Invalid stock code.",
        QuoteFailureKind::Parse => "Quote response could not be parsed.",
        QuoteFailureKind::MissingRecord => "Quote record was missing.",
        QuoteFailureKind::Network => "Quote request failed.",
        QuoteFailureKind::RateLimited => "Quote source rate limited requests.",
    }
}

fn market_open(beijing: DateTime<FixedOffset>) -> bool {
    if matches!(beijing.weekday(), Weekday::Sat | Weekday::Sun) {
        return false;
    }
    let minutes = beijing.hour() * 60 + beijing.minute();
    let morning = (9 * 60 + 15..=11 * 60 + 30).contains(&minutes);
    let afternoon = (13 * 60..=15 * 60).contains(&minutes);
    morning || afternoon
}

fn refresh_interval(beijing: DateTime<FixedOffset>, configured: u32) -> Duration {
    if market_open(beijing) {
        Duration::from_millis(u64::from(configured))
    } else {
        Duration::from_millis(u64::from(configured)).max(OUTSIDE_MARKET_INTERVAL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::ConfigStore,
        provider::{QuoteError, QuoteProvider},
        quote::{Market, Quote, QuoteFailure, QuoteFailureKind},
        tencent::parse_response,
    };
    use chrono::TimeZone;
    use std::{
        collections::VecDeque,
        fs,
        path::{Path, PathBuf},
        sync::{atomic::AtomicU64, Mutex},
    };
    use tokio::sync::Notify;

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "StockOverlay-service-test-{}-{sequence}",
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

    fn state(directory: &TestDirectory) -> Arc<AppState> {
        Arc::new(
            AppState::new(ConfigStore::open(directory.path().join("config.json")).unwrap())
                .unwrap(),
        )
    }

    fn quote(market: Market, code: &str) -> Quote {
        let symbol = Symbol {
            market,
            code: code.into(),
        };
        let bytes: &[u8] = match market {
            Market::SH => include_bytes!("../tests/fixtures/tencent/sh600519.gbk"),
            Market::SZ => include_bytes!("../tests/fixtures/tencent/sz000001.gbk"),
            Market::BJ => include_bytes!("../tests/fixtures/tencent/bj920021.gbk"),
        };
        parse_response(bytes, &[symbol]).quotes.remove(0)
    }

    struct FakeProvider {
        results: Mutex<VecDeque<QuoteBatchResult>>,
        requests: Mutex<Vec<Vec<Symbol>>>,
    }

    impl FakeProvider {
        fn new(results: Vec<QuoteBatchResult>) -> Self {
            Self {
                results: Mutex::new(results.into()),
                requests: Mutex::new(Vec::new()),
            }
        }
    }

    impl QuoteProvider for FakeProvider {
        async fn fetch_quotes(&self, symbols: &[Symbol]) -> Result<QuoteBatchResult, QuoteError> {
            self.requests.lock().unwrap().push(symbols.to_vec());
            Ok(self.results.lock().unwrap().pop_front().unwrap())
        }
    }

    fn failure(code: &str, kind: QuoteFailureKind) -> QuoteBatchResult {
        QuoteBatchResult {
            quotes: Vec::new(),
            failures: vec![QuoteFailure {
                symbol: Symbol::from_config_code(code).unwrap(),
                kind,
            }],
        }
    }

    #[test]
    fn configured_stocks_start_in_loading_state() {
        let directory = TestDirectory::new();
        let config = ConfigStore::open(directory.path().join("config.json")).unwrap();
        config
            .update(|settings| settings.stocks = vec!["sh600519".into(), "bj920021".into()])
            .unwrap();
        let state = AppState::new(config).unwrap();
        let snapshot = state.snapshot.lock().unwrap();
        assert_eq!(snapshot.revision, 0);
        assert!(snapshot.quotes.is_empty());
        assert_eq!(snapshot.statuses.len(), 2);
        for key in ["SH:600519", "BJ:920021"] {
            assert_eq!(snapshot.statuses[key].state, QuoteState::Loading);
            assert_eq!(snapshot.statuses[key].last_success_at, None);
        }
    }

    struct ErrorProvider;

    impl QuoteProvider for ErrorProvider {
        async fn fetch_quotes(&self, _: &[Symbol]) -> Result<QuoteBatchResult, QuoteError> {
            Err(QuoteError::StateUnavailable)
        }
    }

    #[tokio::test]
    async fn provider_error_still_advances_revision_and_failure_state() {
        let directory = TestDirectory::new();
        let state = state(&directory);
        state
            .config
            .update(|config| config.stocks = vec!["sh600519".into()])
            .unwrap();
        let service = QuoteService::new(ErrorProvider, state);
        let first = service.refresh_once().await.unwrap().unwrap();
        assert_eq!(first.revision, 1);
        assert_eq!(first.statuses["SH:600519"].state, QuoteState::Delayed);
        let second = service.refresh_once().await.unwrap().unwrap();
        assert_eq!(second.revision, 2);
        assert_eq!(second.statuses["SH:600519"].state, QuoteState::Disconnected);
    }

    #[tokio::test]
    async fn transitions_through_failure_and_recovery_without_losing_cached_quote() {
        let directory = TestDirectory::new();
        let state = state(&directory);
        state
            .config
            .update(|config| config.stocks = vec!["sh600519".into()])
            .unwrap();
        let mut received = quote(Market::SH, "600519");
        received.timestamp = Some(Utc::now().timestamp_millis());
        let service = QuoteService::new(
            FakeProvider::new(vec![
                QuoteBatchResult {
                    quotes: vec![received.clone()],
                    failures: Vec::new(),
                },
                failure("sh600519", QuoteFailureKind::Network),
                failure("sh600519", QuoteFailureKind::Network),
                QuoteBatchResult {
                    quotes: vec![received.clone()],
                    failures: Vec::new(),
                },
            ]),
            Arc::clone(&state),
        );
        let first = service.refresh_once().await.unwrap().unwrap();
        assert_eq!(first.statuses["SH:600519"].state, QuoteState::Normal);
        let last_success = first.statuses["SH:600519"].last_success_at;
        assert!(last_success.is_some());
        let second = service.refresh_once().await.unwrap().unwrap();
        assert_eq!(second.revision, 2);
        assert_eq!(second.statuses["SH:600519"].state, QuoteState::Delayed);
        assert_eq!(second.statuses["SH:600519"].last_success_at, last_success);
        assert_eq!(second.quotes, vec![received.clone()]);
        let third = service.refresh_once().await.unwrap().unwrap();
        assert_eq!(third.revision, 3);
        assert_eq!(third.statuses["SH:600519"].state, QuoteState::Disconnected);
        assert_eq!(third.quotes, vec![received.clone()]);
        let fourth = service.refresh_once().await.unwrap().unwrap();
        assert_eq!(fourth.revision, 4);
        assert_eq!(fourth.statuses["SH:600519"].state, QuoteState::Normal);
        assert_eq!(fourth.quotes, vec![received]);
    }

    #[tokio::test]
    async fn invalid_symbol_waits_for_watchlist_change_before_retry() {
        let directory = TestDirectory::new();
        let state = state(&directory);
        state
            .config
            .update(|config| config.stocks = vec!["sh600519".into()])
            .unwrap();
        let service = QuoteService::new(
            FakeProvider::new(vec![
                failure("sh600519", QuoteFailureKind::InvalidSymbol),
                QuoteBatchResult {
                    quotes: vec![quote(Market::SH, "600519")],
                    failures: Vec::new(),
                },
            ]),
            Arc::clone(&state),
        );
        let first = service.refresh_once().await.unwrap().unwrap();
        assert_eq!(first.statuses["SH:600519"].state, QuoteState::Invalid);
        let second = service.refresh_once().await.unwrap().unwrap();
        assert_eq!(second.revision, 2);
        assert_eq!(second.statuses["SH:600519"].state, QuoteState::Invalid);
        assert_eq!(service.provider.requests.lock().unwrap().len(), 1);
        state
            .config
            .update(|config| config.stocks.push("sz000001".into()))
            .unwrap();
        let third = service.refresh_once().await.unwrap().unwrap();
        assert_eq!(third.revision, 3);
        assert_ne!(third.statuses["SH:600519"].state, QuoteState::Invalid);
        assert_eq!(service.provider.requests.lock().unwrap().len(), 2);
    }

    #[test]
    fn flags_stale_exchange_time_only_during_market_hours() {
        let zone = FixedOffset::east_opt(8 * 3600).unwrap();
        let market_time = zone
            .with_ymd_and_hms(2026, 9, 25, 10, 0, 0)
            .single()
            .unwrap()
            .with_timezone(&Utc);
        let after_close = zone
            .with_ymd_and_hms(2026, 9, 25, 16, 0, 0)
            .single()
            .unwrap()
            .with_timezone(&Utc);
        let mut received = quote(Market::SH, "600519");
        received.timestamp = Some(market_time.timestamp_millis() - 61_000);
        let delayed = status_for_quote(&received, &market_time);
        assert_eq!(delayed.state, QuoteState::Delayed);
        assert_eq!(
            delayed.last_success_at,
            Some(market_time.timestamp_millis())
        );
        assert_eq!(
            status_for_quote(&received, &after_close).state,
            QuoteState::Normal
        );
        received.timestamp = None;
        assert_eq!(
            status_for_quote(&received, &market_time).state,
            QuoteState::Delayed
        );
        received.timestamp = Some(market_time.timestamp_millis() - 60_000);
        assert_eq!(
            status_for_quote(&received, &market_time).state,
            QuoteState::Normal
        );
    }

    #[tokio::test]
    async fn merges_complete_rounds_in_current_subscription_order() {
        let directory = TestDirectory::new();
        let state = state(&directory);
        state
            .config
            .update(|config| {
                config.stocks = vec!["sh600519".into(), "sz000001".into()];
            })
            .unwrap();
        let sh = quote(Market::SH, "600519");
        let sz = quote(Market::SZ, "000001");
        let provider = FakeProvider::new(vec![
            QuoteBatchResult {
                quotes: vec![sz.clone(), sh.clone()],
                failures: Vec::new(),
            },
            QuoteBatchResult {
                quotes: vec![sz],
                failures: vec![QuoteFailure {
                    symbol: Symbol {
                        market: Market::SH,
                        code: "600519".into(),
                    },
                    kind: QuoteFailureKind::Network,
                }],
            },
            QuoteBatchResult::default(),
        ]);
        let service = QuoteService::new(provider, Arc::clone(&state));

        let first = service.refresh_once().await.unwrap().unwrap();
        assert_eq!(first.revision, 1);
        assert_eq!(
            first.quotes.iter().map(Quote::key).collect::<Vec<_>>(),
            ["SH:600519", "SZ:000001"]
        );
        let second = service.refresh_once().await.unwrap().unwrap();
        assert_eq!(second.revision, 2);
        assert_eq!(second.quotes.len(), 2);
        assert_eq!(second.quotes[0], sh);

        state
            .config
            .update(|config| config.stocks = vec!["sz000001".into()])
            .unwrap();
        let third = service.refresh_once().await.unwrap().unwrap();
        assert_eq!(third.revision, 3);
        assert_eq!(third.quotes.len(), 1);
        assert_eq!(third.quotes[0].key(), "SZ:000001");
        assert_eq!(service.provider.requests.lock().unwrap().len(), 3);

        state.config.update(|config| config.stocks.clear()).unwrap();
        let empty = service.refresh_once().await.unwrap().unwrap();
        assert_eq!(empty.revision, 4);
        assert!(empty.quotes.is_empty());
        assert_eq!(service.provider.requests.lock().unwrap().len(), 3);
    }

    struct SlowProvider {
        entered: Notify,
        release: Notify,
        calls: AtomicU64,
        result: QuoteBatchResult,
    }

    impl QuoteProvider for SlowProvider {
        async fn fetch_quotes(&self, _: &[Symbol]) -> Result<QuoteBatchResult, QuoteError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.entered.notify_one();
            self.release.notified().await;
            Ok(self.result.clone())
        }
    }

    #[tokio::test]
    async fn skips_a_trigger_while_the_previous_request_is_running() {
        let directory = TestDirectory::new();
        let state = state(&directory);
        state
            .config
            .update(|config| config.stocks.push("sh600519".into()))
            .unwrap();
        let service = Arc::new(QuoteService::new(
            SlowProvider {
                entered: Notify::new(),
                release: Notify::new(),
                calls: AtomicU64::new(0),
                result: QuoteBatchResult::default(),
            },
            state,
        ));
        let first_service = Arc::clone(&service);
        let first = tokio::spawn(async move { first_service.refresh_once().await });
        service.provider.entered.notified().await;
        assert!(service.refresh_once().await.unwrap().is_none());
        assert_eq!(service.provider.calls.load(Ordering::Relaxed), 1);
        service.provider.release.notify_one();
        assert_eq!(first.await.unwrap().unwrap().unwrap().revision, 1);
    }

    #[tokio::test]
    async fn removal_during_an_inflight_request_does_not_restore_the_stock() {
        let directory = TestDirectory::new();
        let state = state(&directory);
        state
            .config
            .update(|config| config.stocks.push("sh600519".into()))
            .unwrap();
        let service = Arc::new(QuoteService::new(
            SlowProvider {
                entered: Notify::new(),
                release: Notify::new(),
                calls: AtomicU64::new(0),
                result: QuoteBatchResult {
                    quotes: vec![quote(Market::SH, "600519")],
                    failures: Vec::new(),
                },
            },
            Arc::clone(&state),
        ));
        let pending = Arc::clone(&service);
        let refresh = tokio::spawn(async move { pending.refresh_once().await });
        service.provider.entered.notified().await;
        state.config.update(|config| config.stocks.clear()).unwrap();
        service.provider.release.notify_one();
        let snapshot = refresh.await.unwrap().unwrap().unwrap();
        assert!(snapshot.quotes.is_empty());
        assert!(snapshot.statuses.is_empty());
    }

    #[test]
    fn uses_market_hours_and_the_current_refresh_setting() {
        let zone = FixedOffset::east_opt(8 * 3600).unwrap();
        let at = |year, month, day, hour, minute| {
            zone.with_ymd_and_hms(year, month, day, hour, minute, 0)
                .single()
                .unwrap()
        };
        assert_eq!(
            refresh_interval(at(2026, 9, 25, 9, 15), 1_000),
            Duration::from_secs(1)
        );
        assert_eq!(
            refresh_interval(at(2026, 9, 25, 11, 31), 1_000),
            OUTSIDE_MARKET_INTERVAL
        );
        assert_eq!(
            refresh_interval(at(2026, 9, 25, 13, 0), 5_000),
            Duration::from_secs(5)
        );
        assert_eq!(
            refresh_interval(at(2026, 9, 25, 15, 1), 5_000),
            OUTSIDE_MARKET_INTERVAL
        );
        assert_eq!(
            refresh_interval(at(2026, 9, 26, 10, 0), 1_000),
            OUTSIDE_MARKET_INTERVAL
        );
    }
}
