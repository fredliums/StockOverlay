use crate::quote::{OrderLevel, Quote, QuoteBatchResult, QuoteFailure, QuoteFailureKind, Symbol};
use chrono::{FixedOffset, NaiveDateTime, TimeZone};
use encoding_rs::GBK;
use std::collections::HashMap;

/// Decode one Tencent response and retain a result for every requested symbol.
pub fn parse_response(bytes: &[u8], requested: &[Symbol]) -> QuoteBatchResult {
    let mut result = QuoteBatchResult::default();
    if requested.is_empty() {
        return result;
    }
    let (decoded, _, had_errors) = GBK.decode(bytes);
    if had_errors {
        return fail_all(requested, QuoteFailureKind::Parse);
    }
    let records = split_records(&decoded);
    let invalid_marker = records.contains_key("pv_none_match");
    let missing_count = requested
        .iter()
        .filter(|symbol| !records.contains_key(&response_key(symbol)))
        .count();
    for symbol in requested {
        let key = response_key(symbol);
        let outcome = match records.get(&key) {
            Some(payload) => parse_quote(symbol, payload),
            None if invalid_marker && missing_count == 1 => Err(QuoteFailureKind::InvalidSymbol),
            None => Err(QuoteFailureKind::MissingRecord),
        };
        match outcome {
            Ok(quote) => result.quotes.push(quote),
            Err(kind) => result.failures.push(QuoteFailure {
                symbol: symbol.clone(),
                kind,
            }),
        }
    }
    result
}

pub(crate) fn response_key(symbol: &Symbol) -> String {
    format!(
        "{}{}",
        symbol.market.as_str().to_ascii_lowercase(),
        symbol.code
    )
}

fn fail_all(requested: &[Symbol], kind: QuoteFailureKind) -> QuoteBatchResult {
    QuoteBatchResult {
        quotes: Vec::new(),
        failures: requested
            .iter()
            .cloned()
            .map(|symbol| QuoteFailure { symbol, kind })
            .collect(),
    }
}

fn split_records(decoded: &str) -> HashMap<String, &str> {
    decoded
        .split(';')
        .filter_map(|record| {
            let record = record.trim();
            let (key, quoted_payload) = record.strip_prefix("v_")?.split_once('=')?;
            let payload = quoted_payload.strip_prefix('"')?.strip_suffix('"')?;
            if key.is_empty() {
                return None;
            }
            Some((key.to_string(), payload))
        })
        .collect()
}

fn parse_quote(symbol: &Symbol, payload: &str) -> Result<Quote, QuoteFailureKind> {
    let fields: Vec<_> = payload.split('~').collect();
    let name = field(&fields, 1).ok_or(QuoteFailureKind::Parse)?;
    let response_code = field(&fields, 2).ok_or(QuoteFailureKind::Parse)?;
    if response_code != symbol.code {
        return Err(QuoteFailureKind::Parse);
    }
    let price = number(&fields, 3).filter(|value| *value > 0.0);
    if price.is_none() {
        return Err(QuoteFailureKind::Parse);
    }
    let previous_close = number(&fields, 4).filter(|value| *value > 0.0);
    let turnover_wan = number(&fields, 57)
        .filter(|value| *value >= 0.0)
        .or_else(|| number(&fields, 37).filter(|value| *value >= 0.0));
    let turnover = turnover_wan.and_then(|value| {
        let yuan = value * 10_000.0;
        yuan.is_finite().then_some(yuan)
    });
    let bid_levels = order_levels(&fields, 9);
    let ask_levels = order_levels(&fields, 19);
    let order_ratio = order_ratio(&bid_levels, &ask_levels);

    Ok(Quote {
        symbol: symbol.code.clone(),
        market: symbol.market,
        name: name.into(),
        price,
        previous_close,
        change: number(&fields, 31),
        change_percent: number(&fields, 32),
        high: number(&fields, 33).filter(|value| *value > 0.0),
        low: number(&fields, 34).filter(|value| *value > 0.0),
        turnover,
        turnover_rate: number(&fields, 38),
        pe: number(&fields, 39),
        volume_ratio: number(&fields, 49),
        order_ratio,
        bid_levels,
        ask_levels,
        timestamp: field(&fields, 30).and_then(parse_beijing_time),
    })
}

