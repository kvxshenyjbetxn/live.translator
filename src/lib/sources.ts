import type { AudioSource, Settings, SourceRef } from '../types';

/** Mirrors `crates/translator-core/src/settings.rs`; the backend stays authoritative. */
export const MAX_FAVORITES = 32;
export const MAX_HIDDEN = 32;
export const MAX_SOURCE_ID_BYTES = 512;
export const MAX_SOURCE_NAME_BYTES = 128;
export const MAX_SERIALIZED_BYTES = 60 * 1024;

const TOO_LARGE = 'Список програм завеликий для збереження. Приберіть частину обраних або прихованих програм.';
const FAVORITES_FULL = `Можна додати не більше ${MAX_FAVORITES} програм до обраних.`;
const HIDDEN_FULL = `Можна приховати не більше ${MAX_HIDDEN} програм.`;

export interface SourceView {
  id: string;
  name: string;
  active: boolean;
  detail: string | null;
  favorite: boolean;
  available: boolean;
}

export interface SourceCatalog {
  visible: SourceView[];
  hidden: SourceView[];
  all: SourceView[];
}

export interface PreferencePatch {
  favoriteSources?: SourceRef[];
  hiddenSources?: SourceRef[];
}

const utf8Bytes = (text: string) => new TextEncoder().encode(text).length;

/**
 * Compact JSON doubled over-counts the pretty form written by the backend (indentation and per-line
 * keys are not counted). It is a conservative proxy, not a guarantee: the backend stays authoritative,
 * and an allowed shrinking patch can still be refused there by the real byte limit.
 */
const serializedEstimate = (settings: Settings) => utf8Bytes(JSON.stringify(settings)) * 2;

/**
 * One list: favorites in starred order, then the remaining enumeration order, then hidden records
 * that are neither. Enumeration is the only availability truth; a record with no live source is an
 * unavailable placeholder. Hiding filters that order and never drops a row; first id wins.
 */
export function sourceCatalog(available: readonly AudioSource[], favorites: readonly SourceRef[], hidden: readonly SourceRef[]): SourceCatalog {
  const live = new Map<string, AudioSource>();
  for (const source of available) if (!live.has(source.id)) live.set(source.id, source);
  const hiddenIds = new Set(hidden.map(record => record.id));
  const done = new Set<string>();
  const all: SourceView[] = [];
  const include = (id: string, name: string, favorite: boolean) => {
    if (done.has(id)) return;
    done.add(id);
    const source = live.get(id);
    all.push(source
      ? { id, name: source.name, active: source.active, detail: source.detail, favorite, available: true }
      : { id, name, active: false, detail: null, favorite, available: false });
  };
  for (const record of favorites) include(record.id, record.name, true);
  for (const source of live.values()) include(source.id, source.name, false);
  for (const record of hidden) include(record.id, record.name, false);
  return {
    all,
    visible: all.filter(view => !hiddenIds.has(view.id)),
    hidden: all.filter(view => hiddenIds.has(view.id)),
  };
}

export function isFavorite(favorites: readonly SourceRef[], id: string): boolean {
  return favorites.some(record => record.id === id);
}

export function toggleFavorite(favorites: readonly SourceRef[], ref: SourceRef): SourceRef[] {
  return isFavorite(favorites, ref.id)
    ? favorites.filter(record => record.id !== ref.id)
    : [...favorites, { id: ref.id, name: ref.name }];
}

export function toggleHidden(hidden: readonly SourceRef[], ref: SourceRef): SourceRef[] {
  return hidden.some(record => record.id === ref.id)
    ? hidden.filter(record => record.id !== ref.id)
    : [...hidden, { id: ref.id, name: ref.name }];
}

/** Name for a selection that may no longer be listed; the favorite record is consulted first. */
export function recordName(favorites: readonly SourceRef[], hidden: readonly SourceRef[], id: string): string | null {
  return favorites.find(record => record.id === id)?.name ?? hidden.find(record => record.id === id)?.name ?? null;
}

function recordsValid(records: readonly SourceRef[]): boolean {
  const ids = new Set<string>();
  for (const record of records) {
    if (record.id.length === 0 || utf8Bytes(record.id) > MAX_SOURCE_ID_BYTES) return false;
    if (record.name.length === 0 || utf8Bytes(record.name) > MAX_SOURCE_NAME_BYTES) return false;
    if (ids.has(record.id)) return false;
    ids.add(record.id);
  }
  return true;
}

/**
 * Refusal text for the preference lists the patch would persist, or null. Records come from the
 * enumeration, so shape problems are reported with the size wording the brief pins; a patch that
 * strictly shrinks an already oversized file is allowed through so removal can always relieve it.
 * The backend re-checks the real bytes.
 */
export function preferenceError(settings: Settings, patch: PreferencePatch): string | null {
  const favoriteSources = patch.favoriteSources ?? settings.favoriteSources;
  const hiddenSources = patch.hiddenSources ?? settings.hiddenSources;
  if (favoriteSources.length > MAX_FAVORITES) return FAVORITES_FULL;
  if (hiddenSources.length > MAX_HIDDEN) return HIDDEN_FULL;
  if (!recordsValid(favoriteSources) || !recordsValid(hiddenSources)) return TOO_LARGE;
  const estimate = serializedEstimate({ ...settings, favoriteSources, hiddenSources });
  if (estimate <= MAX_SERIALIZED_BYTES || estimate < serializedEstimate(settings)) return null;
  return TOO_LARGE;
}
