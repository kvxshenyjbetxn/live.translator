import type { Phase, Settings } from '../types';
export interface DesktopAction { kind: string; value?: string | null; error?: string | null }
export interface SaveResult { captureShortcut: string | null; shortcutError: string | null }
export interface ShortcutStatus { active: string | null; error: string | null; integrationError: string | null }
export interface MenuModel { revision: number; sources: { id: string; name: string }[]; selectedSourceId: string | null; targetLanguage: string; canSelect: boolean; canStart: boolean; canStop: boolean }
export interface DesktopContext { loaded: boolean; closing: boolean; checking: boolean; recording: boolean; starting: boolean; phase: Phase; settings: Settings }
type Decision = { kind: 'ignore' | 'stop' | 'refresh' } | { kind: 'start'; sourceId: string } | { kind: 'error'; message: string } | { kind: 'language' | 'source'; value: string };
export function decideDesktop(action: DesktopAction, c: DesktopContext): Decision {
  if (!c.loaded || c.closing || c.checking || c.recording || c.phase === 'stopping') return { kind: 'ignore' };
  const busy = c.starting || c.phase === 'starting' || c.phase === 'running';
  if (action.kind === 'stop') return { kind: 'stop' };
  if (action.kind === 'toggle' || action.kind === 'shortcut') {
    if (busy) return { kind: 'stop' };
    let sourceId: string | null | undefined;
    if (action.kind === 'toggle') sourceId = c.settings.selectedSourceId;
    else if (c.settings.shortcutMode === 'lastCaptured') sourceId = c.settings.lastCapturedSourceId;
    else {
      if (action.error) return { kind: 'error', message: action.error };
      sourceId = action.value;
    }
    return sourceId ? { kind: 'start', sourceId } : { kind: 'error', message: 'Програма для захоплення невідома. Виберіть джерело або спочатку запустіть переклад із вікна.' };
  }
  if (busy) return { kind: 'ignore' };
  if (action.kind === 'refresh') return { kind: 'refresh' };
  if ((action.kind === 'language' || action.kind === 'source') && action.value) return { kind: action.kind, value: action.value };
  return { kind: 'ignore' };
}
/** Coalesce semantic menu updates; never let older async completions replace newer state. */
export class MenuSync<T> {
  private desired: string | null = null;
  private saved: string | null = null;
  private writing: Promise<void> | null = null;
  constructor(private write: (model: T) => Promise<void>) {}
  async update(model: T): Promise<void> {
    this.desired = JSON.stringify(model);
    if (!this.writing) this.writing = this.drain().finally(() => { this.writing = null; });
    await this.writing;
    if (this.desired !== this.saved) await this.update(JSON.parse(this.desired!) as T);
  }
  private async drain(): Promise<void> {
    while (this.desired !== this.saved) {
      const value = this.desired!;
      await this.write(JSON.parse(value) as T);
      this.saved = value;
    }
  }
}
