use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A market-qualified six-digit stock code. Use `key` for cache and status maps.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Symbol {
    pub code: String,
    pub market: Market,
}

impl Symbol {
    pub fn key(&self) -> String {
        format!("{}:{}", self.market.as_str(), self.code)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum Market {
    SH,
    SZ,
    BJ,
}

impl Market {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SH => "SH",
            Self::SZ => "SZ",
            Self::BJ => "BJ",
        }
    }
}

/// Provider-independent quote. Percentage fields contain percent values, not fractions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quote {
    pub symbol: String,
    pub market: Market,
    pub name: String,
    pub price: Option<f64>,
    pub previous_close: Option<f64>,
    pub change: Option<f64>,
    pub change_percent: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    /// Turnover in yuan.
    pub turnover: Option<f64>,
    pub turnover_rate: Option<f64>,
    /// Trailing twelve-month price-to-earnings ratio.
    pub pe: Option<f64>,
    pub volume_ratio: Option<f64>,
    pub order_ratio: Option<f64>,
    pub bid_levels: Vec<OrderLevel>,
    pub ask_levels: Vec<OrderLevel>,
    /// Exchange quote time as Unix milliseconds; never a local receipt time.
    pub timestamp: Option<i64>,
}

impl Quote {
    pub fn key(&self) -> String {
        format!("{}:{}", self.market.as_str(), self.symbol)
    }
}

/// Volume is measured in lots. Index zero in each side is level one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderLevel {
    pub price: Option<f64>,
    pub volume_lots: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuoteState {
    Loading,
    Normal,
    Delayed,
    Disconnected,
    Invalid,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteStatus {
    pub state: QuoteState,
    /// Local time when a valid quote was last received, as Unix milliseconds.
    pub last_success_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QuoteSnapshot {
    pub revision: u64,
    pub quotes: Vec<Quote>,
    /// Keys have the form `SH:600519`.
    pub statuses: HashMap<String, QuoteStatus>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct QuoteBatchResult {
    pub quotes: Vec<Quote>,
    pub failures: Vec<QuoteFailure>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QuoteFailure {
    pub symbol: Symbol,
    pub kind: QuoteFailureKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum QuoteFailureKind {
    InvalidSymbol,
    Parse,
    MissingRecord,
    Network,
    RateLimited,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, to_value};

    #[test]
    fn snapshot_json_matches_the_frontend_contract() {
        for (market, code) in [
            (Market::SH, "600519"),
            (Market::SZ, "000001"),
            (Market::BJ, "920001"),
        ] {
            let symbol = Symbol {
                code: code.into(),
                market,
            };
            let quote = Quote {
                symbol: code.into(),
                market,
                name: "Sample".into(),
                price: Some(10.5),
                previous_close: Some(10.0),
                change: Some(0.5),
                change_percent: Some(5.0),
                high: None,
                low: None,
                turnover: Some(12_500.0),
                turnover_rate: None,
                pe: None,
                volume_ratio: None,
                order_ratio: None,
                bid_levels: (0..5)
                    .map(|index| OrderLevel {
                        price: (index == 0).then_some(10.0),
                        volume_lots: (index == 0).then_some(100),
                    })
                    .collect(),
                ask_levels: (0..5)
                    .map(|index| OrderLevel {
                        price: (index == 0).then_some(10.6),
                        volume_lots: (index == 0).then_some(80),
                    })
                    .collect(),
                timestamp: None,
            };
            let snapshot = QuoteSnapshot {
                revision: 3,
                quotes: vec![quote],
                statuses: HashMap::from([(
                    symbol.key(),
                    QuoteStatus {
                        state: QuoteState::Delayed,
                        last_success_at: None,
                        message: None,
                    },
                )]),
            };

            let actual = to_value(&snapshot).unwrap();
            let expected = json!({
                "revision": 3,
                "quotes": [{
                    "symbol": code,
                    "market": market.as_str(),
                    "name": "Sample",
                    "price": 10.5,
                    "previousClose": 10.0,
                    "change": 0.5,
                    "changePercent": 5.0,
                    "high": null,
                    "low": null,
                    "turnover": 12500.0,
                    "turnoverRate": null,
                    "pe": null,
                    "volumeRatio": null,
                    "orderRatio": null,
                    "bidLevels": [
                        { "price": 10.0, "volumeLots": 100 },
                        { "price": null, "volumeLots": null },
                        { "price": null, "volumeLots": null },
                        { "price": null, "volumeLots": null },
                        { "price": null, "volumeLots": null }
                    ],
                    "askLevels": [
                        { "price": 10.6, "volumeLots": 80 },
                        { "price": null, "volumeLots": null },
                        { "price": null, "volumeLots": null },
                        { "price": null, "volumeLots": null },
                        { "price": null, "volumeLots": null }
                    ],
                    "timestamp": null
                }],
                "statuses": {
                    symbol.key(): { "state": "delayed", "lastSuccessAt": null }
                }
            });
            assert_eq!(actual, expected);
            assert_eq!(
                serde_json::from_value::<QuoteSnapshot>(actual).unwrap(),
                snapshot
            );
        }
    }

    #[test]
    fn provider_failures_keep_the_requested_symbol() {
        let failure = QuoteFailure {
            symbol: Symbol {
                code: "920001".into(),
                market: Market::BJ,
            },
            kind: QuoteFailureKind::MissingRecord,
        };

        assert_eq!(
            to_value(failure).unwrap(),
            json!({ "symbol": { "code": "920001", "market": "BJ" }, "kind": "MissingRecord" })
        );
    }
}
