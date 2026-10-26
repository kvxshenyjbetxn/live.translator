import type { DiagnosticsSnapshot } from '../types';
import { translate, type Locale } from './i18n';

interface Measurement { time: number; value: number | null }
interface Point { x: number; y: number }

/** Coordinates in a 100×30 view, using only actual observations in the last 10 s. */
export function projectHistory(samples: Measurement[], now: number, maximum: number, dotsOnly: boolean): { points: Point[]; paths: string[] } {
  const points: Point[] = [];
  const paths: string[] = [];
  if (!Number.isFinite(now) || !Number.isFinite(maximum) || maximum <= 0) return { points, paths };
  let segment = '';
  let previousTime: number | null = null;
  const flush = () => { if (segment) paths.push(segment); segment = ''; previousTime = null; };
  for (const sample of samples) {
    if (!Number.isFinite(sample.time) || sample.time < now - 10 || sample.time > now || sample.value === null || !Number.isFinite(sample.value) || sample.value < 0) { flush(); continue; }
    if (previousTime !== null && (sample.time - previousTime > .4 || sample.time <= previousTime)) flush();
    const point = { x: (sample.time - now + 10) * 10, y: 30 * (1 - Math.min(sample.value / maximum, 1)) };
    points.push(point);
    if (!dotsOnly) segment += `${segment ? ' L' : 'M'}${point.x.toFixed(2)},${point.y.toFixed(2)}`;
    previousTime = sample.time;
  }
  flush();
  return { points, paths };
}

export function diagnosticMessages(diagnostics: DiagnosticsSnapshot): string[] {
  const messages = diagnostics.issues.map(issue => ({
    networkUnstable: 'Відгук з’єднання з Google високий або нестабільний. Це може додавати затримку, але не доводить низьку швидкість інтернету.',
    sendingQueue: 'Захоплене аудіо накопичується: надсилання не встигає за джерелом. Причиною може бути з’єднання або навантаження програми.',
    playbackQueue: 'Отримана озвучка накопичується перед відтворенням. Програма плавно прискорює її, щоб зменшити чергу.'
  })[issue]);
  if (messages.length) return messages;
  const samples = diagnostics.samples;
  if (samples.length < 2 || samples.at(-1)!.elapsedSeconds - samples[0].elapsedSeconds < 2) return ['Збираємо вимірювання. Пауза в озвучці сама по собі не означає несправність.'];
  return ['Значного накопичення в локальних чергах не виявлено. Ці вимірювання не визначають час роботи моделі.'];
}

export const formatMeasurement = (value: number, unit: string, language: Locale = 'uk') => `${value.toLocaleString(language === 'en' ? 'en-US' : 'uk-UA', { maximumFractionDigits: unit === 'мс' ? 0 : 2 })} ${translate(unit, language)}`;
