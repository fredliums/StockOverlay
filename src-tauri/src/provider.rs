use crate::{
    quote::{QuoteBatchResult, QuoteFailure, QuoteFailureKind, Symbol},
    tencent::{parse_response, response_key},
};
use std::{
    future::Future,
    sync::Mutex,
    time::{Duration, Instant},
};
use thiserror::Error;

const MAX_BATCH_SIZE: usize = 50;
const TENCENT_ENDPOINT: &str = "https://qt.gtimg.cn/q=";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_DELAY: Duration = Duration::from_millis(500);
const RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(60);

#[derive(Debug, Error)]
pub enum QuoteError {
    #[error("could not initialize the quote HTTP client: {0}")]
    ClientInitialization(String),
    #[error("quote provider state is unavailable")]
    StateUnavailable,
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("request timed out")]
    Timeout,
    #[error("network request failed: {0}")]
    Network(String),
}

pub trait QuoteProvider: Send + Sync {
    fn fetch_quotes(
        &self,
        symbols: &[Symbol],
    ) -> impl Future<Output = Result<QuoteBatchResult, QuoteError>> + Send;
}

pub struct TransportResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

pub trait QuoteTransport: Send + Sync {
    fn get(
        &self,
        query: &str,
    ) -> impl Future<Output = Result<TransportResponse, TransportError>> + Send;
}

pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Result<Self, QuoteError> {
        let client = reqwest::Client::builder()
            .user_agent("StockOverlay/0.1")
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| QuoteError::ClientInitialization(error.to_string()))?;
        Ok(Self { client })
    }
}

impl QuoteTransport for ReqwestTransport {
    async fn get(&self, query: &str) -> Result<TransportResponse, TransportError> {
        let response = self
            .client
            .get(format!("{TENCENT_ENDPOINT}{query}"))
            .send()
            .await
            .map_err(transport_error)?;
        let status = response.status().as_u16();
        let body = if status == 200 {
            response.bytes().await.map_err(transport_error)?.to_vec()
        } else {
            Vec::new()
        };
        Ok(TransportResponse { status, body })
    }
}

fn transport_error(error: reqwest::Error) -> TransportError {
    if error.is_timeout() {
        TransportError::Timeout
    } else {
        TransportError::Network(error.to_string())
    }
}

pub struct TencentQuoteProvider<T: QuoteTransport = ReqwestTransport> {
    transport: T,
    cooldown_until: Mutex<Option<Instant>>,
}

impl TencentQuoteProvider<ReqwestTransport> {
    pub fn new() -> Result<Self, QuoteError> {
        Ok(Self {
            transport: ReqwestTransport::new()?,
            cooldown_until: Mutex::new(None),
        })
    }
}

impl<T: QuoteTransport> TencentQuoteProvider<T> {
    pub fn with_transport(transport: T) -> Self {
        Self {
            transport,
            cooldown_until: Mutex::new(None),
        }
    }

    fn is_cooling_down(&self) -> Result<bool, QuoteError> {
        let until = self
            .cooldown_until
            .lock()
            .map_err(|_| QuoteError::StateUnavailable)?;
        Ok(until.is_some_and(|time| Instant::now() < time))
    }

    fn start_cooldown(&self) -> Result<(), QuoteError> {
        let mut until = self
            .cooldown_until
            .lock()
            .map_err(|_| QuoteError::StateUnavailable)?;
        *until = Some(Instant::now() + RATE_LIMIT_COOLDOWN);
        Ok(())
    }

    async fn request_batch(&self, query: &str) -> Result<TransportResponse, TransportError> {
        let first = self.transport.get(query).await;
        let retry = matches!(&first, Err(TransportError::Timeout))
            || matches!(&first, Ok(response) if (500..600).contains(&response.status));
        if retry {
            tokio::time::sleep(RETRY_DELAY).await;
            self.transport.get(query).await
        } else {
            first
        }
    }
}

