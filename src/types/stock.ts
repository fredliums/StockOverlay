import type { Market } from './quote';

export interface StockEntry {
  market: Market;
  code: string;
  name: string;
  /** A former BSE code, shown as a search hint. Subscriptions use `code`. */
  legacyCode?: string;
}
