<script lang="ts">
  import type { Settings } from '../types';
  import type { SaveState } from '../lib/autosave';
  import type { ShortcutStatus } from '../lib/desktop';
  import ShortcutSettings from './ShortcutSettings.svelte';
  let { settings, directory, state: saveState, disabled = false, checking = false, connectionBlocked = false, connectionMessage = '', connectionError = false, onCheck = () => {}, shortcutStatus, onRecording, onChange, onRetry, onOpen }: { settings: Settings; directory: string; state: SaveState; disabled?: boolean; checking?: boolean; connectionBlocked?: boolean; connectionMessage?: string; connectionError?: boolean; onCheck?: () => void; shortcutStatus?: ShortcutStatus; onRecording?: (recording: boolean) => Promise<void>; onChange: (patch: Partial<Settings>) => void; onRetry: () => void; onOpen: () => void } = $props();
  let revealed = $state(false);
</script>
<section class="settings-view" aria-label="Налаштування">
  <div class="page-heading"><p class="eyebrow">ВАШ ПРОСТІР</p><h1>Налаштування</h1><p class="muted">Зміни зберігаються автоматично.</p></div>
  <div class="card">
    <h2>Підключення до Google</h2><p class="muted">Офіційний Gemini Live Translation API. Ваш ключ, ваші квоти.</p>
    <label class="field-label" for="api-key">Gemini API-ключ</label>
    <div class="key-input"><input id="api-key" type={revealed ? 'text' : 'password'} autocomplete="off" autocapitalize="off" spellcheck="false" maxlength="4096" value={settings.apiKey} disabled={disabled || checking} oninput={event => onChange({ apiKey: event.currentTarget.value })} placeholder="Вставте ключ із Google AI Studio"/><button class="quiet" {disabled} onclick={() => revealed = !revealed} aria-label={revealed ? 'Приховати ключ' : 'Показати ключ'}>{revealed ? 'Приховати' : 'Показати'}</button></div>
    <div class="connection-actions"><button class="secondary" disabled={disabled || checking || connectionBlocked || !settings.apiKey.trim()} onclick={onCheck}>{checking ? 'Перевіряємо…' : 'Перевірити з’єднання'}</button><span class="subtle">{connectionBlocked ? 'Спочатку зупиніть переклад' : 'Коротка Live-сесія без передавання аудіо'}</span></div>
    {#if connectionMessage}<p role="status" class="connection-result" class:check-error={connectionError}>{connectionMessage}</p>{/if}
    <div class="key-actions"><span class="subtle">{settings.apiKey.trim() ? 'Ключ додано · доступ перевіряється під час запуску' : 'Ключ ще не додано'}</span><button class="text-button" disabled={disabled || checking || !settings.apiKey} onclick={() => { revealed = false; onChange({ apiKey: '' }); }}>Видалити ключ</button></div>
    <div class="notice">Ключ зберігається відкритим текстом у локальному файлі. Не діліться цим файлом. Новий ключ застосовується з наступного запуску перекладу.</div>
    <p class="subtle">Безкоштовний рівень має ліміти. Програма не вмикає платний білінг; тариф залежить від вашого проєкту Google. На безкоштовному рівні Google може використовувати дані для покращення продуктів.</p>
  </div>
  <ShortcutSettings {settings} {disabled} {shortcutStatus} {onRecording} {onChange}/>
  <div class="card"><h2>Локальні налаштування</h2><p class="muted">Ключ, мова, джерело та баланс звуку відновляться після перезапуску. Переклад самостійно не запускається.</p><code class="directory">{directory || 'Директорія доступна в desktop-програмі'}</code><div class="settings-actions"><span class:save-error={saveState === 'error'} aria-live="polite">{saveState === 'error' ? 'Не збережено. Перевірте доступ до папки.' : saveState === 'saving' || saveState === 'unsaved' ? 'Зберігаємо…' : saveState === 'loading' ? 'Завантажуємо…' : 'Усі зміни збережено'}</span>{#if saveState === 'error'}<button onclick={onRetry}>Повторити збереження</button>{/if}<button class="secondary" disabled={disabled || !directory} onclick={onOpen}>Відкрити папку</button></div></div>
</section>