impl<T: QuoteTransport> QuoteProvider for TencentQuoteProvider<T> {
    async fn fetch_quotes(&self, symbols: &[Symbol]) -> Result<QuoteBatchResult, QuoteError> {
        let mut result = QuoteBatchResult::default();
        if symbols.is_empty() {
            return Ok(result);
        }

        let mut valid = Vec::with_capacity(symbols.len());
        for symbol in symbols {
            if valid_symbol(symbol) {
                valid.push(symbol.clone());
            } else {
                result.failures.push(QuoteFailure {
                    symbol: symbol.clone(),
                    kind: QuoteFailureKind::InvalidSymbol,
                });
            }
        }

        for batch in valid.chunks(MAX_BATCH_SIZE) {
            if self.is_cooling_down()? {
                result
                    .failures
                    .extend(batch_failures(batch, QuoteFailureKind::RateLimited));
                continue;
            }
            let query = batch.iter().map(response_key).collect::<Vec<_>>().join(",");
            match self.request_batch(&query).await {
                Ok(response) if response.status == 200 => {
                    let parsed = parse_response(&response.body, batch);
                    result.quotes.extend(parsed.quotes);
                    result.failures.extend(parsed.failures);
                }
                Ok(response) => {
                    let kind = if response.status == 403 || response.status == 429 {
                        self.start_cooldown()?;
                        QuoteFailureKind::RateLimited
                    } else {
                        QuoteFailureKind::Network
                    };
                    result.failures.extend(batch_failures(batch, kind));
                }
                Err(_) => result
                    .failures
                    .extend(batch_failures(batch, QuoteFailureKind::Network)),
            }
        }
        Ok(result)
    }
}

fn batch_failures(batch: &[Symbol], kind: QuoteFailureKind) -> Vec<QuoteFailure> {
    batch
        .iter()
        .cloned()
        .map(|symbol| QuoteFailure { symbol, kind })
        .collect()
}

