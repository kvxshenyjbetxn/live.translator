import { expect, test } from 'vitest';
import { shortcutFromKey } from '../src/lib/shortcut';
test('shortcut recorder uses physical keys and portable modifier spelling', () => {
  expect(shortcutFromKey({ code: 'KeyT', ctrlKey: true, shiftKey: true, altKey: false, metaKey: false, repeat: false })).toBe('Control+Shift+KeyT');
  expect(shortcutFromKey({ code: 'Space', ctrlKey: false, shiftKey: false, altKey: true, metaKey: true, repeat: false })).toBe('Super+Alt+Space');
});
test('plain, modifier-only, unsupported and repeated keys cannot become shortcuts', () => {
  const base = { code: 'KeyT', ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, repeat: false };
  expect(shortcutFromKey(base)).toBeNull();
  expect(shortcutFromKey({ ...base, ctrlKey: true, code: 'ControlLeft' })).toBeNull();
  expect(shortcutFromKey({ ...base, ctrlKey: true, code: 'Unidentified' })).toBeNull();
  expect(shortcutFromKey({ ...base, ctrlKey: true, repeat: true })).toBeNull();
});
