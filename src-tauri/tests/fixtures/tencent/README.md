# Tencent quote response fixtures

Captured on 2026-09-25 at approximately 14:09 China Standard Time from
`https://qt.gtimg.cn/q=` with HTTPS GET and `User-Agent: StockOverlay/0.1`.
The requests sent no API key, Cookie, or Referer. Each request returned HTTP 200.
Files preserve the original response bytes in GBK; decoding them as strict UTF-8 fails.

| File | Query | Records | Bytes |
| --- | --- | ---: | ---: |
| `sh600519.gbk` | `sh600519` | 1 | 550 |
| `sz000001.gbk` | `sz000001` | 1 | 524 |
| `bj920021.gbk` | `bj920021` | 1 | 472 |
| `batch.gbk` | `sh600519,sz000001` | 2 | 1074 |
| `invalid.gbk` | `sh000000` | `v_pv_none_match` | 21 |

The valid responses have 87 or 88 tilde-separated fields. The SH, SZ, and BJ
responses confirm the documented positions for name (1), code (2), price (3),
previous close (4), the five bid and ask price/lot pairs (9–28), quote time
(30), change (31), change percentage (32), high (33), low (34), turnover
fallback (37), turnover rate (38), trailing P/E (39), volume ratio (49), and
precise turnover in ten-thousand yuan (57). The batch response contains both
requested records. The invalid request returned `v_pv_none_match="1";`.

At capture time, the source quote timestamps were dated 2026-09-24, one day
before the request. The fixtures therefore validate response structure and
parsing, not real-time freshness. Freshness is evaluated separately by the
quote service. Replay these exact bytes in parser tests; do not depend on live
market data in automated tests.
