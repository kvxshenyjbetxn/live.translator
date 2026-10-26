import type { Settings } from '../types';

export const DEFAULT_ACCENT = '#c5e89d';
export function normalizeAccent(value: string): string | null {
  const hex = value.trim();
  return /^#[\da-f]{6}$/i.test(hex) ? hex.toLowerCase() : null;
}
export function accentText(hex: string): string {
  const channels = [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16) / 255)
    .map(c => c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4);
  const luminance = channels[0] * .2126 + channels[1] * .7152 + channels[2] * .0722;
  return (luminance + .05) / .05 >= 1.05 / (luminance + .05) ? '#000000' : '#ffffff';
}
export function applyAppearance(settings: Settings, root: HTMLElement = document.documentElement): void {
  const accent = normalizeAccent(settings.accentColor) ?? DEFAULT_ACCENT;
  root.dataset.theme = settings.theme;
  root.style.setProperty('--accent', accent);
  root.style.setProperty('--accent-text', accentText(accent));
  // A black accent still needs a visible outline on a black theme.
  root.style.setProperty('--focus', accentText(accent) === '#ffffff' ? `color-mix(in srgb, ${accent} 45%, white)` : accent);
}
