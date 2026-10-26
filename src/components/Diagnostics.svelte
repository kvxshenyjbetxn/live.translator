<script lang="ts">
  import type { DiagnosticsSnapshot } from '../types';
  import { t, locale } from '../lib/i18n';
  import { diagnosticMessages, formatMeasurement, projectHistory } from '../lib/diagnostics';
  let { diagnostics }: { diagnostics: DiagnosticsSnapshot } = $props();
  let rows = $derived.by(() => {
    const now = diagnostics.elapsedSeconds;
    const freshQueues = diagnostics.samples.length > 0 && now - diagnostics.samples.at(-1)!.elapsedSeconds <= 1.5;
    const configs = [
      { id: 'network', label: 'Відгук Google', unit: 'мс', color: '#8ebcd2', floor: 1000, value: diagnostics.networkRttMs,
        samples: diagnostics.networkSamples.map(s => ({ time: s.elapsedSeconds, value: s.rttMs })) },
      { id: 'sending', label: 'Черга надсилання', unit: 'с', color: 'var(--focus)', floor: 1, value: freshQueues ? diagnostics.sendingQueueSeconds : null,
        samples: diagnostics.samples.map(s => ({ time: s.elapsedSeconds, value: s.sendingQueueSeconds })) },
      { id: 'playback', label: 'Черга озвучення', unit: 'с', color: '#e6bc80', floor: 3, value: freshQueues ? diagnostics.playbackQueueSeconds : null,
        samples: diagnostics.samples.map(s => ({ time: s.elapsedSeconds, value: s.playbackQueueSeconds })) }
    ];
    return configs.map(row => {
      const visible = row.samples.filter(s => Number.isFinite(s.value) && (row.id === 'network' || (s.time >= now - 10 && s.time <= now))).slice(-101);
      const retained = row.id === 'network' ? visible.slice(-10) : visible;
      const max = Math.max(row.floor, ...retained.map(s => Math.ceil(s.value / row.floor) * row.floor));
      const graph = row.id === 'network'
        ? { paths: [], points: retained.map((s, i) => ({ x: (10 - retained.length + i + .5) * 10, y: 30 * (1 - Math.min(s.value / max, 1)) })) }
        : projectHistory(row.samples, now, max, false);
      return { ...row, max, graph,
        display: row.value === null ? $t('Немає даних') : formatMeasurement(row.value, row.unit, $locale) };
    });
  });
  let messages = $derived(diagnosticMessages(diagnostics));
</script>

<section class="diagnostics-panel" aria-label={$t('Діагностика затримки')}>
  <div class="diagnostics-heading"><h2>{$t('Діагностика затримки')}</h2><span>{$t('Черги · останні 10 с')}</span></div>
  <div class="diagnostics-charts">
    {#each rows as row (row.id)}
      <div class="diagnostics-row" aria-label={`${$t(row.label)}: ${row.display}`}>
        <div class="diagnostics-label"><h3>{$t(row.label)}</h3>{#if row.id === 'network'}<small>{$t('Останні 10 вимірювань')}</small>{/if}<small>{$t('Шкала: 0–{value}', { value: formatMeasurement(row.max, row.unit, $locale) })}</small></div>
        <svg class="diagnostics-chart" viewBox="-2 -3 104 36" preserveAspectRatio="none" aria-hidden="true" data-network-chart={row.id === 'network' ? '' : undefined}>
          <line x1="0" y1="0" x2="100" y2="0" class="diagnostics-grid" />
          <line x1="0" y1="15" x2="100" y2="15" class="diagnostics-grid" />
          <line x1="0" y1="30" x2="100" y2="30" class="diagnostics-grid" />
          {#each row.graph.paths as path}<path d={path} stroke={row.color} fill="none" stroke-width="1.5" vector-effect="non-scaling-stroke" />{/each}
          {#each row.graph.points as point}
            {#if row.id === 'network'}
              <rect x={point.x - 4} y={point.y} width="8" height={30 - point.y} rx="0.5" fill={row.color} />
            {:else}
              <circle cx={point.x} cy={point.y} r=".5" fill={row.color} />
            {/if}
          {/each}
        </svg>
        <span class="diagnostics-value" style:color={row.color}>{row.display}</span>
      </div>
    {/each}
    <div class="diagnostics-axis" aria-hidden="true"><span>{$t('−10 с')}</span><span>{$t('−5 с')}</span><span>{$t('зараз')}</span></div>
  </div>
  <div class="diagnostics-explanation" class:has-issues={diagnostics.issues.length > 0}>
    {#each messages as message}<p>{$t(message)}</p>{/each}
    {#if diagnostics.networkRttMs === null}<p>{$t('Відгук з’єднання ще невідомий: чекаємо відповіді на Ping. Її відсутність не доводить проблему з інтернетом.')}</p>{/if}
  </div>
  <p class="diagnostics-note">{$t('Ping → Pong — відгук з’єднання, не час перекладу. Кожен стовпчик — фактичне вимірювання приблизно раз на 4 с. Історія пінгу зсувається лише з новим результатом; давні стовпчики не означають свіжий відгук.')}</p>
  {#if diagnostics.sendDurationMs !== null}<p class="diagnostics-note">{$t('Останнє надсилання: {value} · завершення запису не означає обробку Google.', { value: formatMeasurement(diagnostics.sendDurationMs, 'мс', $locale) })}</p>{/if}
  <p class="diagnostics-note">{$t('Ці показники не вимірюють точне відставання від відео.')}</p>
</section>
