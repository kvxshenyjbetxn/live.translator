import { expect, test } from 'vitest';
import { defaults } from '../src/types';
import {
  MAX_SERIALIZED_BYTES,
  MAX_SOURCE_ID_BYTES,
  MAX_SOURCE_NAME_BYTES,
  isFavorite,
  preferenceError,
  recordName,
  sourceCatalog,
  toggleFavorite,
  toggleHidden,
} from '../src/lib/sources';

const chrome = { id: 'app:/Chrome.app', name: 'Chrome', active: true, detail: null };
const ghost = { id: 'app:/Closed.app', name: 'Closed', active: false, detail: null };
const tooBig = 'Список програм завеликий для збереження. Приберіть частину обраних або прихованих програм.';

test('favorites lead, unavailable favorites are placeholders and every hidden record stays listed', () => {
  const catalog = sourceCatalog([chrome], [{ id: ghost.id, name: 'Closed' }, { id: chrome.id, name: 'Chrome' }], [{ id: 'app:/Gone.app', name: 'Gone' }]);
  expect(catalog.visible.map(s => s.id)).toEqual([ghost.id, chrome.id]);
  expect(catalog.visible[0]).toMatchObject({ available: false, active: false, favorite: true });
  expect(catalog.hidden.map(s => s.id)).toEqual(['app:/Gone.app']);
  expect(catalog.hidden[0]).toMatchObject({ available: false, favorite: false, name: 'Gone' });
});

test('live names win, duplicates collapse, hidden favorites keep stars and restore to the top', () => {
  expect(sourceCatalog([{ ...chrome, name: 'Chrome Beta' }], [{ id: chrome.id, name: 'Old' }], []).visible[0].name).toBe('Chrome Beta');
  const catalog = sourceCatalog([chrome], [{ id: chrome.id, name: 'Old' }, { id: chrome.id, name: 'Again' }], [{ id: chrome.id, name: 'Old' }]);
  expect(catalog.all).toHaveLength(1);
  expect(catalog.visible).toHaveLength(0);
  expect(catalog.hidden[0]).toMatchObject({ favorite: true });
  expect(sourceCatalog([chrome], [{ id: chrome.id, name: 'Old' }], []).visible[0].id).toBe(chrome.id);
});

test('toggle and record helpers keep order and identity', () => {
  expect(toggleFavorite([], { id: chrome.id, name: 'Chrome' })).toEqual([{ id: chrome.id, name: 'Chrome' }]);
  expect(toggleFavorite([{ id: chrome.id, name: 'Chrome' }], { id: chrome.id, name: 'Chrome' })).toEqual([]);
  expect(toggleHidden([], { id: 'app:/Gone.app', name: 'Gone' })).toEqual([{ id: 'app:/Gone.app', name: 'Gone' }]);
  expect(recordName([], [{ id: 'app:/Gone.app', name: 'Gone' }], 'app:/Gone.app')).toBe('Gone');
  expect(recordName([], [], 'app:/Gone.app')).toBeNull();
});

test('toggles append in order, keep the list untouched and report membership', () => {
  const favorites = [{ id: ghost.id, name: 'Closed' }];
  expect(toggleFavorite(favorites, { id: chrome.id, name: 'Chrome' })).toEqual([{ id: ghost.id, name: 'Closed' }, { id: chrome.id, name: 'Chrome' }]);
  expect(favorites).toEqual([{ id: ghost.id, name: 'Closed' }]);
  expect(toggleHidden([{ id: ghost.id, name: 'Closed' }], { id: ghost.id, name: 'Closed' })).toEqual([]);
  expect(isFavorite(favorites, ghost.id)).toBe(true);
  expect(isFavorite(favorites, chrome.id)).toBe(false);
  expect(recordName([{ id: chrome.id, name: 'Chrome' }], [], chrome.id)).toBe('Chrome');
});

test('preflight refuses counts, byte length, duplicates and escaped size before any save', () => {
  const many = Array.from({ length: 32 }, (_, n) => ({ id: `app:/F${n}.app`, name: 'F' }));
  expect(preferenceError(defaults(), { favoriteSources: [...many, { id: 'app:/X.app', name: 'X' }] })).toBe('Можна додати не більше 32 програм до обраних.');
  expect(preferenceError(defaults(), { hiddenSources: [...many, { id: 'app:/A.app', name: 'A' }] })).toBe('Можна приховати не більше 32 програм.');
  expect(preferenceError(defaults(), { favoriteSources: [{ id: `app:/${'x'.repeat(509)}`, name: 'A' }] })).toBe(tooBig);
  expect(preferenceError(defaults(), { favoriteSources: [{ id: 'app:/A.app', name: 'A' }, { id: 'app:/A.app', name: 'A' }] })).toBe(tooBig);
  const quotes = Array.from({ length: 31 }, (_, n) => ({ id: `app:/${n}${'"'.repeat(500)}`, name: 'A' }));
  expect(new TextEncoder().encode(JSON.stringify({ ...defaults(), apiKey: 'k'.repeat(4096), favoriteSources: quotes })).length * 2).toBeGreaterThan(MAX_SERIALIZED_BYTES);
  expect(preferenceError({ ...defaults(), apiKey: 'k'.repeat(4096) }, { favoriteSources: quotes })).toBe(tooBig);
  expect(preferenceError(defaults(), { favoriteSources: [{ id: 'app:/A.app', name: 'A' }] })).toBeNull();
});

test('preflight counts UTF-8 bytes, not characters, and accepts the exact caps', () => {
  expect(preferenceError(defaults(), { favoriteSources: [{ id: 'app:/A.app', name: 'і'.repeat(65) }] })).toBe(tooBig);
  expect(preferenceError(defaults(), { favoriteSources: [{ id: 'app:/A.app', name: 'і'.repeat(64) }] })).toBeNull();
  expect(MAX_SOURCE_NAME_BYTES).toBe(128);
  expect(preferenceError(defaults(), { favoriteSources: [{ id: `app:/${'x'.repeat(MAX_SOURCE_ID_BYTES - 5)}`, name: 'A' }] })).toBeNull();
  expect(preferenceError(defaults(), { favoriteSources: [{ id: '', name: 'A' }] })).toBe(tooBig);
  expect(preferenceError(defaults(), { hiddenSources: [{ id: 'app:/A.app', name: '' }] })).toBe(tooBig);
  expect(preferenceError(defaults(), { favoriteSources: Array.from({ length: 32 }, (_, n) => ({ id: `app:/F${n}.app`, name: 'F' })) })).toBeNull();
});

test('an oversized existing file can still shrink so preferences stay removable', () => {
  const quotes = Array.from({ length: 31 }, (_, n) => ({ id: `app:/${n}${'"'.repeat(500)}`, name: 'A' }));
  const oversized = { ...defaults(), apiKey: 'k'.repeat(4096), favoriteSources: quotes };
  expect(preferenceError(oversized, { favoriteSources: quotes.slice(1) })).toBeNull();
  expect(preferenceError(oversized, { favoriteSources: [] })).toBeNull();
  expect(preferenceError({ ...oversized, favoriteSources: quotes.slice(1) }, { favoriteSources: quotes })).toBe(tooBig);
});
