import type { DisplayConfig } from '../types/config';
import type { Quote, QuoteStatus } from '../types/quote';
import type { StockEntry } from '../types/stock';
import { QuoteFields } from './QuoteFields';

function statusText(status: QuoteStatus | null, quote: Quote | null): string | null {
  if (!status || status.state === 'loading') return '加载中';
  if (status.state === 'normal') return null;
  if (status.state === 'invalid') return '股票代码无效';
  const time = quote?.timestamp === null || quote?.timestamp === undefined
    ? null
    : new Date(quote.timestamp).toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai', hour12: false });
  const label = status.state === 'disconnected' ? '连接中断' : '行情延迟';
  return time ? `${label} · 行情时间 ${time}` : label;
}

export function StockCard({
  code,
  entry,
  quote,
  status,
  display,
}: {
  code: string;
  entry: StockEntry | null;
  quote: Quote | null;
  status: QuoteStatus | null;
  display: DisplayConfig;
}) {
  const market = code.slice(0, 2).toUpperCase();
  const symbol = code.slice(2);
  const name = quote?.name || entry?.name || `${market} ${symbol}`;
  const showIdentity = display.showName || display.showCode;
  const message = statusText(status, quote);
  return <section className="stock-card" aria-label={`${name} ${market} ${symbol}`}>
    <div className="stock-card__identity">
      {(display.showName || !showIdentity) && <strong>{name}</strong>}
      {display.showCode && <span className="stock-card__code">{market} {symbol}</span>}
    </div>
    <QuoteFields quote={quote} display={display} />
    {message && <div className={`stock-card__status stock-card__status--${status?.state ?? 'loading'}`}>{message}</div>}
  </section>;
}
