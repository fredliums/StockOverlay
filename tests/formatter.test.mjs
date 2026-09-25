import assert from 'node:assert/strict';
import test from 'node:test';
import {
  formatChange,
  formatChangePercent,
  formatLevel,
  formatLevel1,
  formatLots,
  formatOrderRatio,
  formatPE,
  formatPrice,
  formatTurnover,
  formatTurnoverRate,
  formatVolumeRatio,
} from '../src/formatter.ts';

test('formats signed prices and percentages without changing the percent scale', () => {
  assert.equal(formatPrice(1418.32), '1418.32');
  assert.equal(formatPrice(0), '0.00');
  assert.equal(formatChange(29.81), '+29.81');
  assert.equal(formatChange(-3.26), '-3.26');
  assert.equal(formatChange(0), '0.00');
  assert.equal(formatChangePercent(2.15), '+2.15%');
  assert.equal(formatChangePercent(-0.68), '-0.68%');
  assert.equal(formatTurnoverRate(2.36), '2.36%');
  assert.equal(formatOrderRatio(18.26), '+18.26%');
  assert.equal(formatOrderRatio(-18.26), '-18.26%');
});

test('scales yuan turnover to ten-thousands and hundred-millions', () => {
  assert.equal(formatTurnover(86_230_000), '8623万');
  assert.equal(formatTurnover(1_260_000_000), '12.6亿');
  assert.equal(formatTurnover(0), '0万');
  assert.equal(formatTurnover(12_300), '1.23万');
  assert.equal(formatPE(24.61), '24.61');
  assert.equal(formatVolumeRatio(1.38), '1.38');
});

test('keeps ask-one before bid-one and treats missing values as missing', () => {
  const ask = { price: 1418.8, volumeLots: 85 };
  const bid = { price: 1418.7, volumeLots: 102 };
  assert.equal(formatLevel1(ask, bid), '1418.80-85 / 1418.70-102');
  assert.equal(formatLevel1(null, null), '-- / --');
  assert.equal(formatLevel1({ price: null, volumeLots: 4 }, bid), '-- / 1418.70-102');
  assert.equal(formatLevel({ price: 1.2, volumeLots: 0 }), '1.20-0');
  assert.equal(formatLots(null), '--');
});

test('never exposes NaN or undefined for absent or nonfinite values', () => {
  for (const formatter of [
    formatPrice,
    formatChange,
    formatChangePercent,
    formatTurnover,
    formatTurnoverRate,
    formatPE,
    formatVolumeRatio,
    formatOrderRatio,
  ]) {
    assert.equal(formatter(null), '--');
    assert.equal(formatter(Number.NaN), '--');
    assert.equal(formatter(Number.POSITIVE_INFINITY), '--');
  }
});
