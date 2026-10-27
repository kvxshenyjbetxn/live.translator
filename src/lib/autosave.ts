import type { Settings } from '../types';
export type SaveState = 'loading' | 'saved' | 'saving' | 'unsaved' | 'error';

const SAVE_FAILED = 'Не вдалося зберегти налаштування. Перевірте доступ до папки й повторіть.';
/**
 * Backend save failures are arbitrary text that can echo a path or the file it tried to write, so
 * only these fixed, user-actionable refusals are shown; everything else is masked.
 */
export const ACTIONABLE_REFUSALS = new Set([
  'Некоректні налаштування. Перевірте мову, гучність і список програм.',
  'Список програм завеликий для збереження. Приберіть частину обраних або прихованих програм.',
]);
function saveFailure(reason: unknown): Error {
  const text = typeof reason === 'string' ? reason : reason instanceof Error ? reason.message : '';
  return new Error(ACTIONABLE_REFUSALS.has(text) ? text : SAVE_FAILED);
}

/** Serial writes; immutable snapshots prevent old completions from replacing new edits. */
export class AutosaveStore {
  private desired: string | null = null;
  private saved: string | null = null;
  private writing: Promise<void> | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  constructor(private write: (settings: Settings) => Promise<void>, private changed: (state: SaveState) => void = () => {}, private delay = 350) {}
  init(settings: Settings): void {
    this.desired = this.saved = JSON.stringify(settings);
    this.changed('saved');
  }
  update(settings: Settings): void {
    if (this.desired === null) return;
    const snapshot = JSON.stringify(settings);
    if (snapshot === this.desired) return;
    this.desired = snapshot;
    this.cancelTimer();
    if (snapshot === this.saved && !this.writing) { this.changed('saved'); return; }
    this.changed(this.writing ? 'saving' : 'unsaved');
    this.timer = setTimeout(() => { this.timer = null; void this.flush().catch(() => {}); }, this.delay);
  }
  async flush(): Promise<void> {
    this.cancelTimer();
    if (this.desired === null) throw new Error('Налаштування ще не завантажено.');
    if (!this.writing) this.writing = this.drain().finally(() => { this.writing = null; });
    await this.writing;
    if (this.desired !== this.saved) await this.flush();
  }
  async retry(): Promise<void> { await this.flush(); }
  private cancelTimer(): void { if (this.timer !== null) { clearTimeout(this.timer); this.timer = null; } }
  private async drain(): Promise<void> {
    while (this.desired !== this.saved) {
      const snapshot = this.desired!;
      this.changed('saving');
      try { await this.write(JSON.parse(snapshot) as Settings); }
      catch (reason) { this.changed('error'); throw saveFailure(reason); }
      this.saved = snapshot;
    }
    this.changed('saved');
  }
}
