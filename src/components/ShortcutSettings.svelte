<script lang="ts">
  import { onDestroy, tick } from 'svelte';
  import type { Settings } from '../types';
  import type { ShortcutStatus } from '../lib/desktop';
  import { shortcutFromKey } from '../lib/shortcut';
  let { settings, disabled = false, shortcutStatus = { active: null, error: null, integrationError: null }, onChange, onRecording = async () => {} }: { settings: Settings; disabled?: boolean; shortcutStatus?: ShortcutStatus; onChange: (patch: Partial<Settings>) => void; onRecording?: (recording: boolean) => Promise<void> } = $props();
  let recording = $state(false); let pending = $state(false); let message = $state('');
  let recorder: HTMLButtonElement;
  let candidate: { code: string; value: string } | null = null;
  let disposed = false; let ownsSuspension = false;
  async function begin() {
    pending = true; message = ''; candidate = null;
    ownsSuspension = true;
    try {
      await onRecording(true);
      if (disposed) { await onRecording(false); ownsSuspension = false; return; }
      recording = true; pending = false;
      // A disabled button cannot receive focus; flush its enabled state first.
      await tick(); if (!disposed) recorder?.focus();
    }
    catch { ownsSuspension = false; message = 'Не вдалося призупинити глобальну клавішу для запису.'; }
    finally { pending = false; }
  }
  async function finish(value?: string) {
    pending = true;
    try { await onRecording(false); recording = false; ownsSuspension = false; if (value && !disposed) onChange({ captureShortcut: value }); }
    catch { message = 'Не вдалося завершити запис. Спробуйте ще раз.'; }
    finally { pending = false; }
  }
  function key(event: KeyboardEvent) {
    if (!recording) return;
    event.preventDefault(); event.stopPropagation();
    if (pending) return;
    if (event.code === 'Escape') { void finish(); return; }
    const value = shortcutFromKey(event);
    if (value) { candidate = { code: event.code, value }; message = 'Відпустіть клавішу, щоб завершити запис.'; }
    else message = 'Натисніть модифікатор разом із клавішею (наприклад Ctrl + Shift + T).';
  }
  function released(event: KeyboardEvent) {
    if (!recording) return;
    event.preventDefault(); event.stopPropagation();
    if (candidate?.code === event.code && !pending) {
      const value = candidate.value; candidate = null; message = ''; void finish(value);
    }
  }
  onDestroy(() => { disposed = true; if (ownsSuspension) void onRecording(false).catch(() => {}); });
</script>
<svelte:window onkeydowncapture={key} onkeyupcapture={released}/>
<div class="card">
  <h2>Глобальна гаряча клавіша</h2>
  <p class="muted">Одне натискання запускає захоплення, повторне — зупиняє, навіть після переходу в інше вікно.</p>
  <label class="field-label" for="shortcut-mode">Дія гарячої клавіші</label>
  <select id="shortcut-mode" value={settings.shortcutMode} disabled={disabled || recording || pending} onchange={event => onChange({ shortcutMode: event.currentTarget.value as Settings['shortcutMode'] })}>
    <option value="activeApplication">Активна програма</option><option value="lastCaptured">Остання захоплена</option>
  </select>
  <p class="subtle">Активна програма — це програма переднього вікна в момент натискання, не окрема вкладка браузера. Live Translator не захоплює себе. На непідтримуваних системах оберіть останню захоплену програму.</p>
  <div class="settings-actions">
    <button bind:this={recorder} disabled={disabled || pending} onclick={() => { if (!recording) void begin(); }}>{recording ? 'Натисніть комбінацію…' : 'Обрати гарячу клавішу'}</button>
    <code>{settings.captureShortcut ?? 'Не задана'}</code>
    {#if recording}<button class="secondary" disabled={pending} onclick={() => { void finish(); }}>Скасувати запис</button>{/if}
    <button class="secondary" disabled={disabled || recording || pending || !settings.captureShortcut} onclick={() => onChange({ captureShortcut: null })}>Вимкнути гарячу клавішу</button>
  </div>
  <p class="subtle" aria-live="polite">{shortcutStatus.active ? `Активна комбінація: ${shortcutStatus.active}` : 'Глобальна клавіша вимкнена.'} {recording ? 'Під час запису захоплення клавішею призупинене. Escape — скасувати.' : ''}</p>
  {#if message || shortcutStatus.error}<p class="error" role="alert">{message || shortcutStatus.error} Попередня робоча комбінація залишається активною, якщо заміна не вдалася.</p>{/if}
  {#if shortcutStatus.integrationError}<p class="notice" role="status">{shortcutStatus.integrationError}</p>{/if}
</div>
