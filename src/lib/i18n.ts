import { derived, writable } from 'svelte/store';
import messages from './messages.json';
export type Locale = 'uk' | 'en';
export const locale = writable<Locale>('uk');
const catalog: Record<string, string> = messages;
const templates = Object.keys(catalog).filter(key => /\{\w+\}/.test(key)).map(key => {
  const names: string[] = [];
  const expression = key.split(/(\{\w+\})/).map(part => {
    if (/^\{\w+\}$/.test(part)) { names.push(part.slice(1, -1)); return '(.+?)'; }
    return part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  }).join('');
  return { key, names, pattern: new RegExp(`^${expression}$`) };
});
/** Unknown external errors and application names are never mechanically translated. */
export function translate(text: string, language: Locale, params?: Record<string, string | number>): string {
  let key = text;
  let values = params;
  if (language === 'en' && !catalog[key] && !params) {
    if (text.startsWith('Error: ')) return `Error: ${translate(text.slice(7), language)}`;
    for (const template of templates) {
      const match = template.pattern.exec(text);
      if (match) {
        key = template.key;
        values = Object.fromEntries(template.names.map((name, i) => [name, translate(match[i + 1], language)]));
        break;
      }
    }
  }
  const result = language === 'en' ? catalog[key] ?? key : key;
  return result.replace(/\{(\w+)\}/g, (placeholder, name: string) => String(values?.[name] ?? placeholder));
}
export const t = derived(locale, language => (text: string, params?: Record<string, string | number>) => translate(text, language, params));
