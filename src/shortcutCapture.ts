type KeyInput = Pick<KeyboardEvent, 'code' | 'key' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey'>;

export function shortcutFromKeyEvent(event: KeyInput): string | null {
  const modifiers = [
    event.ctrlKey && 'Ctrl',
    event.altKey && 'Alt',
    event.shiftKey && 'Shift',
    event.metaKey && 'Super',
  ].filter((part): part is string => Boolean(part));
  if (modifiers.length === 0) return null;

  let key: string | null = null;
  if (/^Key[A-Z]$/.test(event.code)) key = event.code.slice(3);
  else if (/^Digit[0-9]$/.test(event.code)) key = event.code.slice(5);
  else if (/^F(?:[1-9]|1[0-9]|2[0-4])$/.test(event.key)) key = event.key;
  return key ? [...modifiers, key].join('+') : null;
}
