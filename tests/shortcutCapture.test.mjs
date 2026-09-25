import assert from 'node:assert/strict';
import test from 'node:test';
import { shortcutFromKeyEvent } from '../src/shortcutCapture.ts';

test('captures supported combinations in a stable modifier order', () => {
  assert.equal(shortcutFromKeyEvent({ code: 'KeyH', key: 'h', ctrlKey: true, altKey: true, shiftKey: false, metaKey: false }), 'Ctrl+Alt+H');
  assert.equal(shortcutFromKeyEvent({ code: 'Digit7', key: '&', ctrlKey: true, altKey: false, shiftKey: true, metaKey: false }), 'Ctrl+Shift+7');
  assert.equal(shortcutFromKeyEvent({ code: 'F12', key: 'F12', ctrlKey: false, altKey: true, shiftKey: false, metaKey: false }), 'Alt+F12');
});

test('ignores modifier-only, unsupported, and unmodified keys', () => {
  assert.equal(shortcutFromKeyEvent({ code: 'ControlLeft', key: 'Control', ctrlKey: true, altKey: false, shiftKey: false, metaKey: false }), null);
  assert.equal(shortcutFromKeyEvent({ code: 'ArrowUp', key: 'ArrowUp', ctrlKey: true, altKey: false, shiftKey: false, metaKey: false }), null);
  assert.equal(shortcutFromKeyEvent({ code: 'KeyA', key: 'a', ctrlKey: false, altKey: false, shiftKey: false, metaKey: false }), null);
});
