<script lang="ts">
 import type { AudioSource } from '../types';
 import { t } from '../lib/i18n';
 let { sources, selected, disabled, onSelect, onRefresh }: { sources: AudioSource[]; selected: string | null; disabled: boolean; onSelect: (id: string) => void; onRefresh: () => void } = $props();
</script>
<nav aria-label={$t('Програми')} class="sources">
  <div class="source-heading"><span>{$t('ДЖЕРЕЛО ЗВУКУ')}</span><button class="refresh" aria-label={$t('Оновити список програм')} {disabled} onclick={onRefresh}>↻</button></div>
  <div class="source-list">{#each sources as source (source.id)}
    <button class="source-item" class:selected={selected === source.id} {disabled} onclick={() => onSelect(source.id)} aria-pressed={selected === source.id} title={source.detail ? $t(source.detail) : source.name}><span class="app-icon">{source.name.slice(0, 1).toUpperCase()}</span><span class="source-name">{source.name}<small>{$t(source.active ? 'Відтворює звук' : 'Доступна програма')}</small></span>{#if source.active}<i class="source-active" aria-label={$t('Активне джерело')}></i>{/if}</button>
  {:else}<p class="empty-sources">{$t('Немає доступних джерел. Увімкніть звук у потрібній програмі й оновіть список.')}</p>{/each}</div>
</nav>
