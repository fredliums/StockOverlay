import {
  formatChange,
  formatChangePercent,
  formatOrderRatio,
  formatPE,
  formatPrice,
  formatTurnover,
  formatTurnoverRate,
  formatVolumeRatio,
} from '../formatter';
import type { DisplayConfig } from '../types/config';
import type { Quote } from '../types/quote';
import { CompactLevel1 } from './OrderBook';

type Field = { key: string; label: string; value: string };

export function quoteFields(quote: Quote | null, display: DisplayConfig): Field[] {
  return [
    ...(display.showPrice ? [{ key: 'price', label: '', value: formatPrice(quote?.price) }] : []),
    ...(display.showChange ? [{ key: 'change', label: '', value: formatChange(quote?.change) }] : []),
    ...(display.showChangePercent ? [{ key: 'changePercent', label: '', value: formatChangePercent(quote?.changePercent) }] : []),
    ...(display.showTurnover ? [{ key: 'turnover', label: '额', value: formatTurnover(quote?.turnover) }] : []),
    ...(display.showHigh ? [{ key: 'high', label: '高', value: formatPrice(quote?.high) }] : []),
    ...(display.showLow ? [{ key: 'low', label: '低', value: formatPrice(quote?.low) }] : []),
    ...(display.showTurnoverRate ? [{ key: 'turnoverRate', label: '换', value: formatTurnoverRate(quote?.turnoverRate) }] : []),
    ...(display.showPE ? [{ key: 'pe', label: 'PE', value: formatPE(quote?.pe) }] : []),
    ...(display.showVolumeRatio ? [{ key: 'volumeRatio', label: '量', value: formatVolumeRatio(quote?.volumeRatio) }] : []),
    ...(display.showOrderRatio ? [{ key: 'orderRatio', label: '委', value: formatOrderRatio(quote?.orderRatio) }] : []),
    ...(display.orderBookDepth === 1 ? [{ key: 'level1', label: '', value: '' }] : []),
  ];
}

export function QuoteFields({
  quote,
  display,
  range,
}: {
  quote: Quote | null;
  display: DisplayConfig;
  range?: { start: number; end: number };
}) {
  const fields = quoteFields(quote, display);
  const visible = fields.map((field, index) => ({ field, index }))
    .filter(({ index }) => !range || (index >= range.start && index < range.end));
  if (visible.length === 0) return null;
  return <div className="quote-fields">
    {visible.map(({ field, index }) => field.key === 'level1'
      ? <CompactLevel1 key={field.key} quote={quote} atomIndex={index} />
      : <span className={`quote-field quote-field--${field.key}`} data-atom-index={index} key={field.key}>
      {field.label && <span className="quote-field__label">{field.label}</span>}
      <span className="quote-field__value">{field.value}</span>
    </span>)}
  </div>;
}
