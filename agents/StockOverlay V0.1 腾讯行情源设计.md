# StockOverlay V0.1 腾讯行情源设计

## 1. 数据源

V0.1 只使用腾讯实时行情：

```text
https://qt.gtimg.cn/q=
```

请求格式：

```text
单股：
https://qt.gtimg.cn/q=sh600519

批量：
https://qt.gtimg.cn/q=sh600519,sz000001,bj920021
```

市场前缀：

```text
上海：sh
深圳：sz
北京：bj
```

北交所使用当前有效的 `920xxx` 证券代码。北交所已于 2025 年 10 月 9 日将存量股票切换到 920 号段；股票搜索和自选股校验以交易所当前代码及其新旧代码对照表为准，旧代码仅可用于搜索时提示对应的新代码，不保存为行情订阅代码。参考：[北交所切换通知](https://www.bse.cn/important_news/200026735.html)、[新旧代码对照表](https://www.bse.cn/service/code_mapping.html)。

---

## 2. 请求验证

| 请求 | 结果 |
|---|---|
| `sh600519` | ✅ 已直接验证，返回完整行情及五档 |
| `sz000001` | ✅ 已直接验证，返回完整行情及五档 |
| 多股票逗号批量 | ✅ 当前实现及近期实测资料确认 |
| `bj920xxx` | ✅ 2026 年近期实测确认 |
| 无效代码 | ✅ 返回 `v_pv_none_match="1"` |
| HTTPS | ✅ |
| API Key / Cookie | 不需要 |

沪深单股当前实际返回仍是 `v_xxx="...~...";` 格式；批量请求使用逗号拼接也是目前维护中的腾讯行情客户端实现方式。

无效证券不能仅靠字段长度判断，必须识别：

```text
v_pv_none_match="1"
```

近期 stock-sdk 仍专门修复了这一腾讯接口行为。

> 注：本环境能直接验证沪深单股；批量 URL 因当前网页工具对逗号 URL 的限制无法直接发出，北交所 URL也被该工具拦截。因此这两项使用 2026 年近期实际实现和实测资料交叉确认，而不是声称本轮直接请求成功。

---

## 3. HTTP 策略

```text
GET
HTTPS
Timeout: 5s
User-Agent: StockOverlay/0.1
```

`Referer` 不作为必需条件。

单次批量：

```text
MAX_BATCH_SIZE = 50
```

超过后分批。

50 只是应用自身的单批上限，不代表腾讯接口承诺的配额。V0.1 不承诺该接口的服务等级或持续可用性；遇到限流或拒绝访问时按第 12 节降速处理，不能持续以 1 秒间隔重试。

刷新时：

```text
一个 Timer
→ 批量请求全部自选股
→ 解析
→ 一次更新 UI
```

禁止每只股票单独创建 Timer。

---

## 4. 返回解析

腾讯响应使用 GBK 编码，字段以 `~` 分隔。当前实现和近期字段实测均如此。

流程：

```text
HTTP bytes
    ↓
GBK decode
    ↓
按每个 v_xxx="..." 拆记录
    ↓
payload.split('~')
    ↓
Quote
```

批量响应必须逐条解析，不能对整个响应直接 `split('~')`。

---

## 5. V0.1 字段映射

| 下标 | 含义 |
|---:|---|
| 1 | 名称 |
| 2 | 代码 |
| 3 | 最新价 |
| 4 | 昨收 |
| 9/10 | 买一价 / 买一量 |
| 11/12 | 买二价 / 买二量 |
| 13/14 | 买三价 / 买三量 |
| 15/16 | 买四价 / 买四量 |
| 17/18 | 买五价 / 买五量 |
| 19/20 | 卖一价 / 卖一量 |
| 21~28 | 卖二～卖五 |
| 30 | 行情时间 |
| 31 | 涨跌额 |
| 32 | 涨跌幅 |
| 33 | 最高 |
| 34 | 最低 |
| 38 | 换手率 |
| 39 | 市盈率 TTM |
| 49 | 量比 |
| 57 | 成交额（万元，精度更高） |

这些核心位置与 2026 年近期实测映射一致。

成交额：

```text
优先 fields[57]
fallback fields[37]
```

内部统一转换成“元”：

```text
amountYuan = amountWan × 10000
```

字段 30 是行情源的北京时间 `YYYYMMDDHHmmss`，解析后转为 Unix 毫秒写入 `timestamp`；无法解析时为 `None`，不能用请求完成时间冒充。服务层另记本机成功接收时间 `lastSuccessAt`。`turnoverRate`、`changePercent`、`orderRatio` 均以百分数值保存，例如 `2.15` 表示 `2.15%`，UI 统一追加 `%`。

---

## 6. 五档盘口

内部结构：

```ts
interface OrderLevel {
  price: number | null;
  volumeLots: number | null;
}
```

约定：

```text
bidLevels[0] = 买一
...
bidLevels[4] = 买五

askLevels[0] = 卖一
...
askLevels[4] = 卖五
```

腾讯盘口挂单量单位为**手**。当前接口的买卖五档字段位置和单位与历史及近期实现保持一致。内部和 UI 契约统一使用 `volumeLots`；任一档的价格或数量缺失时，该档对应值为 `null`，不把缺失值转换为 0。

---

## 7. 一档显示

一档作为默认紧凑字段：

```text
卖一价格-挂单量 / 买一价格-挂单量
```

例如：

```text
1237.05-1 / 1237.00-13
```

完整行情：

```text
贵州茅台 1237.00 -1.14%  1237.05-1 / 1237.00-13
```

挂单量默认单位：

```text
手
```

界面不重复显示单位。

---

## 8. 三档 / 五档

三档、五档使用独立盘口区域：

```text
卖三 1237.70-1
卖二 1237.50-1
卖一 1237.05-1
────────────
买一 1237.00-13
买二 1236.95-4
买三 1236.85-1
```

Provider 内部始终保持：

```text
askLevels[0] = 卖一
bidLevels[0] = 买一
```

卖盘倒序只是 UI 行为。

---

## 9. 委比

委比不依赖腾讯扩展字段，直接由五档计算：

```text
委买 = 买1 + 买2 + 买3 + 买4 + 买5
委卖 = 卖1 + 卖2 + 卖3 + 卖4 + 卖5

委比 =
(委买 - 委卖) / (委买 + 委卖) × 100%
```

本轮用腾讯实际返回进行了两组交叉验证：

```text
贵州茅台：
计算值 ≈ 61.54%

平安银行：
计算值 ≈ 60.84%
```

均与接口当前扩展字段返回值一致。

仅当买卖各五档数量均有效且总挂单量大于 0 时计算委比。任一档数量缺失或总挂单量为 0：

```text
orderRatio = null
```

---

## 10. 数据模型

```rust
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Quote {
    symbol: String, // 六位代码，不含 sh/sz/bj 前缀
    market: Market,
    name: String,

    price: Option<f64>,
    previous_close: Option<f64>,

    change: Option<f64>,
    change_percent: Option<f64>,

    high: Option<f64>,
    low: Option<f64>,

    turnover: Option<f64>,
    turnover_rate: Option<f64>,
    pe: Option<f64>, // 市盈率 TTM
    volume_ratio: Option<f64>,
    order_ratio: Option<f64>,

    bid_levels: Vec<OrderLevel>,
    ask_levels: Vec<OrderLevel>,

    timestamp: Option<i64>, // 行情源时间，Unix 毫秒
}
```

```rust
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OrderLevel {
    price: Option<f64>,
    volume_lots: Option<u64>,
}
```

市场：

```rust
#[derive(serde::Serialize)]
enum Market {
    SH,
    SZ,
    BJ,
}
```

`Symbol` 是 Provider 入参，包含六位 `code` 和 `market`；输出 Quote 的 `symbol` 与 `market` 分开。Rust 序列化后的字段名、nullable 规则与主文档第 5 节 TypeScript 模型完全一致，例如 `bidLevels`、`volumeLots`、`previousClose`。`turnover` 单位为元，`volumeLots` 为手。唯一键使用 `SH:600519` 等形式。

---

## 11. Provider

```rust
trait QuoteProvider {
    async fn fetch_quotes(
        &self,
        symbols: &[Symbol],
    ) -> Result<QuoteBatchResult, QuoteError>;
}
```

```rust
struct Symbol {
    code: String, // 六位代码
    market: Market,
}

struct QuoteBatchResult {
    quotes: Vec<Quote>,
    failures: Vec<QuoteFailure>,
}

struct QuoteFailure {
    symbol: Symbol,
    kind: QuoteFailureKind, // InvalidSymbol / Parse / MissingRecord / Network / RateLimited
}
```

`fetch_quotes` 对每只请求股票恰好返回一条成功 Quote 或一条失败记录。`Result::Err` 仅用于客户端无法初始化等整轮无法执行的错误；普通 HTTP、解析和无效代码错误进入逐股 `failures`。空自选股直接返回空结果，不发 HTTP。应用按配置顺序合并成功 Quote，失败股票保留上一帧数据与逐股状态；所有批次结束后只发布一次完整快照。某一批失败不影响已成功批次。主文档的 `QuoteSnapshot` 是服务层缓存及状态协议，不是 Provider 原始返回值。

V0.1：

```text
QuoteProvider
      ▲
      │
TencentQuoteProvider
```

职责：

```text
构造 URL
HTTP 请求
GBK 解码
批量记录拆分
字段解析
数据标准化
委比计算
```

UI 不接触腾讯原始字段。

---

## 12. 异常处理

单个字段缺失：

```text
→ Option::None
→ UI 显示 --
```

单个股票解析失败：

```text
→ 记录该股票错误
→ 其他股票继续更新
```

网络失败：

```text
超时或 5xx → 间隔至少 500 ms 后对失败批次重试一次
仍失败 → 该批次逐股保留上一帧行情并记录失败
首次失败的股票标记 delayed，连续两轮失败标记 disconnected
下一周期继续请求；成功后清零连续失败次数
```

禁止因为一次请求失败清空已有行情。

HTTP 403 / 429 不立即重试；整轮刷新进入至少 60 秒冷却期，并显示限流或拒绝访问原因。冷却期结束后再尝试，不能按用户设置的 1 秒间隔持续打同一接口。每批有 5 秒超时，重试仍属于同一轮刷新；轮次未结束时跳过 Timer，不产生并发请求。

无效代码标记 `invalid`，不再周期请求该代码，直到用户修改自选股；解析失败或响应缺少某个已请求代码则按该股票一次失败处理。网络、解析和缺记录错误不得覆盖其他股票的成功数据。逐股状态和首次快照按主文档第 33～34 节发布。

---

## 13. 必要解析校验

不能直接裸访问：

```rust
fields[57]
```

必须安全读取。

同时验证：

```text
响应 key 对应请求代码
名称非空
代码非空
存在核心价格字段
不是 v_pv_none_match
```

尤其北交所历史旧代码可能返回 HTTP 200 但实际是停止更新的旧行情，因此自选股只保存当前有效证券代码。交易时段若行情源时间比本机北京时间落后超过 60 秒，保留数据但标记 `delayed` 并展示行情时间；非交易时段不因时间不变标记延迟。时间字段无法解析时也不能把报价标为实时。这个时间判断用于提示，不代替新旧代码对照表校验。

---

## 14. 刷新策略

默认：

```text
2000 ms
```

用户可选：

```text
1000
2000
3000
5000 ms
```

必须防止请求重入：

```text
上一次 refresh 未结束
→ 本次 Timer 跳过
```

同一时间最多存在一次行情刷新。

一次逻辑刷新可包含多个最多 50 只的 HTTP 批次。待全部批次结束后合并成功和失败结果，缓存 `revision` 加一并发布一次 `quote:update`；即使全部失败，也发布状态变化。窗口订阅事件后再调用 `get_quote_snapshot`，只接受更高 `revision`，防止首次行情事件丢失。

---

## 15. V0.1 必测用例

```text
sh600519      → 沪市正常行情
sz000001      → 深市正常行情
bj920xxx      → 北交所正常行情

sh600519,sz000001
              → 批量返回两条行情

非法代码      → v_pv_none_match，不生成伪 Quote

空 PE         → --
盘口缺失      → --

五档          → 买卖档位正确
委比          → 与五档计算结果一致

网络断开      → 保留旧数据
网络恢复      → 自动恢复刷新
一批成功一批失败 → 成功股票更新，失败股票保留旧报价并标状态
HTTP 429/403   → 不立即重试，进入冷却期
请求超过刷新间隔 → 后续 Timer 跳过，不产生重叠请求
字段 30 解析    → 北京时间转换为 Unix 毫秒
交易时段旧时间  → 显示 delayed 与行情时间
序列化字段      → 与主文档 Quote/OrderLevel 名称及 null 规则一致
```

---

# 16. 最终结论

V0.1 行情层固定为：

```text
qt.gtimg.cn
    ↓
TencentQuoteProvider
    ↓
QuoteBatchResult
    ↓
QuoteService 缓存与逐股状态
    ↓
QuoteSnapshot → React UI
```

支持：

```text
名称 / 代码
最新价
涨跌额 / 涨跌幅
成交额
最高 / 最低
换手率
PE TTM
量比
委比
一 / 三 / 五档盘口
```

V0.1 **不增加第二行情源，不做自动数据源切换**。

腾讯不可用时：

```text
保留最后有效行情
+
标记连接状态
+
持续自动重试
```

东方财富只作为以后备用 Provider 候选。
