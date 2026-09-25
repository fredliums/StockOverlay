import type { AppConfig } from '../types/config';
import type { QuoteSnapshot } from '../types/quote';
import type { StockEntry } from '../types/stock';
import { StockCard } from './StockCard';
import type { StockPart } from '../pagination';

export function StockList({
  config,
  snapshot,
  watchlist,
  parts,
}: {
  config: AppConfig;
  snapshot: QuoteSnapshot | null;
  watchlist: StockEntry[];
  parts?: StockPart[];
}) {
  if (config.stocks.length === 0) {
    return <div className="stock-list__empty">尚无自选股，请在设置中添加。</div>;
  }
  const quotes = new Map(snapshot?.quotes.map((quote) => [`${quote.market}:${quote.symbol}`, quote]));
  const entries = new Map(watchlist.map((entry) => [`${entry.market}:${entry.code}`, entry]));

  const rendered = parts ?? config.stocks.map((_, stockIndex) => ({ stockIndex, start: 0, end: Infinity }));
  return <div className="stock-list">
    {rendered.map(({ stockIndex, start, end }) => {
      const code = config.stocks[stockIndex];
      const key = `${code.slice(0, 2).toUpperCase()}:${code.slice(2)}`;
      return <StockCard
        key={`${key}:${start}:${end}`}
        code={code}
        entry={entries.get(key) ?? null}
        quote={quotes.get(key) ?? null}
        status={snapshot?.statuses[key] ?? null}
        display={config.display}
        range={{ start, end }}
      />;
    })}
  </div>;
}