fn order_levels(fields: &[&str], first_price_index: usize) -> Vec<OrderLevel> {
    (0..5)
        .map(|level| {
            let price_index = first_price_index + level * 2;
            OrderLevel {
                price: number(fields, price_index).filter(|price| *price > 0.0),
                volume_lots: field(fields, price_index + 1)
                    .and_then(|value| value.parse::<u64>().ok()),
            }
        })
        .collect()
}

fn order_ratio(bids: &[OrderLevel], asks: &[OrderLevel]) -> Option<f64> {
    if bids.len() != 5 || asks.len() != 5 {
        return None;
    }
    let sum_lots = |levels: &[OrderLevel]| {
        levels.iter().try_fold(0u128, |sum, level| {
            Some(sum + u128::from(level.volume_lots?))
        })
    };
    let bid_lots = sum_lots(bids)?;
    let ask_lots = sum_lots(asks)?;
    let total = bid_lots + ask_lots;
    if total == 0 {
        return None;
    }
    Some((bid_lots as f64 - ask_lots as f64) * 100.0 / total as f64)
}

fn field<'a>(fields: &'a [&str], index: usize) -> Option<&'a str> {
    fields
        .get(index)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}

fn number(fields: &[&str], index: usize) -> Option<f64> {
    field(fields, index)
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite())
}

