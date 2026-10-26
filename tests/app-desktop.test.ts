import { render, screen, waitFor, fireEvent } from '@testing-library/svelte';
import { beforeEach, expect, test, vi } from 'vitest';
import { defaults, idle, type AudioSource, type SessionStatus, type Settings } from '../src/types';
import type { DesktopAction } from '../src/lib/desktop';
const native = vi.hoisted(() => ({ handlers: new Map<string, (event: { payload: DesktopAction }) => void>(), api: {
  load: vi.fn(), directory: vi.fn(), languages: vi.fn(), sources: vi.fn(), save: vi.fn(),
  status: vi.fn(), start: vi.fn(), stop: vi.fn(), volumes: vi.fn(), ready: vi.fn(),
  syncDesktop: vi.fn(), shortcutStatus: vi.fn(), suspendShortcut: vi.fn(), show: vi.fn(),
  beginClose: vi.fn(), cancelClose: vi.fn(), finishClose: vi.fn(), checkConnection: vi.fn(), openDirectory: vi.fn(),
} }));
vi.mock('../src/bridge', () => ({ desktop: true, api: native.api }));
vi.mock('@tauri-apps/api/event', () => ({ listen: async (name: string, handler: (event: { payload: DesktopAction }) => void) => { native.handlers.set(name, handler); return () => { native.handlers.delete(name); }; } }));
import App from '../src/App.svelte';
const source: AudioSource = { id: 'app:/Test.app', name: 'Test', active: true, detail: null };
let current: SessionStatus;
beforeEach(() => {
  vi.resetAllMocks(); native.handlers.clear(); current = idle();
  native.api.load.mockResolvedValue({ ...defaults(), apiKey: 'test-key', selectedSourceId: source.id });
  native.api.save.mockImplementation(async (settings: Settings) => ({ captureShortcut: settings.captureShortcut, shortcutError: null }));
  native.api.show.mockResolvedValue(undefined);
  native.api.directory.mockResolvedValue('/test'); native.api.languages.mockResolvedValue([{ code: 'uk', name: 'Українська' }]);
  native.api.sources.mockResolvedValue([source]); native.api.status.mockImplementation(async () => current);
  native.api.shortcutStatus.mockResolvedValue({ active: null, error: null, integrationError: null });
  native.api.start.mockImplementation(async () => { current = { ...idle(), sessionId: 1, phase: 'running' }; return 1; });
  native.api.stop.mockImplementation(async () => { current = { ...current, phase: 'idle' }; });
});
test('appearance and interface language apply, autosave, sync tray and restore independently of audio language', async () => {
  const initial = { ...defaults(), apiKey: 'test-key', targetLanguage: 'de', selectedSourceId: source.id };
  native.api.load.mockResolvedValue(initial);
  await loaded();
  await fireEvent.click(screen.getByRole('button', { name: /Налаштування/ }));
  await fireEvent.change(screen.getByLabelText('Тема'), { target: { value: 'amoled' } });
  await fireEvent.input(screen.getByLabelText('Колір акценту'), { target: { value: '#ff0088' } });
  await fireEvent.change(screen.getByLabelText('Мова інтерфейсу'), { target: { value: 'en' } });
  expect(await screen.findByLabelText('Interface language')).toHaveValue('en');
  expect(document.documentElement.dataset.theme).toBe('amoled');
  expect(document.documentElement.style.getPropertyValue('--accent')).toBe('#ff0088');
  expect(document.documentElement.lang).toBe('en');
  await waitFor(() => expect(native.api.save.mock.calls.at(-1)?.[0]).toMatchObject({ theme: 'amoled', accentColor: '#ff0088', interfaceLanguage: 'en', targetLanguage: 'de', apiKey: 'test-key' }));
  await waitFor(() => expect(native.api.syncDesktop.mock.calls.at(-1)?.[0]).toMatchObject({ interfaceLanguage: 'en', targetLanguage: 'de' }));
  expect(native.api.start).not.toHaveBeenCalled();
  expect(native.api.stop).not.toHaveBeenCalled();
});
test('restores persisted English AMOLED settings before showing the working UI', async () => {
  native.api.load.mockResolvedValue({ ...defaults(), apiKey: 'test-key', selectedSourceId: source.id, theme: 'amoled', interfaceLanguage: 'en', accentColor: '#123456' });
  render(App);
  await waitFor(() => expect(screen.getByRole('button', { name: /Start translation/ })).toBeEnabled());
  expect(document.documentElement.lang).toBe('en');
  expect(document.documentElement.dataset.theme).toBe('amoled');
  expect(document.documentElement.style.getPropertyValue('--accent')).toBe('#123456');
  expect(native.api.save).not.toHaveBeenCalled();
});

