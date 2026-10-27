import { afterEach, expect, test, vi } from 'vitest';
import { AutosaveStore, type SaveState } from '../src/lib/autosave';
import { defaults } from '../src/types';
afterEach(() => vi.useRealTimers());

test('does not overwrite disk before initial load; rapid edits save the latest value', async () => {
  vi.useFakeTimers(); const saved: number[] = [];
  const store = new AutosaveStore(async s => { saved.push(s.originalVolume); });
  store.update({ ...defaults(), originalVolume: .5 });
  await vi.advanceTimersByTimeAsync(500); expect(saved).toEqual([]);
  store.init(defaults());
  store.update({ ...defaults(), originalVolume: .2 });
  store.update({ ...defaults(), originalVolume: .4 });
  await vi.advanceTimersByTimeAsync(350); expect(saved).toEqual([.4]);
});
test('serializes edits arriving during a write; flush waits for the last snapshot', async () => {
  let release!: () => void; const saved: number[] = []; let writing = 0; let max = 0;
  const store = new AutosaveStore(async s => {
    max = Math.max(max, ++writing); saved.push(s.originalVolume);
    if (saved.length === 1) await new Promise<void>(resolve => { release = resolve; });
    writing--;
  });
  store.init(defaults()); store.update({ ...defaults(), originalVolume: .2 });
  const first = store.flush();
  await Promise.resolve();
  store.update({ ...defaults(), originalVolume: .8 });
  const second = store.flush(); release();
  await Promise.all([first, second]);
  expect(saved).toEqual([.2, .8]); expect(max).toBe(1);
});
test('failed save stays unsaved and retry writes the current value, not an older value', async () => {
  let fail = true; const saved: number[] = []; const states: SaveState[] = [];
  const store = new AutosaveStore(async s => { if (fail) throw new Error('disk'); saved.push(s.translationVolume); }, state => states.push(state));
  store.init(defaults()); store.update({ ...defaults(), translationVolume: .3 });
  await expect(store.flush()).rejects.toThrow(); expect(states.at(-1)).toBe('error');
  fail = false; store.update({ ...defaults(), translationVolume: .9 }); await store.retry();
  expect(saved).toEqual([.9]); expect(states.at(-1)).toBe('saved');
});
async function failure(store: AutosaveStore): Promise<string> {
  try { await store.flush(); } catch (reason) { return reason instanceof Error ? reason.message : String(reason); }
  return 'saved';
}
test('actionable validation and size refusals survive a failed recovery save', async () => {
  const refusals = [
    'Некоректні налаштування. Перевірте мову, гучність і список програм.',
    'Список програм завеликий для збереження. Приберіть частину обраних або прихованих програм.',
  ];
  for (const refusal of refusals) {
    const store = new AutosaveStore(async () => { throw refusal; });
    store.init(defaults()); store.update({ ...defaults(), originalVolume: .2 });
    expect(await failure(store)).toBe(refusal);
  }
});
test('arbitrary backend save text is masked so keys and file content never surface', async () => {
  const store = new AutosaveStore(async () => { throw `write failed near {"apiKey":"AIza-test-only"} in /Users/test/settings.json`; });
  store.init(defaults()); store.update({ ...defaults(), originalVolume: .4 });
  const message = await failure(store);
  expect(message).toBe('Не вдалося зберегти налаштування. Перевірте доступ до папки й повторіть.');
  expect(message).not.toMatch(/AIza|apiKey|settings\.json/);
});
