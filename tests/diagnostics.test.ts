import { render, screen, fireEvent } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import { projectHistory, diagnosticMessages } from '../src/lib/diagnostics';
import Diagnostics from '../src/components/Diagnostics.svelte';
import Translation from '../src/components/Translation.svelte';
import { defaults, idle, type DiagnosticsSnapshot } from '../src/types';

const snapshot = (patch: Partial<DiagnosticsSnapshot> = {}): DiagnosticsSnapshot => ({
  elapsedSeconds: 12, samples: [], networkSamples: [], networkRttMs: null, sendDurationMs: null,
  sendingQueueSeconds: 0, playbackQueueSeconds: 0, issues: [], ...patch
});

test('history projects real timestamps in the last ten seconds and clips expired/invalid values', () => {
  const graph = projectHistory([{ time: 1, value: 50 }, { time: 2, value: 20 }, { time: 12, value: 100 }, { time: 13, value: 10 }, { time: 8, value: NaN }], 12, 100, true);
  expect(graph.points).toEqual([{ x: 0, y: 24 }, { x: 100, y: 0 }]);
  expect(graph.paths).toEqual([]); // sparse RTT is dots only, never a continuous fabricated curve
});

test('missing measurements and queue sampling gaps split paths; empty/single graphs stay finite', () => {
  expect(projectHistory([], 10, 1, false)).toEqual({ points: [], paths: [] });
  const graph = projectHistory([{ time: 9, value: .2 }, { time: 9.1, value: null }, { time: 9.2, value: .3 }, { time: 10, value: .4 }], 10, 1, false);
  expect(graph.paths).toHaveLength(3);
  expect(graph.paths.join(' ')).not.toMatch(/NaN|Infinity/);
  expect(graph.points).toHaveLength(3);
});

test('multiple causes are explained without treating high RTT as proven slow internet', () => {
  const messages = diagnosticMessages(snapshot({ issues: ['networkUnstable', 'sendingQueue', 'playbackQueue'] }));
  expect(messages).toHaveLength(3);
  expect(messages.join(' ')).toMatch(/може/);
  expect(messages.join(' ')).toMatch(/не встигає/);
  expect(messages.join(' ')).toMatch(/накопичується/);
  expect(diagnosticMessages(snapshot()).join(' ')).not.toMatch(/повільний інтернет|Google повільно/);
});

test('compact panel labels independent units, unknown network and disclaimer accessibly', async () => {
  const view = render(Diagnostics, { diagnostics: snapshot() });
  expect(screen.getByRole('region', { name: 'Діагностика затримки' })).toBeVisible();
  expect(screen.getAllByText('Немає даних')).toHaveLength(3);
  expect(screen.getByText('Відгук Google')).toBeVisible();
  expect(screen.getByText('Черга надсилання')).toBeVisible();
  expect(screen.getByText('Черга озвучення')).toBeVisible();
  expect(screen.getByText('Ці показники не вимірюють точне відставання від відео.')).toBeVisible();
  expect(screen.getAllByText(/Шкала:/)).toHaveLength(3);
  expect(screen.getByText(/Відгук з’єднання ще невідомий/)).toBeVisible();
  await view.rerender({ diagnostics: snapshot({ networkRttMs: 85, networkSamples: [{ elapsedSeconds: 11, rttMs: 85 }], samples: [{ elapsedSeconds: 12, sendingQueueSeconds: .05, playbackQueueSeconds: .2 }] }) });
  expect(screen.queryByText('Немає даних')).not.toBeInTheDocument();
  expect(screen.getByLabelText(/Відгук Google: 85 мс/)).toBeVisible();
  expect(view.container.querySelectorAll('[data-network-chart] rect')).toHaveLength(1);
  expect(view.container.querySelectorAll('[data-network-chart] circle')).toHaveLength(0);
  expect(view.container.querySelectorAll('[data-network-chart] path')).toHaveLength(0);
  await view.rerender({ diagnostics: snapshot() });
  expect(view.container.querySelectorAll('circle, rect')).toHaveLength(0);
});