async function loaded() {
  render(App);
  await waitFor(() => expect(screen.getByRole('button', { name: /Почати переклад/ })).toBeEnabled());
}
function action(payload: DesktopAction) { native.handlers.get('desktop-action')!({ payload }); }
test('a rejected replacement reconciles the working binding and does not block later starts', async () => {
  const old = 'Control+Shift+KeyA';
  native.api.load.mockResolvedValue({ ...defaults(), apiKey: 'test-key', selectedSourceId: source.id, captureShortcut: old });
  native.api.save.mockImplementation(async (settings: Settings) => ({ captureShortcut: old, shortcutError: settings.captureShortcut === old ? null : 'Конфлікт клавіші' }));
  await loaded();
  await fireEvent.click(screen.getByRole('button', { name: /Налаштування/ }));
  await fireEvent.click(screen.getByRole('button', { name: 'Обрати гарячу клавішу' }));
  const recorder = await screen.findByRole('button', { name: 'Натисніть комбінацію…' });
  await fireEvent.keyDown(recorder, { code: 'KeyT', ctrlKey: true, shiftKey: true });
  await fireEvent.keyUp(recorder, { code: 'KeyT', ctrlKey: true, shiftKey: true });
  await waitFor(() => expect(native.api.save.mock.calls.some(([s]) => (s as Settings).captureShortcut === old)).toBe(true));
  expect(screen.getByText(old, { selector: 'code' })).toBeVisible();
  expect(screen.getByText(/Конфлікт клавіші/)).toBeVisible();
  await fireEvent.click(screen.getByRole('button', { name: /^◉/ }));
  action({ kind: 'shortcut', value: source.id });
  await waitFor(() => expect(native.api.start).toHaveBeenCalledWith(source.id));
});
test('leaving settings during recorder setup releases native suspension and restores start', async () => {
  await loaded();
  let release!: () => void;
  native.api.suspendShortcut.mockImplementationOnce(() => new Promise<void>(resolve => { release = resolve; })).mockResolvedValue(undefined);
  await fireEvent.click(screen.getByRole('button', { name: /Налаштування/ }));
  await fireEvent.click(screen.getByRole('button', { name: 'Обрати гарячу клавішу' }));
  await fireEvent.click(screen.getByRole('button', { name: /^◉/ }));
  release();
  await waitFor(() => expect(native.api.suspendShortcut).toHaveBeenCalledWith(false));
  await waitFor(() => expect(screen.getByRole('button', { name: /Почати переклад/ })).toBeEnabled());
  action({ kind: 'shortcut', value: source.id });
  await waitFor(() => expect(native.api.start).toHaveBeenCalledWith(source.id));
});
test('reselecting the checked tray source refreshes its native checkmark without capture', async () => {
  await loaded();
  await waitFor(() => expect(native.api.syncDesktop).toHaveBeenCalled());
  const count = native.api.syncDesktop.mock.calls.length;
  action({ kind: 'source', value: source.id });
  await waitFor(() => expect(native.api.syncDesktop.mock.calls.length).toBeGreaterThan(count));
  expect(native.api.start).not.toHaveBeenCalled();
});
test('shortcut unavailable foreground never starts the selected application', async () => {
  await loaded();
  action({ kind: 'shortcut', value: 'app:/Missing.app' });
  await screen.findByText(/Програма недоступна для захоплення/);
  expect(native.api.start).not.toHaveBeenCalled();
  expect(native.api.show).toHaveBeenCalled();
});
test('second toggle cancels a start waiting on discovery without starting another session', async () => {
  await loaded();
  let release!: (sources: AudioSource[]) => void;
  native.api.sources.mockImplementationOnce(() => new Promise<AudioSource[]>(resolve => { release = resolve; }));
  action({ kind: 'shortcut', value: source.id });
  action({ kind: 'shortcut', value: source.id });
  release([source]);
  await waitFor(() => expect(native.api.stop).toHaveBeenCalled());
  await waitFor(() => expect(screen.getByRole('button', { name: /Почати переклад/ })).toBeEnabled());
  expect(native.api.start).not.toHaveBeenCalled();
});
test('window start persists last successful source and native stop does not retarget', async () => {
  await loaded();
  await fireEvent.click(screen.getByRole('button', { name: /Почати переклад/ }));
  await waitFor(() => expect(native.api.start).toHaveBeenCalledWith(source.id));
  await waitFor(() => expect(native.api.save.mock.calls.some(([s]) => (s as Settings).lastCapturedSourceId === source.id)).toBe(true));
  action({ kind: 'stop', value: 'app:/Other.app' });
  await waitFor(() => expect(native.api.stop).toHaveBeenCalled());
  expect(native.api.start).toHaveBeenCalledTimes(1);
});
