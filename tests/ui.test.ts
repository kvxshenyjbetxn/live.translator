import { render, screen, fireEvent } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import SettingsView from '../src/components/Settings.svelte';
import Translation from '../src/components/Translation.svelte';
import Sources from '../src/components/Sources.svelte';
import { defaults, idle, type Settings } from '../src/types';

test('key is masked, reveal is explicit, clear updates settings and plaintext warning is visible', async () => {
  let changed: Partial<Settings> = {};
  render(SettingsView, { settings: { ...defaults(), apiKey: 'test-only' }, directory: '/test/config', state: 'error', onChange: patch => { changed = patch; }, onRetry: () => {}, onOpen: () => {} });
  const field = screen.getByLabelText('Gemini API-ключ'); expect(field).toHaveAttribute('type', 'password');
  expect(screen.getByText(/відкритим текстом/)).toBeVisible();
  await fireEvent.click(screen.getByRole('button', { name: 'Показати ключ' })); expect(field).toHaveAttribute('type', 'text');
  await fireEvent.click(screen.getByRole('button', { name: 'Видалити ключ' })); expect(changed).toEqual({ apiKey: '' });
  expect(screen.getByRole('button', { name: 'Повторити збереження' })).toBeVisible();
});
test('connection check button reflects pending/success and never exposes key', async () => {
  let checks = 0;
  const view = render(SettingsView, { settings: { ...defaults(), apiKey: 'test-only' }, directory: '/test', state: 'saved', onChange: () => {}, onRetry: () => {}, onOpen: () => {}, onCheck: () => { checks++; } });
  await fireEvent.click(screen.getByRole('button', { name: 'Перевірити з’єднання' }));
  expect(checks).toBe(1);
  await view.rerender({ checking: true });
  expect(screen.getByRole('button', { name: 'Перевіряємо…' })).toBeDisabled();
  expect(screen.getByLabelText('Gemini API-ключ')).toBeDisabled();
  await view.rerender({ checking: false, connectionMessage: 'Google Live Translation доступний. Аудіо не передавалося.' });
  expect(screen.getByRole('status')).toHaveTextContent('Аудіо не передавалося');
});
test('connection test is disabled without a key or while translating', async () => {
  const view = render(SettingsView, { settings: defaults(), directory: '/test', state: 'saved', onChange: () => {}, onRetry: () => {}, onOpen: () => {} });
  expect(screen.getByRole('button', { name: 'Перевірити з’єднання' })).toBeDisabled();
  await view.rerender({ settings: { ...defaults(), apiKey: 'test-only' }, connectionBlocked: true });
  expect(screen.getByRole('button', { name: 'Перевірити з’єднання' })).toBeDisabled();
});

test('running session locks language but permits independent live volume change and stop', async () => {
  let changed: Partial<Settings> = {}; let stopped = false;
  render(Translation, { settings: defaults(), status: { ...idle(), sessionId: 1, phase: 'running', inputLevel: .4 }, sourceName: 'Chrome', languages: [{ code: 'uk', name: 'Українська' }], canStart: true, onStart: () => {}, onStop: () => { stopped = true; }, onChange: patch => { changed = patch; } });
  expect(screen.getByLabelText('Перекладати на')).toBeDisabled();
  const gain = screen.getByRole('slider', { name: 'Оригінал' }); expect(gain).toBeEnabled();
  await fireEvent.input(gain, { target: { value: '25' } }); expect(changed).toEqual({ originalVolume: .25 });
  await fireEvent.click(screen.getByRole('button', { name: /Зупинити переклад/ })); expect(stopped).toBe(true);
});
test('start is disabled without credentials/source; source selection emits a stable id', async () => {
  render(Translation, { settings: defaults(), status: idle(), sourceName: null, languages: [], canStart: false, onStart: () => {}, onStop: () => {}, onChange: () => {} });
  expect(screen.getByRole('button', { name: /Почати переклад/ })).toBeDisabled();
  let selected = '';
  render(Sources, { sources: [{ id: 'app:/Chrome.app', name: 'Chrome', active: true, detail: null }], selected: null, disabled: false, onSelect: id => { selected = id; }, onRefresh: () => {} });
  await fireEvent.click(screen.getByRole('button', { name: /Chrome/ })); expect(selected).toBe('app:/Chrome.app');
});
