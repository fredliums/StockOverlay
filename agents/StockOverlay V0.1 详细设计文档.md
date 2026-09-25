# StockOverlay V0.1 详细设计文档

## 1. 项目定位

StockOverlay 是一个 Windows A 股实时行情桌面悬浮工具。

核心目标：

- 透明悬浮
- 始终置顶
- 任意移动
- 自由调整大小
- 锁定后鼠标穿透
- 全局快捷键锁定 / 解锁
- 全局快捷键显示 / 隐藏
- 自选股实时行情
- 显示字段可配置
- 用户选择的内容必须完整展示
- 配置自动保存
- 最终发布为独立 EXE 或安装包

个人使用，不涉及交易、账户、云端和复杂证券分析。

---

# 2. 技术栈

```text
Tauri 2
React
TypeScript
Rust
Vite
```

职责：

```text
React / TypeScript
├── 行情展示
├── 设置界面
├── 自选股管理
├── 布局
└── 用户交互

Tauri
├── Windows 窗口控制
├── Always On Top
├── 透明窗口
├── Drag / Resize
├── 鼠标穿透
├── 系统托盘
└── 全局快捷键

Rust
├── 行情请求
├── Quote Provider
├── 行情解析
├── 定时刷新
├── 配置持久化
└── 状态管理
```

---

# 3. 总体架构

```text
行情数据源
    │
    ▼
Quote Provider
    │
    ▼
Quote Service
├── 请求
├── 解析
├── 刷新
├── 重试
└── 最新行情缓存
    │
    ▼
Tauri Event
    │
    ▼
React UI
    │
    ▼
Tauri 配置命令 → Rust Config Service
    │
    ▼
config.json
```

原则：

```text
UI 负责展示
Rust 负责行情
Tauri 负责桌面能力
```

---

# 4. 核心模块

```text
Window
Quote
Watchlist
Display
Shortcut
Config
```

### Window

负责：

- 透明无边框窗口
- Always On Top
- Drag
- Resize
- 锁定
- 鼠标穿透
- 显示 / 隐藏
- 恢复窗口位置和大小

### Quote

负责：

- 行情获取
- 数据解析
- 周期刷新
- 网络异常处理
- 最新行情缓存

### Watchlist

负责：

- 添加股票
- 删除股票
- 股票排序
- 股票搜索

### Display

负责：

- 字段显示配置
- 股票布局
- 字体
- 透明度
- 涨跌颜色
- 盘口展示

### Shortcut

负责：

```text
锁定 / 解锁
显示 / 隐藏
```

两个快捷键均允许用户配置。

### Config

负责所有用户配置读取、校验和保存。

---

# 5. 行情数据模型

```ts
interface Quote {
  symbol: string;
  market: 'SH' | 'SZ' | 'BJ';
  name: string;

  price: number | null;
  previousClose: number | null;

  change: number | null;
  changePercent: number | null;

  high: number | null;
  low: number | null;

  turnover: number | null;
  turnoverRate: number | null;

  pe: number | null; // 市盈率 TTM
  volumeRatio: number | null;
  orderRatio: number | null;

  bidLevels: OrderLevel[];
  askLevels: OrderLevel[];

  timestamp: number | null; // 行情源时间，Unix 毫秒
}
```

盘口：

```ts
interface OrderLevel {
  price: number | null;
  volumeLots: number | null;
}
```

约定：

```text
bidLevels[0] = 买一
bidLevels[1] = 买二
...

askLevels[0] = 卖一
askLevels[1] = 卖二
...
```

所有可能缺失的行情字段使用 nullable。

`symbol` 为不带市场前缀的六位代码，`market` 单独保存；两者合成唯一键（如 `SH:600519`）。`turnover` 内部单位为元，`volumeLots` 单位为手，`pe` 明确为市盈率 TTM。行情时间解析失败时 `timestamp = null`，不使用本机接收时间伪装成行情时间。Rust 结构及序列化名称与此 TypeScript 契约保持一致，详见《StockOverlay V0.1 腾讯行情源设计》。

---

# 6. 第一版支持的行情字段

## 基础字段

```text
股票名称
股票代码
最新价格
涨跌额
涨跌幅
成交额

最高
最低
换手率
市盈率
量比
委比
```

默认推荐显示：

```text
股票名称
最新价格
涨跌幅
一档盘口
```

其中一档盘口作为高频查看信息，优先采用紧凑显示方式。

---

# 7. 盘口设计

