/** Six-digit exchange code; use `${market}:${code}` as the unique key. */
export interface Symbol {
  code: string;
  market: Market;
}

export type Market = 'SH' | 'SZ' | 'BJ';

/** Percent fields hold percent values, such as 2.15 for 2.15%. */
export interface Quote {
  symbol: string;
  market: Market;
  name: string;
  price: number | null;
  previousClose: number | null;
  change: number | null;
  changePercent: number | null;
  high: number | null;
  low: number | null;
  /** Yuan. */
  turnover: number | null;
  turnoverRate: number | null;
  /** Trailing twelve-month price-to-earnings ratio. */
  pe: number | null;
  volumeRatio: number | null;
  orderRatio: number | null;
  bidLevels: OrderLevel[];
  askLevels: OrderLevel[];
  /** Exchange quote time as Unix milliseconds. */
  timestamp: number | null;
}

/** Volume in lots; index zero of each side is level one. */
export interface OrderLevel {
  price: number | null;
  volumeLots: number | null;
}

export type QuoteState = 'loading' | 'normal' | 'delayed' | 'disconnected' | 'invalid';

export interface QuoteStatus {
  state: QuoteState;
  /** Local receipt time as Unix milliseconds. */
  lastSuccessAt: number | null;
  message?: string;
}

export interface QuoteSnapshot {
  revision: number;
  quotes: Quote[];
  /** Keys have the form `SH:600519`. */
  statuses: Record<string, QuoteStatus>;
}

export interface QuoteBatchResult {
  quotes: Quote[];
  failures: QuoteFailure[];
}

export interface QuoteFailure {
  symbol: Symbol;
  kind: QuoteFailureKind;
}

export type QuoteFailureKind =
  | 'InvalidSymbol'
  | 'Parse'
  | 'MissingRecord'
  | 'Network'
  | 'RateLimited';
