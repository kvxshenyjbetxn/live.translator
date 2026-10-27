<script lang="ts">
 import type { SourceView } from '../lib/sources';
 import { t } from '../lib/i18n';
 let { visible, hidden, selectedId, disabled, controlsDisabled = false, onSelect, onRefresh, onToggleFavorite, onHide, onRestore }: {
   visible: SourceView[]; hidden: SourceView[]; selectedId: string | null; disabled: boolean; controlsDisabled?: boolean;
   onSelect: (id: string) => void; onRefresh: () => void;
   onToggleFavorite: (view: SourceView) => void; onHide: (view: SourceView) => void; onRestore: (view: SourceView) => void;
 } = $props();
 let showHidden = $state(false);
 const status = (source: SourceView) => source.available ? (source.active ? 'Відтворює звук' : 'Доступна програма') : 'Програма недоступна';
</script>
<nav aria-label={$t('Програми')} class="sources">
  <div class="source-heading"><span>{$t('ДЖЕРЕЛО ЗВУКУ')}</span><button class="refresh" aria-label={$t('Оновити список програм')} {disabled} onclick={onRefresh}>↻</button></div>
  <div class="source-list">{#each visible as source (source.id)}
    <div class="source-row">
      <button class="source-item" class:selected={selectedId === source.id} {disabled} onclick={() => onSelect(source.id)} aria-pressed={selectedId === source.id} title={source.detail ? $t(source.detail) : source.name}><span class="app-icon">{source.name.slice(0, 1).toUpperCase()}</span><span class="source-name">{source.name}<small>{$t(status(source))}</small></span>{#if source.active}<i class="source-active" aria-label={$t('Активне джерело')}></i>{/if}</button>
      <div class="source-actions">
        <button class="source-toggle" aria-pressed={source.favorite} aria-label={$t(source.favorite ? 'Прибрати з обраних' : 'Додати до обраних')} disabled={controlsDisabled} onclick={() => onToggleFavorite(source)}>{source.favorite ? '★' : '☆'}</button>
        <button class="source-toggle" aria-label={$t('Приховати')} title={$t('Приховати')} disabled={controlsDisabled} onclick={() => onHide(source)}><svg class="source-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24" /><line x1="1" y1="1" x2="23" y2="23" /></svg></button>
      </div>
    </div>
  {:else}<p class="empty-sources">{$t('Немає доступних джерел. Увімкніть звук у потрібній програмі й оновіть список.')}</p>{/each}</div>
  {#if hidden.length}
    <button class="hidden-toggle" aria-expanded={showHidden} onclick={() => showHidden = !showHidden}>{$t('Приховані ({count})', { count: hidden.length })}</button>
    {#if showHidden}<div class="source-list hidden-list">{#each hidden as source (source.id)}
      <div class="source-row hidden-row">
        <div class="source-item"><span class="app-icon">{source.name.slice(0, 1).toUpperCase()}</span><span class="source-name">{source.name}<small>{$t(status(source))}</small></span></div>
        <button class="source-toggle" aria-label={$t('Повернути')} disabled={controlsDisabled} onclick={() => onRestore(source)}>↩</button>
      </div>
    {/each}</div>{/if}
  {/if}
</nav>
