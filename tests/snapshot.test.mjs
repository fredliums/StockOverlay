import assert from 'node:assert/strict';
import test from 'node:test';
import { acceptNewerSnapshot } from '../src/snapshot.ts';

test('an early event wins over an older initial command response', () => {
  const event = { revision: 2, quotes: [{ symbol: '600519' }], statuses: {} };
  const staleRead = { revision: 1, quotes: [], statuses: {} };
  const current = acceptNewerSnapshot(null, event);
  assert.strictEqual(acceptNewerSnapshot(current, staleRead), event);
});

test('a later snapshot replaces a stale event, including after visibility recovery', () => {
  const old = { revision: 3, quotes: [], statuses: {} };
  const latest = { revision: 5, quotes: [{ symbol: '000001' }], statuses: {} };
  assert.strictEqual(acceptNewerSnapshot(old, latest), latest);
  assert.strictEqual(acceptNewerSnapshot(latest, old), latest);
});
