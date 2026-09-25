# Bundled stock search index

`stocks.json` was generated on 2026-09-25 with `py -3.11 scripts/update_stock_index.py`.
It contains current subscription codes and names for the Shanghai, Shenzhen,
and Beijing markets. Beijing entries may also carry a former code for search.
The application searches this local file and does not use a database.

Sources used by the generator:

- [SSE stock list](https://www.sse.com.cn/assortment/stock/list/share/): main board and STAR Market via the exchange's public list query.
- [SZSE A-share list](https://www.szse.cn/market/product/stock/list/): official spreadsheet export.
- [BSE old/new code mapping](https://www.bseinfo.net/service/code_mapping.html): legacy search aliases; `bseinfo.net` serves the exchange's published mapping page.
- [Tencent quote endpoint](https://qt.gtimg.cn/q=bj920021): probes the 920000–920999 range to discover currently quoted Beijing securities and names. The old/new mapping is attached only to matching current 920 codes.

To refresh, run the generator, inspect the `stocks.json` diff and its reported
counts, then run `cargo test --manifest-path src-tauri/Cargo.toml --lib`. The
generator rejects incomplete lists, duplicate market/code pairs, and missing
known examples before replacing the file. Commit the refreshed index with the
source changes. Stock listings can change after this snapshot; adding a stock
must still check the current quote response before saving it to the watchlist.
