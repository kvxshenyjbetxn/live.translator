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
