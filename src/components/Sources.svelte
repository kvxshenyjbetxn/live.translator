<script lang="ts">
 import type { AudioSource } from '../types';
 let { sources, selected, disabled, onSelect, onRefresh }: { sources: AudioSource[]; selected: string | null; disabled: boolean; onSelect: (id: string) => void; onRefresh: () => void } = $props();
</script>
<nav aria-label="Програми" class="sources">
  <div class="source-heading"><span>ДЖЕРЕЛО ЗВУКУ</span><button class="refresh" aria-label="Оновити список програм" {disabled} onclick={onRefresh}>↻</button></div>
  <div class="source-list">{#each sources as source (source.id)}
    <button class="source-item" class:selected={selected === source.id} {disabled} onclick={() => onSelect(source.id)} aria-pressed={selected === source.id} title={source.detail ?? source.name}><span class="app-icon">{source.name.slice(0, 1).toUpperCase()}</span><span class="source-name">{source.name}<small>{source.active ? 'Відтворює звук' : 'Доступна програма'}</small></span>{#if source.active}<i class="source-active" aria-label="Активне джерело"></i>{/if}</button>
  {:else}<p class="empty-sources">Немає доступних джерел. Увімкніть звук у потрібній програмі й оновіть список.</p>{/each}</div>
</nav>
