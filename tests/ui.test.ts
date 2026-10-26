import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import SettingsView from '../src/components/Settings.svelte';
import Translation from '../src/components/Translation.svelte';
import Sources from '../src/components/Sources.svelte';
import { defaults, idle, type Settings } from '../src/types';

test('appearance and language controls are the last settings card', () => {
  const { container } = render(SettingsView, { settings: defaults(), directory: '/test', state: 'saved', onChange: () => {}, onRetry: () => {}, onOpen: () => {} });
  const cards = container.querySelectorAll('.settings-view > .card');
  expect(cards[cards.length - 1]).toHaveClass('appearance-settings');
  expect(cards[0]).toContainElement(screen.getByLabelText('Gemini API-ключ'));
});

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

test('shortcut settings default to active application and allow changing mode and clearing binding', async () => {
  let changed: Partial<Settings> = {};
  render(SettingsView, { settings: { ...defaults(), captureShortcut: 'Control+Shift+KeyT' }, directory: '/test', state: 'saved', onChange: patch => { changed = patch; }, onRetry: () => {}, onOpen: () => {} });
  expect(screen.getByLabelText('Дія гарячої клавіші')).toHaveValue('activeApplication');
  await fireEvent.change(screen.getByLabelText('Дія гарячої клавіші'), { target: { value: 'lastCaptured' } });
  expect(changed).toEqual({ shortcutMode: 'lastCaptured' });
  await fireEvent.click(screen.getByRole('button', { name: 'Вимкнути гарячу клавішу' }));
  expect(changed).toEqual({ captureShortcut: null });
});

test('recording waits for key release before changing the shortcut', async () => {
  let changed: Partial<Settings> = {};
  render(SettingsView, { settings: defaults(), directory: '/test', state: 'saved', onChange: patch => { changed = patch; }, onRetry: () => {}, onOpen: () => {} });
  await fireEvent.click(screen.getByRole('button', { name: 'Обрати гарячу клавішу' }));
  const recorder = await screen.findByRole('button', { name: 'Натисніть комбінацію…' });
  await fireEvent.keyDown(recorder, { code: 'KeyT', ctrlKey: true, shiftKey: true });
  expect(changed).toEqual({});
  await fireEvent.keyUp(recorder, { code: 'KeyT', ctrlKey: true, shiftKey: true });
  expect(changed).toEqual({ captureShortcut: 'Control+Shift+KeyT' });
});

test('recorder focuses an enabled control after native setup and records window-level keys', async () => {
  let changed: Partial<Settings> = {};
  render(SettingsView, { settings: defaults(), directory: '/test', state: 'saved', onChange: patch => { changed = patch; }, onRetry: () => {}, onOpen: () => {} });
  await fireEvent.click(screen.getByRole('button', { name: 'Обрати гарячу клавішу' }));
  const recorder = await screen.findByRole('button', { name: 'Натисніть комбінацію…' });
  await waitFor(() => expect(recorder).toHaveFocus());
  recorder.blur(); // macOS/WebKit need not focus a button on mouse click.
  await fireEvent.keyDown(window, { code: 'KeyT', ctrlKey: true, shiftKey: true });
  expect(changed).toEqual({});
  await fireEvent.keyUp(window, { code: 'KeyT', ctrlKey: true, shiftKey: true });
  expect(changed).toEqual({ captureShortcut: 'Control+Shift+KeyT' });
});

test('Escape cancels recording even when focus is outside the recorder', async () => {
  let changed: Partial<Settings> = {};
  render(SettingsView, { settings: defaults(), directory: '/test', state: 'saved', onChange: patch => { changed = patch; }, onRetry: () => {}, onOpen: () => {} });
  await fireEvent.click(screen.getByRole('button', { name: 'Обрати гарячу клавішу' }));
  await screen.findByRole('button', { name: 'Натисніть комбінацію…' });
  await fireEvent.keyDown(window, { code: 'Escape' });
  expect(await screen.findByRole('button', { name: 'Обрати гарячу клавішу' })).toBeEnabled();
  expect(changed).toEqual({});
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