test('each real ping adds a packed bar; bars move only on a new measurement', async () => {
  const view = render(Diagnostics, { diagnostics: snapshot({
    networkRttMs: 200,
    networkSamples: [{ elapsedSeconds: 0, rttMs: 900 }, { elapsedSeconds: 4, rttMs: 100 }, { elapsedSeconds: 8, rttMs: 300 }, { elapsedSeconds: 12, rttMs: 200 }],
    samples: [{ elapsedSeconds: 11.8, sendingQueueSeconds: .1, playbackQueueSeconds: .2 }, { elapsedSeconds: 12, sendingQueueSeconds: .2, playbackQueueSeconds: .4 }]
  }) });
  const bars = view.container.querySelectorAll('[data-network-chart] rect');
  expect(bars).toHaveLength(4); // sample history is count-based, not a rolling time axis
  expect(Number(bars[1].getAttribute('height'))).toBeCloseTo(3);
  expect(Number(bars[2].getAttribute('height'))).toBeCloseTo(9);
  expect(Number(bars[3].getAttribute('height'))).toBeCloseTo(6);
  expect(Number(bars[1].getAttribute('x')) - Number(bars[0].getAttribute('x'))).toBe(10);
  expect(Number(bars[0].getAttribute('width'))).toBe(8);
  const firstX = Number(bars[0].getAttribute('x'));
  const before = Array.from(bars, b => b.outerHTML);
  await view.rerender({ diagnostics: snapshot({ elapsedSeconds: 18, networkRttMs: 200, networkSamples: [{ elapsedSeconds: 0, rttMs: 900 }, { elapsedSeconds: 4, rttMs: 100 }, { elapsedSeconds: 8, rttMs: 300 }, { elapsedSeconds: 12, rttMs: 200 }], samples: [{ elapsedSeconds: 18, sendingQueueSeconds: .2, playbackQueueSeconds: .4 }] }) });
  expect(Array.from(view.container.querySelectorAll('[data-network-chart] rect'), b => b.outerHTML)).toEqual(before);
  await view.rerender({ diagnostics: snapshot({ elapsedSeconds: 20, networkRttMs: 100, networkSamples: [{ elapsedSeconds: 0, rttMs: 900 }, { elapsedSeconds: 4, rttMs: 100 }, { elapsedSeconds: 8, rttMs: 300 }, { elapsedSeconds: 12, rttMs: 200 }, { elapsedSeconds: 20, rttMs: 100 }], samples: [{ elapsedSeconds: 20, sendingQueueSeconds: .2, playbackQueueSeconds: .4 }] }) });
  const after = view.container.querySelectorAll('[data-network-chart] rect');
  expect(after).toHaveLength(5);
  expect(Number(after[0].getAttribute('x'))).toBe(firstX - 10);
  expect(view.container.querySelectorAll('[data-network-chart] path, [data-network-chart] circle')).toHaveLength(0);
  expect(view.container.querySelectorAll('svg:not([data-network-chart]) path')).toHaveLength(2);
  expect(view.container.querySelectorAll('svg:not([data-network-chart]) rect')).toHaveLength(0);
});

test('startup connects immediately and remains cancellable', async () => {
  let stopped = false;
  const view = render(Translation, { settings: defaults(), status: { ...idle(), phase: 'starting' }, sourceName: 'Chrome', languages: [], canStart: true, onStart: () => {}, onStop: () => { stopped = true; }, onChange: () => {} });
  expect(screen.getByText('Підключаємося до Google…')).toHaveAttribute('role', 'status');
  expect(screen.queryByText(/перейдіть у програму/)).not.toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button', { name: /Зупинити переклад/ }));
  expect(stopped).toBe(true);
  await view.rerender({ status: { ...idle(), phase: 'starting' } });
  expect(screen.getByText('Підключаємося до Google…')).toHaveAttribute('role', 'status');
});

test('playback tempo stays visible at normal speed and while catch-up changes', async () => {
  const status = { ...idle(), phase: 'running' as const, sentAudioSeconds: 1 };
  const view = render(Translation, { settings: defaults(), status, sourceName: 'Chrome', languages: [], canStart: true, onStart: () => {}, onStop: () => {}, onChange: () => {} });
  const tempo = screen.getByText('Темп 1.00×');
  expect(tempo).toBeVisible();
  await view.rerender({ status: { ...status, playbackSpeed: 1.08 } });
  expect(tempo).toHaveTextContent('Темп 1.08×');
  await view.rerender({ status: { ...status, playbackSpeed: 1.001 } });
  expect(tempo).toHaveTextContent('Темп 1.00×');
  expect(tempo).toBeVisible();
});

test('panel appears only in running sessions and does not replace audio controls', async () => {
  const view = render(Translation, { settings: defaults(), status: idle(), sourceName: 'Chrome', languages: [], canStart: true, onStart: () => {}, onStop: () => {}, onChange: () => {} });
  expect(screen.queryByRole('region', { name: 'Діагностика затримки' })).not.toBeInTheDocument();
  await view.rerender({ status: { ...idle(), phase: 'running' } });
  expect(screen.getByRole('region', { name: 'Діагностика затримки' })).toBeVisible();
  expect(screen.getByRole('slider', { name: 'Оригінал' })).toBeEnabled();
  await view.rerender({ status: idle() });
  expect(screen.queryByRole('region', { name: 'Діагностика затримки' })).not.toBeInTheDocument();
});
