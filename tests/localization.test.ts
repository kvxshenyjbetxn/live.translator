import { render, screen, act } from '@testing-library/svelte';
import { afterEach, expect, test } from 'vitest';
import { locale, translate } from '../src/lib/i18n';
import messages from '../src/lib/messages.json';
import SettingsView from '../src/components/Settings.svelte';
import Translation from '../src/components/Translation.svelte';
import { defaults, idle } from '../src/types';
import { formatMeasurement } from '../src/lib/diagnostics';

afterEach(() => locale.set('uk'));
test('switches existing accessible UI and status text without changing audio language', async () => {
  const settings = { ...defaults(), targetLanguage: 'de' };
  render(SettingsView, { settings, directory: '/test', state: 'saved', onChange: () => {}, onRetry: () => {}, onOpen: () => {} });
  expect(screen.getByLabelText('Мова інтерфейсу')).toBeVisible();
  await act(() => locale.set('en'));
  expect(screen.getByLabelText('Interface language')).toBeVisible();
  expect(screen.getByRole('button', { name: 'Check connection' })).toBeVisible();
  expect(screen.getByText('All changes saved')).toBeVisible();
  expect(settings.targetLanguage).toBe('de');
  await act(() => locale.set('uk'));
  expect(screen.getByLabelText('Мова інтерфейсу')).toBeVisible();
});
test('translates running diagnostics, units and structured provider errors but preserves opaque errors', async () => {
  locale.set('en');
  render(Translation, { settings: defaults(), status: { ...idle(), phase: 'running', message: 'Немає аудіовиходу.' }, sourceName: 'Chrome', languages: [], canStart: true, onChange: () => {}, onStart: () => {}, onStop: () => {} });
  expect(screen.getByLabelText('Input audio level')).toBeVisible();
  expect(screen.getByRole('heading', { name: 'Latency diagnostics' })).toBeVisible();
  expect(screen.getByRole('alert')).toHaveTextContent('No audio output.');
  expect(formatMeasurement(1234, 'мс', 'en')).toBe('1,234 ms');
  expect(translate('Google тимчасово не може обробити запит (код 503). Це не обов\'язково ліміт; повторіть пізніше.', 'en')).toContain('(code 503)');
  expect(translate("Збій з'єднання з Google: підключення — час очікування вичерпано. Спробуйте запустити знову.", 'en')).toBe('Google connection failed: connection — timed out. Try starting again.');
  expect(translate('WebSocket error: opaque external response', 'en')).toBe('WebSocket error: opaque external response');
});
test('shared catalog covers all native Ukrainian user-facing string literals', () => {
  // Pin completeness for native messages, not arbitrary external error payloads.
  const files = import.meta.glob<string>('../{crates/translator-core,src-tauri}/src/**/*.rs', { query: '?raw', import: 'default', eager: true });
  expect(Object.keys(files).length).toBeGreaterThan(15);
  for (const [path, text] of Object.entries(files)) {
    for (const match of text.matchAll(/"([^"\n]*[А-Яа-яІіЇїЄєҐґ][^"\n]*)"/g)) {
      const value = match[1];
      if (value.startsWith('Live Translator · ')) continue;
      expect(messages, `${path}: ${value}`).toHaveProperty(value);
    }
  }
});
