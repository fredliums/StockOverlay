import type { QuoteSnapshot } from './types/quote';

export function acceptNewerSnapshot(
  current: QuoteSnapshot | null,
  incoming: QuoteSnapshot,
): QuoteSnapshot {
  return current === null || incoming.revision > current.revision ? incoming : current;
}
