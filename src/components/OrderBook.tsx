import { formatLevel, formatLevel1 } from '../formatter';
import type { OrderLevel, Quote } from '../types/quote';

export function CompactLevel1({ quote, atomIndex }: { quote: Quote | null; atomIndex: number }) {
  return <span className="quote-field quote-field--level1" data-atom-index={atomIndex} aria-label="卖一 / 买一">
    {formatLevel1(quote?.askLevels[0], quote?.bidLevels[0])}
  </span>;
}

function levelRow(side: '卖' | '买', index: number, levels: OrderLevel[], atomIndex: number) {
  return <div className="order-book__row" key={`${side}${index}`} data-atom-index={atomIndex}>
    <span className="order-book__label">{side}{index + 1}</span>
    <span>{formatLevel(levels[index])}</span>
  </div>;
}

export function OrderBook({
  quote,
  depth,
  atomOffset,
  range,
}: {
  quote: Quote | null;
  depth: 3 | 5;
  atomOffset: number;
  range?: { start: number; end: number };
}) {
  const asks = quote?.askLevels ?? [];
  const bids = quote?.bidLevels ?? [];
  const askIndices = Array.from({ length: depth }, (_, index) => index)
    .filter((index) => !range || (atomOffset + index >= range.start && atomOffset + index < range.end));
  const bidIndices = Array.from({ length: depth }, (_, index) => index)
    .filter((index) => !range || (atomOffset + depth + index >= range.start && atomOffset + depth + index < range.end));
  if (askIndices.length === 0 && bidIndices.length === 0) return null;
  return <div className="order-book" aria-label={`${depth}档盘口`}>
    {askIndices.length > 0 && <div className="order-book__side" aria-label="卖盘">
      {askIndices.map((index) => levelRow('卖', depth - index - 1, asks, atomOffset + index))}
    </div>}
    {bidIndices.length > 0 && <div className={`order-book__side${askIndices.length ? ' order-book__side--bid' : ''}`} aria-label="买盘">
      {bidIndices.map((index) => levelRow('买', index, bids, atomOffset + depth + index))}
    </div>}
  </div>;
}
