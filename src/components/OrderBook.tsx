import { formatLevel, formatLevel1 } from '../formatter';
import type { OrderLevel, Quote } from '../types/quote';

export function CompactLevel1({ quote }: { quote: Quote | null }) {
  return <span className="quote-field quote-field--level1" aria-label="卖一 / 买一">
    {formatLevel1(quote?.askLevels[0], quote?.bidLevels[0])}
  </span>;
}

function levelRow(side: '卖' | '买', index: number, levels: OrderLevel[]) {
  return <div className="order-book__row" key={`${side}${index}`}>
    <span className="order-book__label">{side}{index + 1}</span>
    <span>{formatLevel(levels[index])}</span>
  </div>;
}

export function OrderBook({ quote, depth }: { quote: Quote | null; depth: 3 | 5 }) {
  const asks = quote?.askLevels ?? [];
  const bids = quote?.bidLevels ?? [];
  return <div className="order-book" aria-label={`${depth}档盘口`}>
    <div className="order-book__side" aria-label="卖盘">
      {Array.from({ length: depth }, (_, index) => levelRow('卖', depth - index - 1, asks))}
    </div>
    <div className="order-book__side order-book__side--bid" aria-label="买盘">
      {Array.from({ length: depth }, (_, index) => levelRow('买', index, bids))}
    </div>
  </div>;
}
