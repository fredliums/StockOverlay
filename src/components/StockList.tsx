import type { AppConfig } from '../types/config';
import type { QuoteSnapshot } from '../types/quote';
import type { StockEntry } from '../types/stock';
import { StockCard } from './StockCard';

export function StockList({
  config,
  snapshot,
  watchlist,
}: {
  config: AppConfig;
  snapshot: QuoteSnapshot | null;
  watchlist: StockEntry[];
}) {
  if (config.stocks.length === 0) {
    return <div className="stock-list__empty">尚无自选股，请在设置中添加。</div>;
  }
  const quotes = new Map(snapshot?.quotes.map((quote) => [`${quote.market}:${quote.symbol}`, quote]));
  const entries = new Map(watchlist.map((entry) => [`${entry.market}:${entry.code}`, entry]));

  return <div className="stock-list">
    {config.stocks.map((code) => {
      const key = `${code.slice(0, 2).toUpperCase()}:${code.slice(2)}`;
      return <StockCard
        key={key}
        code={code}
        entry={entries.get(key) ?? null}
        quote={quotes.get(key) ?? null}
        status={snapshot?.statuses[key] ?? null}
        display={config.display}
      />;
    })}
  </div>;
}