盘口支持：

```text
不显示
一档
三档
五档
```

## 7.1 一档盘口

一档是默认最适合悬浮窗使用的盘口模式。

**不单独占用盘口区域。**

直接作为一个紧凑字段显示：

```text
卖一价格-挂单量 / 买一价格-挂单量
```

例如：

```text
1418.80-85 / 1418.70-102
```

或者主行情完整示例：

```text
贵州茅台 1418.32 +2.15%  1418.80-85 / 1418.70-102
```

语义固定为：

```text
卖一价格-卖一挂单量 / 买一价格-买一挂单量
```

顺序固定，不根据涨跌改变。

设置界面应明确说明该格式。

如果无盘口数据：

```text
-- / --
```

一侧缺失：

```text
1418.80-85 / --
```

---

# 8. 三档 / 五档盘口

三档和五档信息量明显增大，因此使用独立盘口区域。

三档：

```text
卖三 1419.00-47
卖二 1418.90-62
卖一 1418.80-85
──────────────
买一 1418.70-102
买二 1418.60-79
买三 1418.50-68
```

五档：

```text
卖五 1419.20-38
卖四 1419.10-51
卖三 1419.00-47
卖二 1418.90-62
卖一 1418.80-85
──────────────
买一 1418.70-102
买二 1418.60-79
买三 1418.50-68
买四 1418.40-54
买五 1418.30-33
```

设计规则：

```text
一档
→ 作为普通字段紧凑显示

三档 / 五档
→ 作为独立盘口区域显示
```

不允许窗口 Resize 时自动把：

```text
五档 → 三档 → 一档
```

用户选择几档，就完整显示几档。

---

# 9. 挂单量单位

Provider 内部将腾讯盘口量标准化为手，保存到 `volumeLots`。

UI 层负责格式化。

第一版默认显示单位为手；设置界面说明一档、三档和五档的数量均为手。

显示时支持紧凑格式，例如：

```text
85
102
1.2万
3.6万
```

具体单位转换只允许在统一 formatter 中实现，不能散落在不同 UI 组件里。

避免出现：

```text
一处按股
一处按手
```

导致盘口含义不一致。

---

# 10. Quote Provider

统一接口以《StockOverlay V0.1 腾讯行情源设计》中的 Rust `QuoteProvider` 和 `QuoteBatchResult` 为准。第一版只实现 `TencentQuoteProvider`。Provider 返回逐股成功或失败结果，不能因为一股失败丢弃其他股票的有效报价。

Provider 负责：

```text
股票代码转换
网络请求
数据解析
单位标准化
字段缺失处理
盘口解析
```

React UI 不直接解析第三方行情协议。

---

# 11. 行情能力降级

V0.1 固定使用腾讯行情源，但个别股票或时段仍可能缺少：

```text
PE
委比
换手率
五档盘口
```

因此：

```text
有效数据 → 正常显示
无数据   → --
```

盘口：

```text
整档无数据 → []
部分档位缺字段 → 对应 price / volumeLots 为 null
```

禁止因为某个附加字段缺失导致：

```text
整只股票不显示
整个 Quote 更新失败
```

---

# 12. 行情刷新

默认：

```text
2000 ms
```

支持：

```text
1000
2000
3000
5000 ms
```

刷新模型：

```text
单个 Timer
    │
    ▼
按最多 50 只一批请求所有自选股
    │
    ▼
逐批解析，保留各批成功结果和逐股错误
    │
    ▼
一次合并更新缓存与状态
    │
    ▼
一次更新 UI
```

禁止：

```text
每只股票一个 Timer
```

防止不必要的线程、请求和状态复杂度。

同一时刻最多一轮刷新在途；上一轮未结束时跳过本次 Timer。每轮可以包含多个 HTTP 请求，但只向 UI 发布一次合并后的状态。超时、重试和限流响应处理以行情源文档为准。

---

# 13. 交易时间

交易时间内正常刷新。

非交易时间：

```text
降低到约 60 秒刷新
```

或者由 Provider 判断数据未变化。

第一版不维护完整节假日交易日历。

