import type { OrderLevel } from './types/quote';

const MISSING = '--';

function valid(value: number | null | undefined): value is number {
  return value !== null && value !== undefined && Number.isFinite(value);
}

function decimal(value: number, places: number): string {
  return value.toFixed(places);
}

function signed(value: number, places: number): string {
  return `${value > 0 ? '+' : ''}${decimal(value, places)}`;
}

function trimmed(value: number, places: number): string {
  return decimal(value, places).replace(/0+$/, '').replace(/\.$/, '');
}

export function formatPrice(value: number | null | undefined): string {
  return valid(value) ? decimal(value, 2) : MISSING;
}

export function formatChange(value: number | null | undefined): string {
  return valid(value) ? signed(value, 2) : MISSING;
}

export function formatChangePercent(value: number | null | undefined): string {
  return valid(value) ? `${signed(value, 2)}%` : MISSING;
}

export function formatTurnover(value: number | null | undefined): string {
  if (!valid(value)) return MISSING;
  return Math.abs(value) >= 100_000_000
    ? `${trimmed(value / 100_000_000, 1)}亿`
    : `${trimmed(value / 10_000, 2)}万`;
}

export function formatTurnoverRate(value: number | null | undefined): string {
  return valid(value) ? `${decimal(value, 2)}%` : MISSING;
}

export function formatPE(value: number | null | undefined): string {
  return valid(value) ? decimal(value, 2) : MISSING;
}

export function formatVolumeRatio(value: number | null | undefined): string {
  return valid(value) ? decimal(value, 2) : MISSING;
}

export function formatOrderRatio(value: number | null | undefined): string {
  return valid(value) ? `${signed(value, 2)}%` : MISSING;
}

export function formatLots(value: number | null | undefined): string {
  if (!valid(value) || value < 0) return MISSING;
  return value >= 10_000 ? `${trimmed(value / 10_000, 1)}万` : decimal(value, 0);
}

export function formatLevel(level: OrderLevel | null | undefined): string {
  if (!level || !valid(level.price) || !valid(level.volumeLots)) return MISSING;
  return `${formatPrice(level.price)}-${formatLots(level.volumeLots)}`;
}

/** Ask one / bid one, each with price and volume in lots. */
export function formatLevel1(
  ask: OrderLevel | null | undefined,
  bid: OrderLevel | null | undefined,
): string {
  return `${formatLevel(ask)} / ${formatLevel(bid)}`;
}
