import { render, screen, act } from '@testing-library/svelte';
import { afterEach, expect, test } from 'vitest';
import { locale } from '../src/lib/i18n';
import Transcripts from '../src/components/Transcripts.svelte';

afterEach(() => locale.set('uk'));
test('shows original and translated text in separate split panes while running', async () => {
  const view = render(Transcripts, { transcripts: { original: 'Hello, world', translated: 'Привіт, світе' }, phase: 'running' });
  const panes = view.container.querySelectorAll('.transcript-pane');
  expect(panes).toHaveLength(2);
  expect(panes[0]).toHaveTextContent('Оригінал');
  expect(panes[1]).toHaveTextContent('Переклад');
  expect(panes[0].querySelector('.pane-body')).toHaveTextContent('Hello, world');
  expect(panes[1].querySelector('.pane-body')).toHaveTextContent('Привіт, світе');
  // Live indicator is lit while the session runs.
  expect(panes[0].querySelector('i.live-dot')).not.toBeNull();
  await view.rerender({ phase: 'idle', transcripts: { original: '', translated: '' } });
  expect(screen.getAllByText('Поки порожньо. Запустіть переклад, щоб бачити текст наживо.')).toHaveLength(2);
  expect(panes[0].querySelector('i.live-dot')).toBeNull();
});
test('transcript labels follow the interface language', async () => {
  locale.set('en');
  const view = render(Transcripts, { transcripts: { original: 'Hi', translated: 'Привіт' }, phase: 'starting' });
  const panes = view.container.querySelectorAll('.transcript-pane');
  expect(panes[0]).toHaveTextContent('Original');
  expect(panes[1]).toHaveTextContent('Translation');
  await act(() => locale.set('uk'));
  expect(panes[0]).toHaveTextContent('Оригінал');
});

test('follows incremental text until the reader scrolls up, then resumes near the bottom', async () => {
  const view = render(Transcripts, { transcripts: { original: 'one', translated: 'один' }, phase: 'running' });
  const [original, translated] = Array.from(view.container.querySelectorAll<HTMLDivElement>('.pane-body'));
  let height = 300;
  for (const pane of [original, translated]) {
    Object.defineProperty(pane, 'scrollHeight', { get: () => height });
    Object.defineProperty(pane, 'clientHeight', { value: 100 });
  }
  await view.rerender({ transcripts: { original: 'one two', translated: 'один два' } });
  expect(original.scrollTop).toBe(300);
  expect(translated.scrollTop).toBe(300);
  original.scrollTop = 20;
  original.dispatchEvent(new Event('scroll'));
  height = 400;
  await view.rerender({ transcripts: { original: 'one two three', translated: 'один два три' } });
  expect(original.scrollTop).toBe(20);
  expect(translated.scrollTop).toBe(400);
  original.scrollTop = 300;
  original.dispatchEvent(new Event('scroll'));
  height = 500;
  await view.rerender({ transcripts: { original: 'one two three four', translated: 'один два три' } });
  expect(original.scrollTop).toBe(500);
});

test('opening the tab scrolls existing text to the bottom', () => {
  const height = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'scrollHeight');
  Object.defineProperty(HTMLElement.prototype, 'scrollHeight', { configurable: true, get: () => 500 });
  try {
    const view = render(Transcripts, { transcripts: { original: 'existing', translated: 'наявний' }, phase: 'idle' });
    for (const pane of view.container.querySelectorAll<HTMLDivElement>('.pane-body')) expect(pane.scrollTop).toBe(500);
  } finally {
    if (height) Object.defineProperty(HTMLElement.prototype, 'scrollHeight', height);
    else delete (HTMLElement.prototype as unknown as { scrollHeight?: number }).scrollHeight;
  }
});
