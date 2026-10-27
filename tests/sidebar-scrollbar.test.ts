// @ts-expect-error Node built-ins are used only for this static stylesheet check; the app bundle never imports them.
import { readFileSync } from 'node:fs';
import { render, screen, fireEvent } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import Sources from '../src/components/Sources.svelte';
import type { SourceView } from '../src/lib/sources';

// Static contract checks only. jsdom has no layout, so real scrollbar geometry and the macOS
// overlay track cannot be measured here; these assertions lock the CSS values a real window needs.
// Vitest runs from the project root, so the relative path resolves to the stylesheet the app ships.
const stylesheet = readFileSync('src/style.css', 'utf8');

const declarations = (selector: string) => {
  const blocks: string[] = [];
  for (const [, selectors, body] of stylesheet.replace(/\/\*[\s\S]*?\*\//g, '').matchAll(/([^{}]+)\{([^}]*)\}/g)) {
    if (selectors.split(',').some((part: string) => part.trim() === selector)) blocks.push(body);
  }
  expect(blocks, `missing rule for ${selector}`).not.toHaveLength(0);
  return Object.fromEntries(blocks.flatMap(body => body.split(';').filter(Boolean).map(entry => {
    const [property, ...value] = entry.split(':');
    return [property.trim(), value.join(':').trim()];
  })));
};

const chrome: SourceView = { id: 'app:/Chrome.app', name: 'Chrome', active: true, detail: null, favorite: false, available: true };
const closed: SourceView = { id: 'app:/Closed.app', name: 'Closed', active: false, detail: null, favorite: true, available: false };
const gone: SourceView = { id: 'app:/Gone.app', name: 'Gone', active: false, detail: null, favorite: false, available: false };

test('hide control is an eye-off icon that keeps the Приховати name and never reads as delete', async () => {
  const actions: string[] = [];
  render(Sources, { visible: [closed, chrome], hidden: [gone], selectedId: null, disabled: false, onSelect: () => {}, onRefresh: () => {}, onToggleFavorite: v => actions.push(`fav:${v.id}`), onHide: v => actions.push(`hide:${v.id}`), onRestore: v => actions.push(`restore:${v.id}`) });
  const hide = screen.getAllByRole('button', { name: 'Приховати' });
  expect(hide).toHaveLength(2);
  for (const button of hide) {
    expect(button).toHaveAttribute('title', 'Приховати');
    expect(button).not.toHaveTextContent('✕');
    expect(button.querySelector('svg')).toHaveAttribute('aria-hidden', 'true');
    expect(button.querySelector('svg')).toHaveAttribute('focusable', 'false');
  }
  const star = screen.getByRole('button', { name: 'Прибрати з обраних' });
  expect(star).toHaveTextContent('★');
  await fireEvent.click(star);
  expect(actions).toEqual(['fav:app:/Closed.app']);
});

test('hidden restore controls live in the same inset list as the visible rows', async () => {
  render(Sources, { visible: [chrome], hidden: [gone], selectedId: null, disabled: false, onSelect: () => {}, onRefresh: () => {}, onToggleFavorite: () => {}, onHide: () => {}, onRestore: () => {} });
  await fireEvent.click(screen.getByRole('button', { name: 'Приховані (1)' }));
  const restore = screen.getByRole('button', { name: 'Повернути' });
  expect(restore.closest('.source-list')).toHaveClass('hidden-list');
  expect(screen.getByRole('button', { name: 'Приховати' }).closest('.source-list')).not.toHaveClass('hidden-list');
});

test('scrollable source lists keep a real right inset plus a stable classic-scrollbar gutter', () => {
  const list = declarations('.source-list');
  expect(list.overflow).toContain('auto');
  expect(list['scrollbar-gutter']).toBe('stable');
  expect(parseFloat(list['padding-right'])).toBeGreaterThanOrEqual(10);
  expect(declarations('.source-actions')['flex-shrink']).toBe('0');
  expect(declarations('.source-row .source-item')['min-width']).toBe('0');
  expect(declarations('.source-toggle svg')['display']).toBe('block');
});