fn valid_symbol(symbol: &Symbol) -> bool {
    symbol.code.len() == 6
        && symbol.code.bytes().all(|byte| byte.is_ascii_digit())
        && (symbol.market != crate::quote::Market::BJ || symbol.code.starts_with("920"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quote::Market;
    use std::{
        collections::{HashSet, VecDeque},
        sync::Mutex,
    };

    struct MockTransport {
        responses: Mutex<VecDeque<Result<TransportResponse, TransportError>>>,
        queries: Mutex<Vec<String>>,
    }

    impl MockTransport {
        fn new(responses: Vec<Result<TransportResponse, TransportError>>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                queries: Mutex::new(Vec::new()),
            }
        }

        fn queries(&self) -> Vec<String> {
            self.queries.lock().unwrap().clone()
        }
    }

    impl QuoteTransport for MockTransport {
        async fn get(&self, query: &str) -> Result<TransportResponse, TransportError> {
            self.queries.lock().unwrap().push(query.into());
            self.responses.lock().unwrap().pop_front().unwrap()
        }
    }

    fn symbol(code: &str) -> Symbol {
        Symbol {
            code: code.into(),
            market: Market::SH,
        }
    }

    fn quote_record(code: &str) -> String {
        format!("v_sh{code}=\"1~Sample~{code}~10~9\";")
    }

    #[tokio::test]
    async fn empty_and_invalid_inputs_never_call_http() {
        let provider = TencentQuoteProvider::with_transport(MockTransport::new(vec![]));
        assert_eq!(
            provider.fetch_quotes(&[]).await.unwrap(),
            QuoteBatchResult::default()
        );
        let invalid = Symbol {
            code: "830001".into(),
            market: Market::BJ,
        };
        let result = provider.fetch_quotes(&[invalid.clone()]).await.unwrap();
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].symbol, invalid);
        assert_eq!(result.failures[0].kind, QuoteFailureKind::InvalidSymbol);
        assert!(provider.transport.queries().is_empty());
    }

    #[tokio::test]
    async fn splits_large_requests_and_keeps_successful_batches() {
        let requested: Vec<_> = (600000..=600050)
            .map(|code| symbol(&code.to_string()))
            .collect();
        let first_batch = requested[..50]
            .iter()
            .map(|stock| quote_record(&stock.code))
            .collect::<String>();
        let provider = TencentQuoteProvider::with_transport(MockTransport::new(vec![
            Ok(TransportResponse {
                status: 200,
                body: first_batch.into_bytes(),
            }),
            Err(TransportError::Network("connection reset".into())),
        ]));

        let result = provider.fetch_quotes(&requested).await.unwrap();
        let queries = provider.transport.queries();
        assert_eq!(queries.len(), 2);
        assert_eq!(queries[0].split(',').count(), 50);
        assert_eq!(queries[1], "sh600050");
        assert_eq!(result.quotes.len(), 50);
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].symbol, requested[50]);
        assert_eq!(result.failures[0].kind, QuoteFailureKind::Network);
        let reported: HashSet<_> = result
            .quotes
            .iter()
            .map(|quote| quote.key())
            .chain(result.failures.iter().map(|failure| failure.symbol.key()))
            .collect();
        let expected: HashSet<_> = requested.iter().map(Symbol::key).collect();
        assert_eq!(reported, expected);
    }

    #[tokio::test]
    async fn missing_record_and_invalid_marker_affect_only_the_missing_stock() {
        let stocks = [symbol("600519"), symbol("600000")];
        let provider =
            TencentQuoteProvider::with_transport(MockTransport::new(vec![Ok(TransportResponse {
                status: 200,
                body: quote_record("600519").into_bytes(),
            })]));
        let result = provider.fetch_quotes(&stocks).await.unwrap();
        assert_eq!(result.quotes.len(), 1);
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].symbol, stocks[1]);
        assert_eq!(result.failures[0].kind, QuoteFailureKind::MissingRecord);

        let mixed = format!("{}v_pv_none_match=\"1\";", quote_record("600519"));
        let provider =
            TencentQuoteProvider::with_transport(MockTransport::new(vec![Ok(TransportResponse {
                status: 200,
                body: mixed.into_bytes(),
            })]));
        let result = provider.fetch_quotes(&stocks).await.unwrap();
        assert_eq!(result.quotes.len(), 1);
        assert_eq!(result.failures[0].kind, QuoteFailureKind::InvalidSymbol);
    }

    #[tokio::test]
    async fn forbidden_responses_start_cooldown_without_more_requests() {
        for status in [403, 429] {
            let provider = TencentQuoteProvider::with_transport(MockTransport::new(vec![Ok(
                TransportResponse {
                    status,
                    body: Vec::new(),
                },
            )]));
            let stocks: Vec<_> = (600000..=600050)
                .map(|code| symbol(&code.to_string()))
                .collect();
            let first = provider.fetch_quotes(&stocks).await.unwrap();
            assert_eq!(first.failures.len(), 51);
            assert!(first
                .failures
                .iter()
                .all(|failure| failure.kind == QuoteFailureKind::RateLimited));
            assert_eq!(provider.transport.queries().len(), 1);

            let second = provider.fetch_quotes(&stocks[..1]).await.unwrap();
            assert_eq!(second.failures[0].kind, QuoteFailureKind::RateLimited);
            assert_eq!(provider.transport.queries().len(), 1);
            let deadline = provider.cooldown_until.lock().unwrap().unwrap();
            assert!(deadline.duration_since(Instant::now()) >= Duration::from_secs(59));
        }
    }

    #[tokio::test]
    async fn timeout_and_server_error_retry_once_after_backoff() {
        for first in [
            Err(TransportError::Timeout),
            Ok(TransportResponse {
                status: 503,
                body: Vec::new(),
            }),
        ] {
            let provider = TencentQuoteProvider::with_transport(MockTransport::new(vec![
                first,
                Ok(TransportResponse {
                    status: 200,
                    body: quote_record("600519").into_bytes(),
                }),
            ]));
            let started = Instant::now();
            let result = provider.fetch_quotes(&[symbol("600519")]).await.unwrap();
            assert!(started.elapsed() >= RETRY_DELAY);
            assert_eq!(provider.transport.queries().len(), 2);
            assert_eq!(result.quotes.len(), 1);
            assert!(result.failures.is_empty());
        }
    }

    #[tokio::test]
    async fn client_errors_do_not_retry() {
        let provider =
            TencentQuoteProvider::with_transport(MockTransport::new(vec![Ok(TransportResponse {
                status: 404,
                body: Vec::new(),
            })]));
        let result = provider.fetch_quotes(&[symbol("600519")]).await.unwrap();
        assert_eq!(result.failures[0].kind, QuoteFailureKind::Network);
        assert_eq!(provider.transport.queries().len(), 1);
    }
}
