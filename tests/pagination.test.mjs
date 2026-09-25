import assert from 'node:assert/strict';
import test from 'node:test';
import { paginateStocks } from '../src/pagination.ts';

test('packs complete stocks without changing their order', () => {
  const pages = paginateStocks([50, 60, 70], [2, 2, 2], 120, () => {
    throw new Error('complete cards do not need part measurement');
  });
  assert.deepEqual(pages.map((page) => page.map((part) => part.stockIndex)), [[0, 1], [2]]);
  assert.ok(pages.flat().every((part) => part.start === 0 && part.end === Infinity));
});

test('splits an oversized stock only at atom boundaries and repeats its identity', () => {
  const pages = paginateStocks([300], [5], 110, (_stock, start, end) => 20 + (end - start) * 35);
  assert.deepEqual(pages, [
    [{ stockIndex: 0, start: 0, end: 2 }],
    [{ stockIndex: 0, start: 2, end: 4 }],
    [{ stockIndex: 0, start: 4, end: 5 }],
  ]);
});

test('starts a new page before splitting the next oversized stock', () => {
  const pages = paginateStocks([70, 250], [1, 4], 120, (_stock, start, end) => 25 + (end - start) * 35);
  assert.deepEqual(pages.map((page) => page.map((part) => [part.stockIndex, part.start, part.end])), [
    [[0, 0, Infinity]],
    [[1, 0, 2]],
    [[1, 2, 4]],
  ]);
});
