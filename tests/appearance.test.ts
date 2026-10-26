import { render, screen, fireEvent } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import { defaults, type Settings } from '../src/types';
import { normalizeAccent, accentText, applyAppearance } from '../src/lib/appearance';
import AppearanceSettings from '../src/components/AppearanceSettings.svelte';

test('HEX validation accepts only opaque full RGB colors', () => {
  expect(normalizeAccent(' #12ABef ')).toBe('#12abef');
  for (const value of ['red', '#fff', '#12345678', '#gg0000', '', '#12345é']) expect(normalizeAccent(value)).toBeNull();
});
test('black, white and middle accents have readable button text', () => {
  expect(accentText('#000000')).toBe('#ffffff');
  expect(accentText('#ffffff')).toBe('#000000');
  expect(accentText('#c5e89d')).toBe('#000000');
});
test('appearance updates root tokens immediately and can restore default theme', () => {
  const root = document.createElement('div');
  applyAppearance({ ...defaults(), theme: 'amoled', accentColor: '#000000' }, root);
  expect(root.dataset.theme).toBe('amoled');
  expect(root.style.getPropertyValue('--accent')).toBe('#000000');
  expect(root.style.getPropertyValue('--accent-text')).toBe('#ffffff');
  applyAppearance(defaults(), root);
  expect(root.dataset.theme).toBe('default');
  expect(root.style.getPropertyValue('--accent')).toBe('#c5e89d');
});
test('controls emit theme, language, palette and valid HEX patches only, and reset accent', async () => {
  const patches: Partial<Settings>[] = [];
  render(AppearanceSettings, { settings: defaults(), onChange: patch => patches.push(patch) });
  await fireEvent.change(screen.getByLabelText('Тема'), { target: { value: 'amoled' } });
  await fireEvent.change(screen.getByLabelText('Мова інтерфейсу'), { target: { value: 'en' } });
  await fireEvent.input(screen.getByLabelText('Колір акценту'), { target: { value: '#ff0088' } });
  const hex = screen.getByLabelText('HEX');
  await fireEvent.input(hex, { target: { value: '#12' } });
  expect(patches).toHaveLength(3);
  expect(hex).toHaveAttribute('aria-invalid', 'true');
  await fireEvent.input(hex, { target: { value: '#123ABC' } });
  await fireEvent.click(screen.getByRole('button', { name: 'Відновити типовий акцент' }));
  expect(patches).toEqual([{ theme: 'amoled' }, { interfaceLanguage: 'en' }, { accentColor: '#ff0088' }, { accentColor: '#123abc' }, { accentColor: '#c5e89d' }]);
});