按北京时间在工作日 09:15～11:30、13:00～15:00 使用用户设定的刷新间隔，其余时段约 60 秒刷新；其中 09:25～09:30 不依据旧行情时间判断连接延迟。不因午休或收盘后行情时间不变就把正常报价标为网络延迟。第一版不维护完整节假日交易日历，节假日仍可能按工作日时间表刷新，属于 V0.1 的已知限制。交易时段判定参考交易所公布的[竞价交易时间](https://www.sse.com.cn/lawandrules/sselawsrules2025/stocks/exchange/c/c_20260424_10816482.shtml)。

---

# 14. 主悬浮窗口

窗口默认：

```text
frameless = true
transparent = true
alwaysOnTop = true
resizable = true
```

两种状态：

```text
editing
locked
```

使用 Tauri 2 原生窗口 API 切换鼠标穿透、可调整大小状态和窗口最小尺寸。若从前端调用这些方法，需在对应窗口的 capability 中授予 `core:window:allow-set-ignore-cursor-events`、`core:window:allow-set-resizable` 和 `core:window:allow-set-min-size` 等权限；实际切换结果失败时不更新持久化锁定状态。

---

# 15. 编辑状态

允许：

```text
拖动
Resize
点击
打开设置
修改股票
调整股票顺序
修改显示内容
锁定窗口
```

可显示轻量 Toolbar。

---

# 16. 锁定状态

进入锁定：

```text
隐藏 Toolbar
禁止 Resize
禁止拖动
开启鼠标穿透
保持 Always On Top
保存 locked 状态
```

效果：

```text
行情只负责显示
鼠标完全操作下方程序
```

---

# 17. 锁定 / 解锁快捷键

默认：

```text
Ctrl + Alt + L
```

行为：

```text
editing ⇄ locked
```

关键安全规则：

**进入鼠标穿透之前，解锁全局快捷键必须已经成功注册。**

如果快捷键注册失败：

```text
保持 editing，不开启鼠标穿透
主窗口保持可见，并在设置和托盘提示失败原因
```

启动时两个快捷键分别校验、注册；两者相同或与已注册快捷键冲突时视为配置无效。锁定快捷键不可用时，即使配置记录了 `locked = true`，本次启动也保持 editing。托盘的“解锁”入口不依赖全局快捷键。

系统托盘始终保留：

```text
锁定 / 解锁
```

作为备用入口。

---

# 18. 显示 / 隐藏快捷键

默认：

```text
Ctrl + Alt + S
```

行为：

```text
visible ⇄ hidden
```

隐藏只隐藏主窗口：

```text
Quote Service 继续运行
托盘继续存在
全局快捷键继续有效
```

再次显示时立即展示最新缓存行情。

---

# 19. 快捷键修改

两个快捷键均支持用户配置。

修改流程：

```text
录入新快捷键
    │
    ▼
检查基本合法性
    │
    ▼
尝试注册新快捷键
    │
    ├── 成功 → 保存新配置 → 注销旧快捷键
    │
    └── 失败 → 保留旧快捷键 → 提示冲突
```

不能先删除旧快捷键再测试新的快捷键。

新快捷键与另一个动作的快捷键相同时拒绝保存；与原值相同时直接保持原注册。注册或保存失败时注销新快捷键、保留旧快捷键和旧配置；注销旧快捷键失败时保持两个入口可用、提示异常并重试清理。注册返回成功后仍以 Windows 实机按键测试和托盘备用入口验证可恢复性。

---

# 20. 系统托盘

托盘菜单：

```text
显示 / 隐藏
锁定 / 解锁
设置
────────
退出
```

关闭主悬浮窗默认：

```text
隐藏到托盘
```

只有：

```text
托盘 → 退出
```

才真正终止程序。

---

# 21. 自选股

结构：

```ts
interface WatchItem {
  symbol: string;
  market: 'SH' | 'SZ' | 'BJ';
}
```

支持：

```text
添加
删除
拖动排序
```

暂不支持：

```text
分组
标签
多个列表
云同步
```

---

# 22. 股票搜索

支持：

```text
代码搜索
名称搜索
```

例如：

```text
600519
贵州茅台
```

结果：

```text
600519 贵州茅台 SH
```

基础证券信息可以内置轻量股票列表。

不需要数据库。

---

# 23. 显示配置

```ts
interface DisplayConfig {
  showName: boolean;
  showCode: boolean;

  showPrice: boolean;
  showChange: boolean;
  showChangePercent: boolean;

  showTurnover: boolean;

  showHigh: boolean;
  showLow: boolean;

  showTurnoverRate: boolean;
  showPE: boolean;
  showVolumeRatio: boolean;
  showOrderRatio: boolean;

  orderBookDepth: 0 | 1 | 3 | 5;
}
```

设置：

```text
☑ 股票名称
☐ 股票代码

☑ 最新价格
☐ 涨跌额
☑ 涨跌幅
☐ 成交额

☐ 最高
☐ 最低
☐ 换手率
☐ 市盈率
☐ 量比
☐ 委比

盘口：
○ 不显示
● 一档
○ 三档
○ 五档
```

一档可作为推荐默认配置。

---

# 24. 字段顺序

第一版不允许用户自由拖动行情字段顺序。

固定逻辑顺序：

```text
名称
代码

最新价格
涨跌额
涨跌幅

成交额

最高
最低

换手率
市盈率
量比
委比

盘口
```

对于一档盘口，它表现为该序列中的最后一个紧凑字段。

这样能避免第一版增加无必要的布局复杂度。

---

# 25. 内容完整性规则

最高优先级：

> 用户选择显示的内容必须完整显示。

这里的“完整”指所有已选字段和盘口档位均可查看，不承诺任意数量的股票在有限屏幕上同时可见。内容超过显示器工作区时，不能裁切字段或降低盘口档位。

Resize 优先级：

```text
1. 保留全部字段
2. 减小合理的空白和间距
3. 从单行调整为多行
4. 调整内部区域排列
5. 限制最小窗口尺寸
6. 达到工作区边界后，使用分页访问剩余内容
```

禁止：

```text
自动隐藏字段
自动隐藏一档盘口
五档自动降级三档
三档自动降级一档
```

编辑状态允许滚动或翻页；锁定后鼠标穿透，无法用鼠标滚动，因此内容超出时自动按完整股票区块轮播，每页停留 8 秒。单只股票区块仍超过一页时，可在字段之间分页并重复股票名称和代码；盘口各档不得裁切或与下一只股票混排。解锁后回到用户上次查看的位置。页面指示器显示当前页和总页数。

---

# 26. 布局策略

## 26.1 默认紧凑布局

这是主要使用模式。

例如：

```text
贵州茅台 1418.32 +2.15%  1418.80-85 / 1418.70-102
宁德时代  287.41 -0.68%   287.50-42 / 287.40-66
```

适合长时间悬浮。

---

## 26.2 字段较多

自动使用多行：

```text
贵州茅台 600519
1418.32 +29.81 +2.15%

高 1421.00   低 1388.20
换 1.32%     PE 24.6
量比 1.41    委比 +12.3%

1418.80-85 / 1418.70-102
```

所有用户字段仍完整保留。

---

## 26.3 三档 / 五档

每只股票使用独立信息块：

```text
贵州茅台 1418.32 +2.15%

卖三 1419.00-47
卖二 1418.90-62
卖一 1418.80-85
────────────
买一 1418.70-102
买二 1418.60-79
买三 1418.50-68
```

下一只股票从下一个完整区块开始。

不同股票盘口禁止交叉排列。

---

# 27. 窗口最小尺寸

不能简单写死一个固定最小宽高。

应根据：

```text
字体大小
已启用字段
盘口档位
当前布局
```

计算容纳一个完整字段或一条盘口行的最小尺寸，兼顾当前显示器工作区，而不是按所有股票的总高度无限增加窗口最小高度。

实际实现允许采用：

```text
若单个字段或盘口行将溢出
→ 在工作区范围内动态提高窗口原生最小宽高
若全部内容超出工作区
→ 按第 25 节分页
```

而不是隐藏内容。

使用 Tauri 窗口 API 设置原生尺寸约束；CSS `min-width` / `min-height` 只约束页面内部，不能阻止用户将原生窗口缩得更小。尺寸使用逻辑像素，并按显示器缩放比例处理。显示器拔插或分辨率变化时重新计算工作区和窗口位置，确保至少有可操作区域留在可见屏幕内。

---

# 28. 外观

第一版支持：

```text
字体大小
背景透明度
文字透明度
涨跌颜色模式
```

默认：

```text
上涨 = 红
下跌 = 绿
平盘 = 默认颜色
```

允许切换：

```text
红涨绿跌
绿涨红跌
```

盘口价格颜色可以保持中性，避免界面颜色过度复杂。

---

# 29. 配置文件

使用：

```text
%APPDATA%\StockOverlay\config.json
```

示例：

```json
{
  "schemaVersion": 1,
  "window": {
    "x": 1450,
    "y": 80,
    "width": 560,
    "height": 180,
    "locked": true,
    "alwaysOnTop": true
  },

  "display": {
    "fontSize": 18,
    "backgroundOpacity": 0,
    "textOpacity": 1,

    "showName": true,
    "showCode": false,
    "showPrice": true,
    "showChange": false,
    "showChangePercent": true,
    "showTurnover": false,

    "showHigh": false,
    "showLow": false,
    "showTurnoverRate": false,
    "showPE": false,
    "showVolumeRatio": false,
    "showOrderRatio": false,

    "orderBookDepth": 1,

    "redForRise": true
  },

  "shortcuts": {
    "toggleLock": "Ctrl+Alt+L",
    "toggleVisibility": "Ctrl+Alt+S"
  },

  "stocks": [
    "sh600519",
    "sz300750"
  ],

  "refreshInterval": 2000
}
```

`schemaVersion` 用于后续迁移。窗口坐标和尺寸以逻辑像素保存；读取时校验数值范围、显示器工作区和快捷键合法性。若原显示器不存在或窗口完全落在屏幕外，将窗口放回主显示器可见区域，并保留可行的尺寸。

---

# 30. 配置保存策略

立即保存：

```text
增加 / 删除股票
调整股票顺序
修改显示字段
修改盘口档位
修改快捷键
修改刷新频率
修改外观
锁定 / 解锁
```

窗口位置和尺寸：

```text
Drag / Resize 结束后保存
```

不要在连续 Mouse Move / Resize Event 中反复写磁盘。

配置修改通过 Rust 单一写入队列顺序提交，避免窗口事件与设置命令互相覆盖。每次保存先写同目录临时文件并刷新，再替换 `config.json`；失败时保留旧配置并在设置或托盘提示。读取损坏或版本不兼容的文件时先保留原文件副本，再使用默认配置启动，不静默覆盖用户原文件。

---

# 31. 前端结构

```text
src/
├── App.tsx
│
├── overlay/
│   ├── Overlay.tsx
│   ├── StockList.tsx
│   ├── StockCard.tsx
│   ├── QuoteFields.tsx
│   ├── CompactLevel1.tsx
│   ├── OrderBook.tsx
│   └── OverlayToolbar.tsx
│
├── settings/
│   ├── SettingsWindow.tsx
│   ├── WatchlistSettings.tsx
│   ├── DisplaySettings.tsx
│   ├── AppearanceSettings.tsx
│   ├── ShortcutSettings.tsx
│   └── QuoteSettings.tsx
│
├── stores/
│   └── appStore.ts
│
└── types/
    ├── quote.ts
    └── config.ts
```

`CompactLevel1.tsx` 专门负责：

```text
卖一价格-挂单量 / 买一价格-挂单量
```

避免和三 / 五档组件混在一起。

---

# 32. Rust 结构

```text
src-tauri/src/
├── main.rs
├── quote.rs
├── provider.rs
├── config.rs
├── shortcut.rs
└── window.rs
```

不增加 Repository、DI 等复杂架构。

---

# 33. 前后端通信

Rust → React：

```text
quote:update
```

Payload 为当前完整缓存快照：

```ts
interface QuoteStatus {
  state: 'loading' | 'normal' | 'delayed' | 'disconnected' | 'invalid';
  lastSuccessAt: number | null; // 本机成功接收时间，Unix 毫秒
  message?: string;
}

interface QuoteSnapshot {
  revision: number; // 本次进程内单调递增
  quotes: Quote[];
  statuses: Record<string, QuoteStatus>; // 键为 SH:600519 等
}
```

前端先订阅 `quote:update`，再调用 `get_quote_snapshot` 取得初始快照；只应用 `revision` 更大的快照，避免首次事件与命令返回竞争。重新显示窗口或重新创建设置窗口时也读取快照。自选股顺序始终按配置列表呈现，不依赖批量响应顺序。

配置命令主要包括：

```text
load_config
save_config

add_stock
remove_stock
reorder_stocks

update_display_config
update_shortcuts

set_lock_state
get_quote_snapshot
```

---

# 34. 网络异常

行情请求失败：

```text
继续显示最后一次有效数据
```

逐股维护最后有效 Quote、`lastSuccessAt` 和连续失败次数。首次未获得数据为 `loading`；本轮成功且行情时间正常为 `normal`；一次失败后保留旧数据并标记 `delayed`；连续两轮失败为 `disconnected`；确认无效代码为 `invalid`，不生成伪 Quote。成功后清零失败计数。非交易时段只因行情时间不变不标记延迟；正常交易时段若行情时间超过 60 秒未更新，或时间字段无法解析，也显示行情时间并标记 `delayed`，避免把旧报价当作实时数据。无效股票和网络错误均不得阻断其他股票更新。

不使用连续弹窗。

网络恢复后自动恢复请求。

---

# 35. 格式化规则

最新价：

```text
1418.32
```

涨跌：

```text
+29.81
-3.26
```

涨跌幅：

```text
+2.15%
-0.68%
```

成交额：

```text
8623万
12.6亿
```

换手：

```text
2.36%
```

PE：

```text
24.61
```

量比：

```text
1.38
```

委比：

```text
+18.26%
```

一档：

```text
1418.80-85 / 1418.70-102
```

缺失：

```text
--
```

所有格式化集中放在统一 formatter 中。

---

# 36. 启动顺序

```text
启动 EXE
    │
    ▼
读取并校验配置
    │
    ▼
创建系统托盘
    │
    ▼
校验并尝试注册两个全局快捷键
    │
    ▼
创建悬浮窗口
    │
    ▼
按当前显示器工作区恢复窗口位置 / 尺寸
    │
    ▼
根据快捷键结果恢复 editing / locked 状态
    │
    ▼
异步获取第一次行情，并发布首次快照
```

恢复鼠标穿透不等待网络请求成功。若锁定快捷键注册失败、托盘创建失败或窗口初始化失败，不能进入 locked；保持可操作的主窗口并提示错误。网络超时不阻塞窗口和托盘显示。

---

# 37. 退出顺序

```text
停止 Quote Timer
注销全局快捷键
保存最终配置
关闭窗口
销毁托盘
退出进程
```

---

# 38. V0.1 发布

提供：

```text
StockOverlay.exe
```

以及：

```text
StockOverlay-Setup.exe
```

个人日常优先使用 Portable EXE。

安装包用于需要完整 Windows 安装 / 卸载体验的场景。

“Portable”仅表示无需安装器，配置仍写入 `%APPDATA%\StockOverlay\config.json`。发布时明确支持的 Windows 版本与架构，并在干净系统上验证 WebView2 Runtime 前提、单独 EXE 启动、NSIS 安装与卸载。安装包负责按选定模式检查或安装 WebView2；单独 EXE 不保证在缺少该运行时的机器上直接运行。

---

# 39. V0.1 验收标准

## 窗口

- 透明
- 无边框
- 始终置顶
- 可拖动
- 可 Resize
- 位置和尺寸持久化

## 锁定

- 快捷键可锁定
- 锁定后鼠标穿透
- 快捷键可解锁
- 托盘可解锁

## 快捷键

- 锁定 / 解锁可配置
- 显示 / 隐藏可配置
- 快捷键冲突不会破坏现有快捷键

## 自选股

- 添加
- 删除
- 排序
- 持久化

## 行情

支持：

```text
名称
代码
最新价
涨跌额
涨跌幅
成交额
最高
最低
换手率
市盈率
量比
委比
```

## 盘口

支持：

```text
关闭
一档
三档
五档
```

其中：

```text
一档：
卖一价格-挂单量 / 买一价格-挂单量
```

作为紧凑字段显示。

## 显示

- 用户启用字段全部显示
- Resize 不自动隐藏字段
- 三 / 五档盘口不会自动降级
- 多股票盘口不会相互混排
- 内容超过工作区时，编辑状态可访问全部内容，锁定状态自动分页且无字段裁切

## 稳定性

- 网络错误不退出
- 单个字段解析失败不影响整只股票
- 网络恢复自动继续
- 长时间运行资源占用稳定
- 配置损坏可以回退到默认配置
- 部分批次失败不阻断成功批次更新，旧报价有逐股状态提示
- 快捷键注册失败时不进入鼠标穿透，托盘仍能恢复控制
- 多显示器拔插后窗口仍可操作

---

# 40. 明确不做

```text
K线
分时图

MACD
KDJ
RSI

完整 Level-2

逐笔成交

交易
证券账户

资讯
公告
财报

AI 分析
选股器

登录
云同步
数据库
服务器
```

---

# 41. 最终设计原则

```text
功能不多
但实现完整

一档优先紧凑展示
三档 / 五档独立展示

用户配置优先

用户选什么
就完整显示什么

布局适配内容
不能删除内容适配布局

锁定后零干扰

快捷键和托盘始终能恢复控制

行情 Provider 与 UI 解耦

配置简单
长期运行稳定
```
