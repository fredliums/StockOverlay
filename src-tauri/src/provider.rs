use crate::{
    quote::{QuoteBatchResult, QuoteFailure, QuoteFailureKind, Symbol},
    tencent::{parse_response, response_key},
};
use std::{future::Future, time::Duration};
use thiserror::Error;

const MAX_BATCH_SIZE: usize = 50;
const TENCENT_ENDPOINT: &str = "https://qt.gtimg.cn/q=";

#[derive(Debug, Error)]
pub enum QuoteError {
    #[error("could not initialize the quote HTTP client: {0}")]
    ClientInitialization(String),
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
    fn get(&self, query: &str) -> impl Future<Output = Result<TransportResponse, String>> + Send;
}

pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Result<Self, QuoteError> {
        let client = reqwest::Client::builder()
            .user_agent("StockOverlay/0.1")
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|error| QuoteError::ClientInitialization(error.to_string()))?;
        Ok(Self { client })
    }
}

impl QuoteTransport for ReqwestTransport {
    async fn get(&self, query: &str) -> Result<TransportResponse, String> {
        let response = self
            .client
            .get(format!("{TENCENT_ENDPOINT}{query}"))
            .send()
            .await
            .map_err(|error| error.to_string())?;
        let status = response.status().as_u16();
        let body = response
            .bytes()
            .await
            .map_err(|error| error.to_string())?
            .to_vec();
        Ok(TransportResponse { status, body })
    }
}

pub struct TencentQuoteProvider<T: QuoteTransport = ReqwestTransport> {
    transport: T,
}

impl TencentQuoteProvider<ReqwestTransport> {
    pub fn new() -> Result<Self, QuoteError> {
        Ok(Self {
            transport: ReqwestTransport::new()?,
        })
    }
}

impl<T: QuoteTransport> TencentQuoteProvider<T> {
    pub fn with_transport(transport: T) -> Self {
        Self { transport }
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
            let query = batch.iter().map(response_key).collect::<Vec<_>>().join(",");
            match self.transport.get(&query).await {
                Ok(response) if response.status == 200 => {
                    let parsed = parse_response(&response.body, batch);
                    result.quotes.extend(parsed.quotes);
                    result.failures.extend(parsed.failures);
                }
                Ok(response) => {
                    let kind = if response.status == 403 || response.status == 429 {
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
        responses: Mutex<VecDeque<Result<TransportResponse, String>>>,
        queries: Mutex<Vec<String>>,
    }

    impl MockTransport {
        fn new(responses: Vec<Result<TransportResponse, String>>) -> Self {
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
        async fn get(&self, query: &str) -> Result<TransportResponse, String> {
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
            Err("connection reset".into()),
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
    async fn forbidden_response_marks_each_stock_rate_limited() {
        let provider =
            TencentQuoteProvider::with_transport(MockTransport::new(vec![Ok(TransportResponse {
                status: 429,
                body: Vec::new(),
            })]));
        let result = provider
            .fetch_quotes(&[symbol("600519"), symbol("600000")])
            .await
            .unwrap();
        assert_eq!(result.failures.len(), 2);
        assert!(result
            .failures
            .iter()
            .all(|failure| failure.kind == QuoteFailureKind::RateLimited));
    }
}
