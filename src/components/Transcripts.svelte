<script lang="ts">
  import { t } from '../lib/i18n';
  import type { Phase } from '../types';
  let { transcripts, phase, message }: { transcripts: { original: string; translated: string }; phase: Phase; message?: string | null } = $props();
  let originalPane: HTMLDivElement | undefined = $state();
  let translatedPane: HTMLDivElement | undefined = $state();
  let original = $derived(transcripts.original);
  let translated = $derived(transcripts.translated);
  let live = $derived(phase === 'running' || phase === 'starting');
  let followOriginal = true;
  let followTranslated = true;
  const nearBottom = (pane: HTMLDivElement) => pane.scrollHeight - pane.clientHeight - pane.scrollTop <= 32;
  $effect(() => { original; if (originalPane && followOriginal) originalPane.scrollTop = originalPane.scrollHeight; });
  $effect(() => { translated; if (translatedPane && followTranslated) translatedPane.scrollTop = translatedPane.scrollHeight; });
  let phaseLabel = $derived(phase === 'running' ? 'Перекладаємо наживо' : phase === 'starting' ? 'Підключаємося до Google…' : phase === 'stopping' ? 'Повертаємо оригінальний звук…' : phase === 'error' ? 'Переклад зупинено' : 'Готовий до перекладу');
</script>
<section class="transcripts-view">
  <div class="page-heading"><p class="eyebrow">{$t('ТЕКСТИ НАЖИВО')}</p><h1>{$t('Вхідний і вихідний текст')}</h1><p class="muted">{$t('Зліва — те, що говорить програма; справа — переклад, який озвучує Google.')}</p></div>
  <p class="muted" role="status">{$t(phaseLabel)}</p>
  {#if phase === 'error' && message}<div class="error" role="alert">{$t(message)}</div>{/if}
  <div class="transcript-columns">
    <div class="transcript-pane">
      <header><span class="pane-label">{$t('Оригінал')}</span><i class:live-dot={live}></i></header>
      <div class="pane-body" bind:this={originalPane} onscroll={event => followOriginal = nearBottom(event.currentTarget)} aria-live="off">{original || $t('Поки порожньо. Запустіть переклад, щоб бачити текст наживо.')}</div>
    </div>
    <div class="transcript-pane">
      <header><span class="pane-label">{$t('Переклад')}</span><i class:live-dot={live}></i></header>
      <div class="pane-body" bind:this={translatedPane} onscroll={event => followTranslated = nearBottom(event.currentTarget)} aria-live="off">{translated || $t('Поки порожньо. Запустіть переклад, щоб бачити текст наживо.')}</div>
    </div>
  </div>
  <p class="privacy-note">{$t('Тексти формує Google; вони залишаються лише в пам’яті після зупинки чи помилки та зникають після нового запуску або закриття програми.')}</p>
</section>
