import { expect, test } from 'vitest';
import { decideDesktop, MenuSync, type DesktopContext } from '../src/lib/desktop';
import { defaults } from '../src/types';
const context = (patch: Partial<DesktopContext> = {}): DesktopContext => ({ loaded: true, closing: false, checking: false, recording: false, starting: false, waiting: false, phase: 'idle', settings: defaults(), ...patch });
test('active shortcut uses its keypress snapshot, last mode uses remembered stable identity', () => {
  expect(decideDesktop({ kind: 'shortcut', value: 'app:/Active.app' }, context())).toEqual({ kind: 'start', sourceId: 'app:/Active.app' });
  expect(decideDesktop({ kind: 'shortcut', value: 'app:/Active.app' }, context({ settings: { ...defaults(), shortcutMode: 'lastCaptured', lastCapturedSourceId: 'app:/Last.app' } }))).toEqual({ kind: 'start', sourceId: 'app:/Last.app' });
  expect(decideDesktop({ kind: 'toggle' }, context({ settings: { ...defaults(), selectedSourceId: 'app:/Selected.app' } }))).toEqual({ kind: 'start', sourceId: 'app:/Selected.app' });
});
test('Windows shortcut snapshot and foreground errors preserve last-captured behavior', () => {
  const browserId = String.raw`exe:c:\apps\browser.exe`;
  const lastId = String.raw`exe:c:\apps\last.exe`;
  expect(browserId).toContain('\\');
  expect(browserId).not.toContain('\b');
  const last = context({ settings: { ...defaults(), shortcutMode: 'lastCaptured', lastCapturedSourceId: lastId } });
  expect(decideDesktop({ kind: 'shortcut', value: browserId }, context())).toEqual({ kind: 'start', sourceId: browserId });
  expect(decideDesktop({ kind: 'shortcut', error: 'unavailable' }, last)).toEqual({ kind: 'start', sourceId: lastId });
  expect(decideDesktop({ kind: 'shortcut', error: 'unavailable' }, context())).toEqual({ kind: 'error', message: 'unavailable' });
});
test('errors and missing last application never fall back to selected source', () => {
  const c = context({ settings: { ...defaults(), selectedSourceId: 'app:/Selected.app' } });
  expect(decideDesktop({ kind: 'shortcut', error: 'self' }, c)).toEqual({ kind: 'error', message: 'self' });
  expect(decideDesktop({ kind: 'shortcut' }, c).kind).toBe('error');
  expect(decideDesktop({ kind: 'shortcut', value: 'app:/Active.app' }, { ...c, settings: { ...c.settings, shortcutMode: 'lastCaptured' } }).kind).toBe('error');
});
test('toggle always stops active or pending capture even with foreground failure', () => {
  for (const phase of ['running', 'starting'] as const) expect(decideDesktop({ kind: 'shortcut', error: 'unknown' }, context({ phase }))).toEqual({ kind: 'stop' });
  expect(decideDesktop({ kind: 'shortcut' }, context({ starting: true }))).toEqual({ kind: 'stop' });
  expect(decideDesktop({ kind: 'stop' }, context())).toEqual({ kind: 'stop' });
});
test('stopping, startup, shutdown, connection check and recorder ignore native actions', () => {
  for (const patch of [{ phase: 'stopping' as const }, { loaded: false }, { closing: true }, { checking: true }, { recording: true }]) {
    expect(decideDesktop({ kind: 'shortcut', value: 'app:/Active.app' }, context(patch))).toEqual({ kind: 'ignore' });
  }
});
test('waiting is cancelled by toggle and shortcut and ignores refresh', () => {
  const c = context({ waiting: true });
  expect(decideDesktop({ kind: 'toggle' }, c)).toEqual({ kind: 'stop' });
  expect(decideDesktop({ kind: 'shortcut', value: 'app:/A.app' }, c)).toEqual({ kind: 'stop' });
  expect(decideDesktop({ kind: 'refresh' }, c)).toEqual({ kind: 'ignore' });
  expect(decideDesktop({ kind: 'source', value: 'app:/A.app' }, c)).toEqual({ kind: 'ignore' });
});
test('source and language menu changes are locked during capture', () => {
  expect(decideDesktop({ kind: 'source', value: 'app:/A.app' }, context({ phase: 'running' }))).toEqual({ kind: 'ignore' });
  expect(decideDesktop({ kind: 'language', value: 'uk' }, context())).toEqual({ kind: 'language', value: 'uk' });
});
test('menu synchronization serializes updates and keeps the newest state', async () => {
  const written: string[] = [];
  let release!: () => void;
  const sync = new MenuSync<string>(async value => { written.push(value); if (value === 'first') await new Promise<void>(resolve => { release = resolve; }); });
  const first = sync.update('first');
  void sync.update('second'); const last = sync.update('third');
  release(); await first; await last;
  expect(written).toEqual(['first', 'third']);
  await sync.update('third'); expect(written).toHaveLength(2);
});
