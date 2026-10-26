<script lang="ts">
  import type { Settings } from '../types';
  import { t } from '../lib/i18n';
  import { DEFAULT_ACCENT, normalizeAccent } from '../lib/appearance';
  let { settings, disabled = false, onChange }: { settings: Settings; disabled?: boolean; onChange: (patch: Partial<Settings>) => void } = $props();
  let draft = $state('');
  $effect(() => { draft = settings.accentColor; });
  let invalid = $derived(normalizeAccent(draft) === null);
  function hexInput(value: string) {
    draft = value;
    const color = normalizeAccent(value);
    if (color) onChange({ accentColor: color });
  }
</script>
<div class="card appearance-settings">
  <h2>{$t('Вигляд і мова')}</h2>
  <p class="muted">{$t('Зміни застосовуються одразу та зберігаються автоматично.')}</p>
  <label class="field-label" for="theme">{$t('Тема')}</label>
  <select id="theme" value={settings.theme} {disabled} onchange={event => onChange({ theme: event.currentTarget.value as Settings['theme'] })}>
    <option value="default">{$t('Поточна темна')}</option><option value="amoled">{$t('Чорна AMOLED')}</option>
  </select>
  <label class="field-label" for="accent-color">{$t('Колір акценту')}</label>
  <div class="accent-controls">
    <input id="accent-color" type="color" value={settings.accentColor} {disabled} oninput={event => { draft = event.currentTarget.value; onChange({ accentColor: draft }); }}/>
    <label class="field-label" for="accent-hex">HEX</label>
    <input id="accent-hex" type="text" value={draft} aria-invalid={invalid} aria-describedby={invalid ? 'accent-error' : undefined} spellcheck="false" autocomplete="off" maxlength="7" {disabled} oninput={event => hexInput(event.currentTarget.value)}/>
    <button class="secondary" {disabled} onclick={() => { draft = DEFAULT_ACCENT; onChange({ accentColor: DEFAULT_ACCENT }); }}>{$t('Відновити типовий акцент')}</button>
  </div>
  {#if invalid}<p id="accent-error" class="save-error subtle" role="status">{$t('Введіть колір у форматі #RRGGBB.')}</p>{/if}
  <label class="field-label" for="interface-language">{$t('Мова інтерфейсу')}</label>
  <select id="interface-language" value={settings.interfaceLanguage} {disabled} onchange={event => onChange({ interfaceLanguage: event.currentTarget.value as Settings['interfaceLanguage'] })}>
    <option value="uk">Українська</option><option value="en">English</option>
  </select>
  <p class="subtle">{$t('Не змінює мову перекладу аудіо.')}</p>
</div>
