use reqwest::blocking::Client;
use std::{error::Error, time::Duration};

fn main() -> Result<(), Box<dyn Error>> {
    let client = Client::builder()
        .user_agent("StockOverlay/0.1")
        .timeout(Duration::from_secs(5))
        .build()?;
    for (query, expected_keys) in [
        ("sh600519", &["v_sh600519"][..]),
        ("sz000001", &["v_sz000001"][..]),
        ("bj920021", &["v_bj920021"][..]),
        ("sh600519,sz000001", &["v_sh600519", "v_sz000001"][..]),
        ("sh000000", &["v_pv_none_match"][..]),
    ] {
        let response = client
            .get(format!("https://qt.gtimg.cn/q={query}"))
            .send()?
            .error_for_status()?;
        let body = response.bytes()?;
        for key in expected_keys {
            if !body
                .windows(key.len())
                .any(|window| window == key.as_bytes())
            {
                return Err(format!("{query}: missing response key {key}").into());
            }
        }
        println!(
            "{query}: HTTP 200, {} bytes, {} expected keys",
            body.len(),
            expected_keys.len()
        );
    }
    Ok(())
}