fn parse_beijing_time(value: &str) -> Option<i64> {
    let local = NaiveDateTime::parse_from_str(value, "%Y%m%d%H%M%S").ok()?;
    let beijing = FixedOffset::east_opt(8 * 60 * 60)?;
    beijing
        .from_local_datetime(&local)
        .single()
        .map(|time| time.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quote::Market;

    fn symbol(market: Market, code: &str) -> Symbol {
        Symbol {
            market,
            code: code.into(),
        }
    }

    #[test]
    fn decodes_three_markets_from_captured_gbk_bytes() {
        for (market, code, bytes, expected_name, expected_turnover) in [
            (
                Market::SH,
                "600519",
                &include_bytes!("../tests/fixtures/tencent/sh600519.gbk")[..],
                "贵州茅台",
                3_867_310_920.0,
            ),
            (
                Market::SZ,
                "000001",
                &include_bytes!("../tests/fixtures/tencent/sz000001.gbk")[..],
                "平安银行",
                1_186_736_896.0,
            ),
            (
                Market::BJ,
                "920021",
                &include_bytes!("../tests/fixtures/tencent/bj920021.gbk")[..],
                "流金科技",
                176_290_597.0,
            ),
        ] {
            let result = parse_response(bytes, &[symbol(market, code)]);
            assert!(result.failures.is_empty());
            let quote = &result.quotes[0];
            assert_eq!(quote.name, expected_name);
            assert_eq!(quote.symbol, code);
            assert_eq!(quote.market, market);
            assert!((quote.turnover.unwrap() - expected_turnover).abs() < 0.001);
            assert!(quote.timestamp.is_some());
        }
    }

    #[test]
    fn parses_batch_and_preserves_input_order() {
        let requested = [symbol(Market::SZ, "000001"), symbol(Market::SH, "600519")];
        let result = parse_response(
            include_bytes!("../tests/fixtures/tencent/batch.gbk"),
            &requested,
        );
        assert!(result.failures.is_empty());
        assert_eq!(result.quotes.len(), 2);
        assert_eq!(result.quotes[0].symbol, "000001");
        assert_eq!(result.quotes[1].symbol, "600519");
    }

    #[test]
    fn rejects_invalid_code_mismatched_key_and_bad_core_price() {
        let requested = symbol(Market::SH, "600519");
        let invalid = parse_response(
            include_bytes!("../tests/fixtures/tencent/invalid.gbk"),
            &[symbol(Market::SH, "000000")],
        );
        assert_eq!(invalid.failures[0].kind, QuoteFailureKind::InvalidSymbol);
        assert_eq!(invalid.quotes.len(), 0);

        let mismatch = parse_response(b"v_sz600519=\"1~Other~600519~10~9\";", &[requested.clone()]);
        assert_eq!(mismatch.failures[0].kind, QuoteFailureKind::MissingRecord);
        let bad_price = parse_response(b"v_sh600519=\"1~Other~600519~0~9\";", &[requested]);
        assert_eq!(bad_price.failures[0].kind, QuoteFailureKind::Parse);
    }

    #[test]
    fn missing_optional_fields_remain_null_and_turnover_falls_back() {
        let response = b"v_sh600519=\"1~Sample~600519~10~9\";";
        let quote = &parse_response(response, &[symbol(Market::SH, "600519")]).quotes[0];
        assert_eq!(quote.high, None);
        assert_eq!(quote.turnover, None);
        assert_eq!(quote.timestamp, None);

        let mut fields = vec![""; 38];
        fields[1] = "Sample";
        fields[2] = "600519";
        fields[3] = "10";
        fields[4] = "9";
        fields[37] = "12.5";
        let response = format!("v_sh600519=\"{}\";", fields.join("~"));
        let quote = &parse_response(response.as_bytes(), &[symbol(Market::SH, "600519")]).quotes[0];
        assert_eq!(quote.turnover, Some(125_000.0));
    }

    #[test]
    fn rejects_malformed_gbk_and_invalid_calendar_time() {
        let requested = symbol(Market::SH, "600519");
        let invalid_gbk = parse_response(&[0xff], &[requested.clone()]);
        assert_eq!(invalid_gbk.failures[0].kind, QuoteFailureKind::Parse);
        assert_eq!(parse_beijing_time("20260230120000"), None);
        assert_eq!(
            parse_beijing_time("20260924161444"),
            Some(1_790_237_684_000)
        );
    }

    #[test]
    fn parses_five_levels_and_calculates_order_ratio_from_lots() {
        for (market, code, bytes, bid_one, ask_one, bid_five, ask_five, expected_ratio) in [
            (
                Market::SH,
                "600519",
                &include_bytes!("../tests/fixtures/tencent/sh600519.gbk")[..],
                (1237.0, 13),
                (1237.05, 1),
                (1236.51, 2),
                (1237.97, 1),
                (21.0 - 5.0) * 100.0 / 26.0,
            ),
            (
                Market::BJ,
                "920021",
                &include_bytes!("../tests/fixtures/tencent/bj920021.gbk")[..],
                (7.86, 596),
                (7.87, 49),
                (7.82, 113),
                (7.91, 43),
                (1651.0 - 1468.0) * 100.0 / 3119.0,
            ),
        ] {
            let result = parse_response(bytes, &[symbol(market, code)]);
            let quote = &result.quotes[0];
            assert_eq!(quote.bid_levels.len(), 5);
            assert_eq!(quote.ask_levels.len(), 5);
            assert_eq!(quote.bid_levels[0].price, Some(bid_one.0));
            assert_eq!(quote.bid_levels[0].volume_lots, Some(bid_one.1));
            assert_eq!(quote.ask_levels[0].price, Some(ask_one.0));
            assert_eq!(quote.ask_levels[0].volume_lots, Some(ask_one.1));
            assert_eq!(quote.bid_levels[4].price, Some(bid_five.0));
            assert_eq!(quote.bid_levels[4].volume_lots, Some(bid_five.1));
            assert_eq!(quote.ask_levels[4].price, Some(ask_five.0));
            assert_eq!(quote.ask_levels[4].volume_lots, Some(ask_five.1));
            assert!((quote.order_ratio.unwrap() - expected_ratio).abs() < 1e-10);
        }
    }

    #[test]
    fn missing_or_zero_total_lots_do_not_produce_an_order_ratio() {
        let mut fields = vec![""; 29];
        fields[1] = "Sample";
        fields[2] = "600519";
        fields[3] = "10";
        fields[4] = "9";
        for level in 0..5 {
            fields[9 + level * 2] = "9.9";
            fields[10 + level * 2] = "0";
            fields[19 + level * 2] = "10.1";
            fields[20 + level * 2] = "0";
        }
        let response = format!("v_sh600519=\"{}\";", fields.join("~"));
        let quote = &parse_response(response.as_bytes(), &[symbol(Market::SH, "600519")]).quotes[0];
        assert_eq!(quote.order_ratio, None);
        assert_eq!(quote.bid_levels[0].volume_lots, Some(0));

        fields[10] = "5";
        fields[20] = "3";
        fields[28] = "";
        fields[19] = "";
        let response = format!("v_sh600519=\"{}\";", fields.join("~"));
        let quote = &parse_response(response.as_bytes(), &[symbol(Market::SH, "600519")]).quotes[0];
        assert_eq!(quote.ask_levels[0].price, None);
        assert_eq!(quote.ask_levels[0].volume_lots, Some(3));
        assert_eq!(quote.ask_levels[4].volume_lots, None);
        assert_eq!(quote.order_ratio, None);
    }
}
