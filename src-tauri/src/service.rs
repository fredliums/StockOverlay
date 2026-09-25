use crate::{
    commands::AppState,
    config::ConfigError,
    provider::{QuoteError, QuoteProvider},
    quote::{Market, QuoteBatchResult, QuoteSnapshot, Symbol},
};
use chrono::{DateTime, Datelike, FixedOffset, Timelike, Utc, Weekday};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
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
    #[error("quote provider failed: {0}")]
    Provider(#[from] QuoteError),
    #[error("quote snapshot lock is poisoned")]
    SnapshotUnavailable,
    #[error("quote revision has reached its maximum")]
    RevisionExhausted,
}

pub struct QuoteService<P: QuoteProvider> {
    provider: P,
    state: Arc<AppState>,
    refresh_active: AtomicBool,
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
        }
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
        let symbols: Vec<_> = config
            .stocks
            .iter()
            .filter_map(|stock| parse_stock(stock))
            .collect();
        let batch = if symbols.is_empty() {
            QuoteBatchResult::default()
        } else {
            self.provider.fetch_quotes(&symbols).await?
        };
        let current = self.state.config.get()?;
        let mut snapshot = self
            .state
            .snapshot
            .lock()
            .map_err(|_| ServiceError::SnapshotUnavailable)?;
        let next_revision = snapshot
            .revision
            .checked_add(1)
            .ok_or(ServiceError::RevisionExhausted)?;
        let mut cached: HashMap<_, _> = snapshot
            .quotes
            .drain(..)
            .map(|quote| (quote.key(), quote))
            .collect();
        for quote in batch.quotes {
            cached.insert(quote.key(), quote);
        }
        let ordered_keys: Vec<_> = current
            .stocks
            .iter()
            .filter_map(|stock| parse_stock(stock))
            .map(|symbol| symbol.key())
            .collect();
        snapshot.quotes = ordered_keys
            .iter()
            .filter_map(|key| cached.remove(key))
            .collect();
        snapshot
            .statuses
            .retain(|key, _| ordered_keys.contains(key));
        snapshot.revision = next_revision;
        Ok(Some(snapshot.clone()))
    }

    pub fn start(self: Arc<Self>, app: AppHandle)
    where
        P: 'static,
    {
        tauri::async_runtime::spawn(async move {
            let mut timer = tokio::time::interval(TIMER_RESOLUTION);
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut next_due = Instant::now();
            loop {
                timer.tick().await;
                let now = Instant::now();
                if now < next_due {
                    continue;
                }
                let Ok(config) = self.state.config.get() else {
                    continue;
                };
                let beijing = Utc::now().with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap());
                next_due = now + refresh_interval(beijing, config.refresh_interval);
                if self.refresh_active.load(Ordering::Acquire) {
                    continue;
                }
                let service = Arc::clone(&self);
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
    }
}

fn parse_stock(value: &str) -> Option<Symbol> {
    let (prefix, code) = value.get(..2).zip(value.get(2..))?;
    let market = match prefix {
        "sh" => Market::SH,
        "sz" => Market::SZ,
        "bj" => Market::BJ,
        _ => return None,
    };
    Some(Symbol {
        market,
        code: code.into(),
    })
}

fn refresh_interval(beijing: DateTime<FixedOffset>, configured: u32) -> Duration {
    if matches!(beijing.weekday(), Weekday::Sat | Weekday::Sun) {
        return OUTSIDE_MARKET_INTERVAL;
    }
    let minutes = beijing.hour() * 60 + beijing.minute();
    let morning = (9 * 60 + 15..=11 * 60 + 30).contains(&minutes);
    let afternoon = (13 * 60..=15 * 60).contains(&minutes);
    if morning || afternoon {
        Duration::from_millis(u64::from(configured))
    } else {
        OUTSIDE_MARKET_INTERVAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::ConfigStore,
        provider::QuoteProvider,
        quote::{Quote, QuoteFailure, QuoteFailureKind},
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
        Arc::new(AppState::new(
            ConfigStore::open(directory.path().join("config.json")).unwrap(),
        ))
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
    }

    impl QuoteProvider for SlowProvider {
        async fn fetch_quotes(&self, _: &[Symbol]) -> Result<QuoteBatchResult, QuoteError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.entered.notify_one();
            self.release.notified().await;
            Ok(QuoteBatchResult::default())
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
