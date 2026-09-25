use crate::quote::{Market, Symbol};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const BUNDLED_STOCKS: &str = include_str!("../resources/stocks.json");
const MAX_SEARCH_RESULTS: usize = 20;
static STOCKS: OnceLock<Vec<StockEntry>> = OnceLock::new();

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockEntry {
    pub market: Market,
    pub code: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub legacy_code: Option<String>,
}

impl StockEntry {
    pub fn subscription_code(&self) -> String {
        format!("{}{}", self.market.as_str().to_ascii_lowercase(), self.code)
    }
}

pub fn all() -> &'static [StockEntry] {
    STOCKS.get_or_init(|| {
        serde_json::from_str(BUNDLED_STOCKS).expect("bundled stock index must be valid JSON")
    })
}

pub fn find_active(subscription_code: &str) -> Option<&'static StockEntry> {
    let symbol = Symbol::from_config_code(subscription_code)?;
    all()
        .iter()
        .find(|stock| stock.market == symbol.market && stock.code == symbol.code)
}

pub fn search(query: &str) -> Vec<StockEntry> {
    let query = query.trim();
    if query.is_empty() || query.chars().count() > 32 {
        return Vec::new();
    }
    let normalized = query.to_lowercase();
    let mut ranked: Vec<_> = all()
        .iter()
        .filter_map(|stock| {
            let rank = if stock.code == normalized
                || stock.legacy_code.as_deref() == Some(normalized.as_str())
            {
                0
            } else if stock.code.starts_with(&normalized) {
                1
            } else if stock
                .legacy_code
                .as_deref()
                .is_some_and(|old| old.starts_with(&normalized))
            {
                2
            } else if stock.name.to_lowercase().starts_with(&normalized) {
                3
            } else if stock.name.to_lowercase().contains(&normalized) {
                4
            } else {
                return None;
            };
            Some((rank, stock))
        })
        .collect();
    ranked.sort_by_key(|(rank, stock)| (*rank, stock.market.as_str(), stock.code.as_str()));
    ranked
        .into_iter()
        .take(MAX_SEARCH_RESULTS)
        .map(|(_, stock)| stock.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_index_covers_the_three_markets_and_verified_stock_names() {
        assert!(all().len() > 5_500);
        assert_eq!(search("600519")[0].name, "贵州茅台");
        assert_eq!(search("贵州茅台")[0].subscription_code(), "sh600519");
        assert_eq!(search("000001")[0].market, Market::SZ);
        assert_eq!(search("920021")[0].market, Market::BJ);
    }

    #[test]
    fn old_beijing_code_search_points_to_the_new_subscription_code() {
        let result = &search("834021")[0];
        assert_eq!(result.name, "流金科技");
        assert_eq!(result.code, "920021");
        assert_eq!(result.legacy_code.as_deref(), Some("834021"));
        assert_eq!(result.subscription_code(), "bj920021");
        assert!(find_active("bj834021").is_none());
        assert!(find_active("bj920021").is_some());
    }

    #[test]
    fn invalid_codes_and_empty_searches_do_not_return_stocks() {
        assert!(search("999999").is_empty());
        assert!(search(" ").is_empty());
        assert!(find_active("sh999999").is_none());
        assert!(find_active("sh600519").is_some());
        assert!(find_active("sz600519").is_none());
        assert!(search("股").len() <= MAX_SEARCH_RESULTS);
    }
}
