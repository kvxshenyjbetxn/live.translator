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
const ghost: AudioSource = { id: 'app:/Closed.app', name: 'Closed', active: false, detail: null };
async function waitingFavorite() {
  native.api.load.mockResolvedValue({ ...defaults(), apiKey: 'test-key', selectedSourceId: ghost.id, favoriteSources: [{ id: ghost.id, name: 'Closed' }] });
  native.api.sources.mockResolvedValue([]);
  await loaded();
  await fireEvent.click(screen.getByRole('button', { name: /Почати переклад/ }));
  await screen.findByRole('button', { name: /Скасувати очікування/ });
}
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
test('unavailable favorite waits with no session and starts only the exact id', async () => {
  await waitingFavorite();
  expect(native.api.start).not.toHaveBeenCalled();
  native.api.sources.mockResolvedValue([ghost]);
  await fireEvent.click(screen.getByRole('button', { name: 'Оновити список програм' }));
  await waitFor(() => expect(native.api.start).toHaveBeenCalledWith(ghost.id));
  expect(native.api.start).toHaveBeenCalledTimes(1);
});
test('Stop invalidates a pending auto-start at discovery', async () => {
  await waitingFavorite();
  let release!: (s: AudioSource[]) => void;
  native.api.sources.mockImplementationOnce(() => new Promise<AudioSource[]>(resolve => { release = resolve; }));
  await fireEvent.click(screen.getByRole('button', { name: 'Оновити список програм' }));
  action({ kind: 'stop' });
  release([ghost]);
  await waitFor(() => expect(screen.queryByRole('button', { name: /Скасувати очікування/ })).toBeNull());
  expect(native.api.start).not.toHaveBeenCalled();
});
test('selecting another source invalidates a pending auto-start at discovery', async () => {
  native.api.load.mockResolvedValue({ ...defaults(), apiKey: 'test-key', selectedSourceId: ghost.id, favoriteSources: [{ id: ghost.id, name: 'Closed' }] });
  native.api.sources.mockResolvedValue([source]);
  await loaded();
  // The start discovery keeps the unrelated source listed, so it stays selectable in the sidebar.
  await fireEvent.click(screen.getByRole('button', { name: /Почати переклад/ }));
  await screen.findByRole('button', { name: /Скасувати очікування/ });
  let release!: (s: AudioSource[]) => void;
  native.api.sources.mockImplementationOnce(() => new Promise<AudioSource[]>(resolve => { release = resolve; }));
  await fireEvent.click(screen.getByRole('button', { name: 'Оновити список програм' }));
  await fireEvent.click(screen.getByRole('button', { name: /Test/ }));
  release([ghost]);
  await waitFor(() => expect(screen.queryByRole('button', { name: /Скасувати очікування/ })).toBeNull());
  expect(native.api.start).not.toHaveBeenCalled();
});
test('Stop invalidates a start paused in the settings flush before api.start', async () => {
  const other: AudioSource = { id: 'app:/Other.app', name: 'Other', active: true, detail: null };
  native.api.sources.mockResolvedValue([source, other]);
  await loaded();
  let release!: () => void;
  native.api.save.mockImplementationOnce((s: Settings) => new Promise(resolve => { release = () => resolve({ captureShortcut: s.captureShortcut, shortcutError: null }); }));
  action({ kind: 'shortcut', value: other.id });
  await waitFor(() => expect(native.api.save).toHaveBeenCalled());
  action({ kind: 'stop' });
  release();
  await waitFor(() => expect(screen.getByRole('button', { name: /Почати переклад/ })).toBeVisible());
  expect(native.api.start).not.toHaveBeenCalled();
});
test('Stop during an in-flight api.start stops the late session without a duplicate', async () => {
  await loaded();
  let release!: (id: number) => void;
  native.api.start.mockImplementationOnce(() => new Promise<number>(resolve => { release = resolve; }));
  await fireEvent.click(screen.getByRole('button', { name: /Почати переклад/ }));
  await waitFor(() => expect(native.api.start).toHaveBeenCalledWith(source.id));
  action({ kind: 'stop' });
  release(1);
  await waitFor(() => expect(native.api.stop.mock.calls.length).toBe(2));
  await waitFor(() => expect(screen.getByRole('button', { name: /Почати переклад/ })).toBeEnabled());
  expect(screen.queryByRole('button', { name: /Зупинити переклад/ })).toBeNull();
  await fireEvent.click(screen.getByRole('button', { name: /Почати переклад/ }));
  await waitFor(() => expect(native.api.start).toHaveBeenCalledTimes(2));
});
test('hiding the waiting favorite keeps its name and a working cancel', async () => {
  await waitingFavorite();
  await fireEvent.click(screen.getByRole('button', { name: 'Приховати' }));
  expect(screen.getByText('Closed')).toBeVisible();
  await fireEvent.click(screen.getByRole('button', { name: /Скасувати очікування/ }));
  await waitFor(() => expect(native.api.stop).toHaveBeenCalled());
  expect(native.api.start).not.toHaveBeenCalled();
});
test('unstarring the waiting favorite keeps its name and a working cancel', async () => {
  await waitingFavorite();
  await fireEvent.click(screen.getByRole('button', { name: 'Прибрати з обраних' }));
  await waitFor(() => expect(native.api.save.mock.calls.at(-1)?.[0]).toMatchObject({ favoriteSources: [] }));
  expect(screen.getByText('Closed')).toBeVisible();
  await fireEvent.click(screen.getByRole('button', { name: /Скасувати очікування/ }));
  await waitFor(() => expect(native.api.stop).toHaveBeenCalled());
});
test('a different source appearing never satisfies a waiting favorite', async () => {
  await waitingFavorite();
  native.api.sources.mockResolvedValue([{ id: 'app:/Other.app', name: 'Other', active: true, detail: null }]);
  await fireEvent.click(screen.getByRole('button', { name: 'Оновити список програм' }));
  await waitFor(() => expect(screen.getByText('Other')).toBeVisible());
  expect(native.api.start).not.toHaveBeenCalled();
  expect(screen.getByRole('button', { name: /Скасувати очікування/ })).toBeVisible();
});
test('a favorite that vanishes during auto-start keeps the original wait even after unstarring', async () => {
  await waitingFavorite();
  await fireEvent.click(screen.getByRole('button', { name: 'Прибрати з обраних' }));
  await waitFor(() => expect(native.api.save.mock.calls.at(-1)?.[0]).toMatchObject({ favoriteSources: [] }));
  native.api.sources.mockResolvedValueOnce([ghost]).mockResolvedValue([]);
  await fireEvent.click(screen.getByRole('button', { name: 'Оновити список програм' }));
  await waitFor(() => expect(native.api.sources.mock.calls.length).toBeGreaterThanOrEqual(4));
  expect(native.api.start).not.toHaveBeenCalled();
  expect(screen.getByText('Closed')).toBeVisible();
  expect(screen.getByRole('button', { name: /Скасувати очікування/ })).toBeVisible();
  native.api.sources.mockResolvedValue([ghost]);
  await fireEvent.click(screen.getByRole('button', { name: 'Оновити список програм' }));
  await waitFor(() => expect(native.api.start).toHaveBeenCalledWith(ghost.id));
});
test('a vanished former source keeps its captured label after the session ends', async () => {
  await loaded();
  await fireEvent.click(screen.getByRole('button', { name: /Почати переклад/ }));
  await waitFor(() => expect(native.api.start).toHaveBeenCalledWith(source.id));
  current = { ...idle(), sessionId: 1, phase: 'error', message: 'Немає аудіовиходу.' };
  native.api.sources.mockResolvedValue([]);
  await waitFor(() => expect(screen.getByRole('button', { name: 'Оновити список програм' })).toBeEnabled());
  await fireEvent.click(screen.getByRole('button', { name: 'Оновити список програм' }));
  expect(screen.getByText('Test')).toBeVisible();
  expect(screen.getByRole('button', { name: /Почати переклад/ })).toBeDisabled();
});
test('a finished session never resumes waiting and non-favorite unavailable still errors', async () => {
  await loaded();
  await fireEvent.click(screen.getByRole('button', { name: /Почати переклад/ }));
  await waitFor(() => expect(native.api.start).toHaveBeenCalledWith(source.id));
  current = { ...idle(), sessionId: 1, phase: 'error', message: 'Немає аудіовиходу.' };
  await waitFor(() => expect(screen.getByRole('button', { name: /Почати переклад/ })).toBeVisible());
  expect(screen.queryByRole('button', { name: /Скасувати очікування/ })).toBeNull();
  action({ kind: 'shortcut', value: 'app:/Missing.app' });
  await screen.findByText(/Програма недоступна для захоплення/);
  expect(native.api.start).toHaveBeenCalledTimes(1);
});
test('recording is refused while waiting and a native toggle still cancels without starting', async () => {
  await waitingFavorite();
  await fireEvent.click(screen.getByRole('button', { name: /Налаштування/ }));
  expect(screen.getByRole('button', { name: 'Перевірити з’єднання' })).toBeDisabled();
  await fireEvent.click(screen.getByRole('button', { name: 'Обрати гарячу клавішу' }));
  expect(await screen.findByText(/Не вдалося призупинити глобальну клавішу для запису/)).toBeVisible();
  expect(native.api.suspendShortcut).not.toHaveBeenCalled();
  expect(screen.queryByRole('button', { name: 'Натисніть комбінацію…' })).toBeNull();
  await waitFor(() => expect((native.api.syncDesktop.mock.calls.at(-1)?.[0] as { waiting: boolean }).waiting).toBe(true));
  action({ kind: 'toggle' });
  await waitFor(() => expect(native.api.stop).toHaveBeenCalled());
  await waitFor(() => expect((native.api.syncDesktop.mock.calls.at(-1)?.[0] as { waiting: boolean }).waiting).toBe(false));
  expect(native.api.start).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole('button', { name: /^◉/ }));
  expect(screen.queryByRole('button', { name: /Скасувати очікування/ })).toBeNull();
  expect(screen.getByRole('button', { name: /Почати переклад/ })).toBeVisible();
});
test('a rejected rediscovery keeps the armed wait and starts it exactly once later', async () => {
  await waitingFavorite();
  native.api.sources.mockResolvedValueOnce([ghost]).mockRejectedValueOnce(new Error('Список програм недоступний.')).mockResolvedValue([ghost]);
  await fireEvent.click(screen.getByRole('button', { name: 'Оновити список програм' }));
  expect(await screen.findByText(/Список програм недоступний/)).toBeVisible();
  expect(native.api.start).not.toHaveBeenCalled();
  expect(screen.getAllByText('Closed').length).toBeGreaterThan(0);
  expect(screen.getByRole('button', { name: /Скасувати очікування/ })).toBeVisible();
  await fireEvent.click(screen.getByRole('button', { name: 'Оновити список програм' }));
  await waitFor(() => expect(native.api.start).toHaveBeenCalledWith(ghost.id));
  expect(native.api.start).toHaveBeenCalledTimes(1);
  expect(native.api.checkConnection).not.toHaveBeenCalled();
});
test('Stop before a rejected rediscovery settles keeps the wait cancelled', async () => {
  await waitingFavorite();
  let reject!: (reason: Error) => void;
  native.api.sources.mockResolvedValueOnce([ghost]).mockImplementationOnce(() => new Promise<AudioSource[]>((_, fail) => { reject = fail; }));
  await fireEvent.click(screen.getByRole('button', { name: 'Оновити список програм' }));
  await waitFor(() => expect(native.api.sources.mock.calls.length).toBeGreaterThanOrEqual(4));
  action({ kind: 'stop' });
  reject(new Error('Список програм недоступний.'));
  await waitFor(() => expect(screen.queryByRole('button', { name: /Скасувати очікування/ })).toBeNull());
  expect(screen.getByRole('button', { name: /Почати переклад/ })).toBeVisible();
  expect(native.api.start).not.toHaveBeenCalled();
});
test('unstarring during the initial discovery keeps the explicit start and its label', async () => {
  native.api.load.mockResolvedValue({ ...defaults(), apiKey: 'test-key', selectedSourceId: ghost.id, favoriteSources: [{ id: ghost.id, name: 'Closed' }] });
  native.api.sources.mockResolvedValue([]);
  await loaded();
  let release!: (s: AudioSource[]) => void;
  native.api.sources.mockImplementationOnce(() => new Promise<AudioSource[]>(resolve => { release = resolve; }));
  await fireEvent.click(screen.getByRole('button', { name: /Почати переклад/ }));
  await waitFor(() => expect(native.api.sources.mock.calls.length).toBeGreaterThanOrEqual(2));
  await fireEvent.click(screen.getByRole('button', { name: 'Прибрати з обраних' }));
  await waitFor(() => expect(native.api.save.mock.calls.at(-1)?.[0]).toMatchObject({ favoriteSources: [] }));
  expect(screen.getByText('Closed')).toBeVisible();
  release([]);
  await screen.findByRole('button', { name: /Скасувати очікування/ });
  expect(native.api.start).not.toHaveBeenCalled();
  expect(screen.getByText('Closed')).toBeVisible();
  native.api.sources.mockResolvedValue([ghost]);
  await fireEvent.click(screen.getByRole('button', { name: 'Оновити список програм' }));
  await waitFor(() => expect(native.api.start).toHaveBeenCalledWith(ghost.id));
  expect(native.api.start).toHaveBeenCalledTimes(1);
});
test('Stop during the initial discovery with a later unstar never re-arms the wait', async () => {
  native.api.load.mockResolvedValue({ ...defaults(), apiKey: 'test-key', selectedSourceId: ghost.id, favoriteSources: [{ id: ghost.id, name: 'Closed' }] });
  native.api.sources.mockResolvedValue([]);
  await loaded();
  let release!: (s: AudioSource[]) => void;
  native.api.sources.mockImplementationOnce(() => new Promise<AudioSource[]>(resolve => { release = resolve; }));
  await fireEvent.click(screen.getByRole('button', { name: /Почати переклад/ }));
  await waitFor(() => expect(native.api.sources.mock.calls.length).toBeGreaterThanOrEqual(2));
  await fireEvent.click(screen.getByRole('button', { name: 'Прибрати з обраних' }));
  await waitFor(() => expect(native.api.save.mock.calls.at(-1)?.[0]).toMatchObject({ favoriteSources: [] }));
  action({ kind: 'stop' });
  release([]);
  await waitFor(() => expect(screen.getByRole('button', { name: /Почати переклад/ })).toBeVisible());
  expect(screen.queryByRole('button', { name: /Скасувати очікування/ })).toBeNull();
  expect(native.api.start).not.toHaveBeenCalled();
});
test('waiting is never persisted or restored after a restart', async () => {
  native.api.load.mockResolvedValue({ ...defaults(), apiKey: 'test-key', selectedSourceId: ghost.id, favoriteSources: [{ id: ghost.id, name: 'Closed' }] });
  native.api.sources.mockResolvedValue([]);
  await loaded();
  expect(screen.getByRole('button', { name: /Почати переклад/ })).toBeEnabled();
  expect(screen.queryByRole('button', { name: /Скасувати очікування/ })).toBeNull();
  expect(native.api.save).not.toHaveBeenCalled();
  expect(native.api.start).not.toHaveBeenCalled();
});
test('a preference refusal clears only on the next successful preference change', async () => {
  const full = Array.from({ length: 32 }, (_, index) => ({ id: `app:/Full${index}.app`, name: `Full ${index}` }));
  native.api.load.mockResolvedValue({ ...defaults(), apiKey: 'test-key', selectedSourceId: source.id, favoriteSources: full });
  await loaded();
  const refusal = 'Можна додати не більше 32 програм до обраних.';
  await fireEvent.click(screen.getByRole('button', { name: 'Додати до обраних' }));
  expect(await screen.findByText(refusal)).toBeVisible();
  await fireEvent.click(screen.getAllByRole('button', { name: 'Прибрати з обраних' })[0]);
  await waitFor(() => expect(screen.queryByText(refusal)).toBeNull());
  await waitFor(() => expect(native.api.save.mock.calls.at(-1)?.[0]).toMatchObject({ favoriteSources: expect.arrayContaining([{ id: 'app:/Full1.app', name: 'Full 1' }]) }));
  expect((native.api.save.mock.calls.at(-1)?.[0] as Settings).favoriteSources).toHaveLength(31);
});
test('a successful preference change never clears an unrelated error', async () => {
  const full = Array.from({ length: 32 }, (_, index) => ({ id: `app:/Full${index}.app`, name: `Full ${index}` }));
  native.api.load.mockResolvedValue({ ...defaults(), apiKey: 'test-key', selectedSourceId: source.id, favoriteSources: full });
  await loaded();
  const refusal = 'Можна додати не більше 32 програм до обраних.';
  await fireEvent.click(screen.getByRole('button', { name: 'Додати до обраних' }));
  expect(await screen.findByText(refusal)).toBeVisible();
  action({ kind: 'shortcut', value: 'app:/Missing.app' });
  expect(await screen.findByText(/Програма недоступна для захоплення/)).toBeVisible();
  await fireEvent.click(screen.getAllByRole('button', { name: 'Прибрати з обраних' })[0]);
  await waitFor(() => expect(native.api.save.mock.calls.at(-1)?.[0]).toMatchObject({ favoriteSources: expect.any(Array) }));
  expect(screen.getByText(/Програма недоступна для захоплення/)).toBeVisible();
});
test('starring persists and reorders the sidebar and tray model', async () => {
  native.api.sources.mockResolvedValue([source, { id: 'app:/Other.app', name: 'Other', active: true, detail: null }]);
  await loaded();
  await fireEvent.click(screen.getAllByRole('button', { name: 'Додати до обраних' })[1]);
  await waitFor(() => expect(native.api.save.mock.calls.at(-1)?.[0]).toMatchObject({ favoriteSources: [{ id: 'app:/Other.app', name: 'Other' }] }));
  await waitFor(() => expect((native.api.syncDesktop.mock.calls.at(-1)?.[0] as { sources: { id: string }[] }).sources[0].id).toBe('app:/Other.app'));
});
test('hiding the running app keeps session, star, label and stop control', async () => {
  native.api.load.mockResolvedValue({ ...defaults(), apiKey: 'test-key', selectedSourceId: source.id, favoriteSources: [{ id: source.id, name: source.name }] });
  await loaded();
  await fireEvent.click(screen.getByRole('button', { name: /Почати переклад/ }));
  await waitFor(() => expect(native.api.start).toHaveBeenCalledWith(source.id));
  await fireEvent.click(screen.getByRole('button', { name: 'Приховати' }));
  await waitFor(() => expect(native.api.save.mock.calls.at(-1)?.[0]).toMatchObject({ hiddenSources: [{ id: source.id, name: source.name }], favoriteSources: [{ id: source.id, name: source.name }] }));
  expect(native.api.stop).not.toHaveBeenCalled();
  expect(screen.getByText('Test')).toBeVisible();
  expect(screen.getByRole('button', { name: /Зупинити переклад/ })).toBeVisible();
  await fireEvent.click(screen.getByRole('button', { name: 'Приховані (1)' }));
  await fireEvent.click(screen.getByRole('button', { name: 'Повернути' }));
  await waitFor(() => expect(native.api.save.mock.calls.at(-1)?.[0]).toMatchObject({ hiddenSources: [] }));
});
test('a hidden selected source still resolves, and the tray cannot select it', async () => {
  native.api.load.mockResolvedValue({ ...defaults(), apiKey: 'test-key', selectedSourceId: source.id, hiddenSources: [{ id: source.id, name: source.name }] });
  await loaded();
  expect(screen.getByText('Test')).toBeVisible();
  action({ kind: 'source', value: source.id });
  expect(native.api.save).not.toHaveBeenCalled();
  await waitFor(() => expect((native.api.syncDesktop.mock.calls.at(-1)?.[0] as { sources: unknown[] }).sources).toEqual([]));
});
test('a size refusal shows one actionable message and never saves', async () => {
  const quotes = Array.from({ length: 31 }, () => ({ id: `app:/${'"'.repeat(500)}`, name: 'A' }));
  native.api.load.mockResolvedValue({ ...defaults(), apiKey: 'k'.repeat(4096), selectedSourceId: source.id, favoriteSources: quotes });
  native.api.sources.mockResolvedValue([source]);
  await loaded();
  await fireEvent.click(screen.getByRole('button', { name: 'Додати до обраних' }));
  expect(await screen.findByText('Список програм завеликий для збереження. Приберіть частину обраних або прихованих програм.')).toBeVisible();
  expect(native.api.save).not.toHaveBeenCalled();
});
test('a hidden application missing from discovery stays listed and restorable', async () => {
  const ghost = { id: 'app:/Closed.app', name: 'Closed', active: false, detail: null };
  native.api.load.mockResolvedValue({ ...defaults(), apiKey: 'test-key', selectedSourceId: source.id, hiddenSources: [{ id: ghost.id, name: ghost.name }] });
  await loaded();
  expect(screen.queryByText('Closed')).toBeNull();
  await fireEvent.click(screen.getByRole('button', { name: 'Приховані (1)' }));
  expect(screen.getByText('Closed')).toBeVisible();
  expect(screen.getByText('Програма недоступна')).toBeVisible();
  await fireEvent.click(screen.getByRole('button', { name: 'Повернути' }));
  await waitFor(() => expect(native.api.save.mock.calls.at(-1)?.[0]).toMatchObject({ hiddenSources: [] }));
});
test('preference controls stay usable during capture and lock while a check is pending', async () => {
  native.api.load.mockResolvedValue({ ...defaults(), apiKey: 'test-key', selectedSourceId: source.id, favoriteSources: [{ id: source.id, name: source.name }] });
  await loaded();
  await fireEvent.click(screen.getByRole('button', { name: /Почати переклад/ }));
  await waitFor(() => expect(native.api.start).toHaveBeenCalledWith(source.id));
  expect(screen.getByRole('button', { name: 'Прибрати з обраних' })).toBeEnabled();
  expect(screen.getByRole('button', { name: 'Приховати' })).toBeEnabled();
  native.api.checkConnection.mockImplementation(() => new Promise(() => {}));
  await fireEvent.click(screen.getByRole('button', { name: /Зупинити переклад/ }));
  await fireEvent.click(screen.getByRole('button', { name: /Налаштування/ }));
  await fireEvent.click(screen.getByRole('button', { name: 'Перевірити з’єднання' }));
  expect(screen.getByRole('button', { name: 'Прибрати з обраних' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Приховати' })).toBeDisabled();
});
