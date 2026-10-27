<script lang="ts">
 import type { Settings, SessionStatus, Language } from '../types';
 import { t } from '../lib/i18n';
 import Diagnostics from './Diagnostics.svelte';
 let { settings, status, sourceName, languages, disabled = false, canStart, waiting = false, onStart, onStop, onChange }: { settings: Settings; status: SessionStatus; sourceName: string | null; languages: Language[]; disabled?: boolean; canStart: boolean; waiting?: boolean; onStart: () => void; onStop: () => void; onChange: (patch: Partial<Settings>) => void } = $props();
 let busy = $derived(['starting', 'running', 'stopping'].includes(status.phase));
 let transition = $derived(status.phase === 'starting' || status.phase === 'stopping');
 let level = $derived(Math.round(Math.max(0, Math.min(1, status.inputLevel)) * 100));
 let time = $derived(`${Math.floor(status.elapsedSeconds / 60).toString().padStart(2, '0')}:${(status.elapsedSeconds % 60).toString().padStart(2, '0')}`);
 let label = $derived(waiting ? 'Очікуємо на програму' : status.phase === 'starting' ? 'Підключаємося до Google…' : status.phase === 'stopping' ? 'Повертаємо оригінальний звук…' : status.phase === 'running' ? (status.receivedAudioSeconds > 0 ? 'Перекладаємо наживо' : 'Слухаємо · очікуємо переклад') : status.phase === 'error' ? 'Переклад зупинено' : 'Готовий до перекладу');
</script>
<section class="translation-view">
  <div class="page-heading"><p class="eyebrow">{$t('ЖИВИЙ ПЕРЕКЛАД')}</p><h1>{$t('Слухай своєю мовою')}</h1><p class="muted">{sourceName || $t('Виберіть програму зі списку ліворуч')}</p></div>
  <label class="field-label" for="target-language">{$t('Перекладати на')}</label><select id="target-language" value={settings.targetLanguage} disabled={disabled || busy} onchange={event => onChange({ targetLanguage: event.currentTarget.value })}>{#each languages as language}<option value={language.code}>{$t(language.name)}</option>{/each}</select>
  <section class:active-session={status.phase === 'running'} class="session-card" aria-label={$t('Стан перекладу')}>
    <div class="session-heading"><span class="session-label"><i class:live-dot={status.phase === 'running'}></i><span role="status">{$t(label)}</span></span><span class="timer">{time}</span></div>
    <div class="signal-display"><div class="signal-track" role="meter" aria-label={$t('Рівень вхідного аудіо')} aria-valuemin="0" aria-valuemax="100" aria-valuenow={level}><div style:width={`${level}%`}></div></div><span class="signal-value">{level}%</span></div>
    <div class="session-caption"><span>{$t('Реальний рівень джерела')}</span><span>Gemini Live Translate</span></div>
  </section>
  {#if status.phase === 'running'}<Diagnostics diagnostics={status.diagnostics} />{/if}
  <div class="mix-heading"><h2>{$t('Баланс звуку')}</h2><span>{$t('Незалежна гучність')}</span></div>
  <div class="slider-row"><label for="original-volume">{$t('Оригінал')}</label><input id="original-volume" type="range" min="0" max="100" step="1" value={Math.round(settings.originalVolume * 100)} {disabled} oninput={event => onChange({ originalVolume: Number(event.currentTarget.value) / 100 })}/><output for="original-volume">{Math.round(settings.originalVolume * 100)}%</output></div>
  <div class="slider-row"><label for="translation-volume">{$t('Переклад')}</label><input id="translation-volume" type="range" min="0" max="100" step="1" value={Math.round(settings.translationVolume * 100)} {disabled} oninput={event => onChange({ translationVolume: Number(event.currentTarget.value) / 100 })}/><output for="translation-volume">{Math.round(settings.translationVolume * 100)}%</output></div>
  <p class="mix-note">{$t('0% оригіналу — лише переклад. Оригінал звучить наживо, переклад — із невеликим відставанням. За потреби озвучка плавно прискорюється до 1.12×, щоб наздогнати.')}</p>
  {#if waiting}<button class="primary stop-button" {disabled} onclick={onStop}>■ &nbsp; {$t('Скасувати очікування')}</button>{:else if status.phase === 'running' || transition}<button class="primary stop-button" disabled={disabled || status.phase === 'stopping'} onclick={onStop}>■ &nbsp; {$t(status.phase === 'stopping' ? 'Зупиняємо…' : 'Зупинити переклад')}</button>{:else}<button class="primary" disabled={disabled || !canStart} onclick={onStart}>▶ &nbsp; {$t('Почати переклад')}</button>{/if}
  {#if waiting}<p class="privacy-note">{$t('Аудіо ще не передається Google: переклад почнеться, коли програма з’явиться.')}</p>{:else}<p class="privacy-note">{$t('↗ Аудіо вибраної програми передається Google')}</p>{/if}
  {#if waiting}<p class="help-note">{$t('Переклад почнеться автоматично, щойно програма з’явиться серед джерел. Програма не відкривається сама.')}</p>{:else if !busy && !settings.apiKey.trim()}<p class="help-note">{$t('Додайте API-ключ у налаштуваннях, щоб почати.')}</p>{:else if !busy && !sourceName}<p class="help-note">{$t('Виберіть джерело звуку. Якщо його немає — увімкніть звук у програмі й оновіть список.')}</p>{/if}
  {#if busy}<p class="help-note">{$t('Щоб змінити джерело або мову, зупиніть переклад. Зупинка повертає оригінал і відсікає хвіст озвучки.')}</p>{/if}
  {#if status.message}<div class="error" role="alert">{$t(status.message)}</div>{/if}
  {#if status.sentAudioSeconds > 0}<div class="session-metrics"><span>{$t('Передано {value} с', { value: status.sentAudioSeconds.toFixed(1) })}</span><span>{$t('Озвучка {value} с', { value: status.receivedAudioSeconds.toFixed(1) })}</span><span>{$t('Буфер {value} с', { value: status.playbackBacklogSeconds.toFixed(1) })}</span><span>{$t('Темп {value}×', { value: status.playbackSpeed.toFixed(2) })}</span></div>{/if}
</section>
