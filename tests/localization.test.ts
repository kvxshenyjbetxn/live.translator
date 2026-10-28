import { render, screen, act } from '@testing-library/svelte';
import { afterEach, expect, test } from 'vitest';
import { locale, translate } from '../src/lib/i18n';
import messages from '../src/lib/messages.json';
import SettingsView from '../src/components/Settings.svelte';
import Translation from '../src/components/Translation.svelte';
import { defaults, idle } from '../src/types';
import { formatMeasurement } from '../src/lib/diagnostics';
import { ACTIONABLE_REFUSALS } from '../src/lib/autosave';

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
test('autosave refusal allowlist stays present in the shared catalog', () => {
  // Masked-vs-shown save errors depend on these exact Ukrainian strings being translated keys.
  const catalog: Record<string, string> = messages;
  expect(ACTIONABLE_REFUSALS.size).toBe(2);
  for (const refusal of ACTIONABLE_REFUSALS) expect(catalog[refusal], refusal).toBeTruthy();
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
test('translation dropdown displays and selects localized target languages', async () => {
  let selected = '';
  render(Translation, { settings: defaults(), status: idle(), sourceName: 'Chrome', languages: [
    { code: 'uk', name: 'Українська' }, { code: 'zh-Hans', name: 'Китайська (спрощена)' },
    { code: 'pt-BR', name: 'Португальська (Бразилія)' },
  ], canStart: true, onChange: patch => { selected = patch.targetLanguage ?? ''; }, onStart: () => {}, onStop: () => {} });
  expect(screen.getByRole('option', { name: 'Китайська (спрощена)' })).toHaveValue('zh-Hans');
  await act(() => locale.set('en'));
  expect(screen.getByRole('option', { name: 'Chinese (Simplified)' })).toHaveValue('zh-Hans');
  const select = screen.getByLabelText('Translate into') as HTMLSelectElement;
  select.value = 'pt-BR';
  await act(() => select.dispatchEvent(new Event('change', { bubbles: true })));
  expect(selected).toBe('pt-BR');
});
test('shared target languages put Ukrainian first and sort the rest in Ukrainian', () => {
  const files = import.meta.glob<string>('../crates/translator-core/src/settings.rs', { query: '?raw', import: 'default', eager: true });
  const source = Object.values(files)[0];
  const catalog = source.match(/pub const LANGUAGES: &\[Language\] = &\[([\s\S]*?)\n\];/);
  expect(catalog).not.toBeNull();
  const names = Array.from(catalog![1].matchAll(/name: "([^"]+)"/g), match => match[1]);
  expect(names).toHaveLength(78);
  expect(names[0]).toBe('Українська');
  const collator = new Intl.Collator('uk');
  expect(names.slice(1)).toEqual([...names.slice(1)].sort(collator.compare));
});
