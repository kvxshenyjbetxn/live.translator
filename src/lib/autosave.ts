import type { Settings } from '../types';
export type SaveState = 'loading' | 'saved' | 'saving' | 'unsaved' | 'error';

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
      catch { this.changed('error'); throw new Error('Не вдалося зберегти налаштування. Перевірте доступ до папки й повторіть.'); }
      this.saved = snapshot;
    }
    this.changed('saved');
  }
}
