interface CloseActions { begin: () => Promise<void>; flush: () => Promise<void>; stop: () => Promise<void>; finish: () => Promise<void>; cancel: () => Promise<void> }
export class CloseCoordinator {
  private pending: Promise<void> | null = null;
  constructor(private actions: CloseActions) {}
  request(): Promise<void> { return this.run(true); }
  discard(): Promise<void> { return this.run(false); }
  async cancel(): Promise<void> { if (this.pending) await this.pending.catch(() => {}); await this.actions.cancel(); }
  private run(save: boolean): Promise<void> {
    if (this.pending) return this.pending;
    this.pending = (async () => {
      await this.actions.begin();
      if (save) await this.actions.flush();
      await this.actions.stop();
      await this.actions.finish();
    })().finally(() => { this.pending = null; });
    return this.pending;
  }
}
