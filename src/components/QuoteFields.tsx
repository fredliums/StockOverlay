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

type Field = { key: string; label: string; value: string };

export function QuoteFields({ quote, display }: { quote: Quote | null; display: DisplayConfig }) {
  const fields: Field[] = [
    ...(display.showPrice ? [{ key: 'price', label: '现', value: formatPrice(quote?.price) }] : []),
    ...(display.showChange ? [{ key: 'change', label: '涨跌', value: formatChange(quote?.change) }] : []),
    ...(display.showChangePercent ? [{ key: 'changePercent', label: '涨跌幅', value: formatChangePercent(quote?.changePercent) }] : []),
    ...(display.showTurnover ? [{ key: 'turnover', label: '额', value: formatTurnover(quote?.turnover) }] : []),
    ...(display.showHigh ? [{ key: 'high', label: '高', value: formatPrice(quote?.high) }] : []),
    ...(display.showLow ? [{ key: 'low', label: '低', value: formatPrice(quote?.low) }] : []),
    ...(display.showTurnoverRate ? [{ key: 'turnoverRate', label: '换', value: formatTurnoverRate(quote?.turnoverRate) }] : []),
    ...(display.showPE ? [{ key: 'pe', label: 'PE', value: formatPE(quote?.pe) }] : []),
    ...(display.showVolumeRatio ? [{ key: 'volumeRatio', label: '量比', value: formatVolumeRatio(quote?.volumeRatio) }] : []),
    ...(display.showOrderRatio ? [{ key: 'orderRatio', label: '委比', value: formatOrderRatio(quote?.orderRatio) }] : []),
  ];

  return <div className="quote-fields">
    {fields.map((field) => <span className="quote-field" key={field.key}>
      <span className="quote-field__label">{field.label}</span>
      <span className="quote-field__value">{field.value}</span>
    </span>)}
  </div>;
}
