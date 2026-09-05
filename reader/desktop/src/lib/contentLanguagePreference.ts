import { writable } from 'svelte/store';
import {
  contentLanguagesInNavbarOrder,
  DEFAULT_CLANG,
  isContentLanguage,
  type ContentLanguage,
} from './lang';

export const CONTENT_LANGUAGES_STORAGE_KEY = 'mp:contentLanguages';
export const DEFAULT_CONTENT_LANGUAGES: ContentLanguage[] = [DEFAULT_CLANG];

function availableStorage(): Storage | undefined {
  try {
    return typeof localStorage === 'undefined' ? undefined : localStorage;
  } catch {
    return undefined;
  }
}

export function normalizeContentLanguages(values: unknown): ContentLanguage[] {
  if (!Array.isArray(values)) return [...DEFAULT_CONTENT_LANGUAGES];
  const requested = new Set(
    values.filter(
      (value): value is ContentLanguage =>
        typeof value === 'string' && isContentLanguage(value),
    ),
  );
  // Keep the stored selection in the same stable order as the navbar.
  // Catalog merging relies on this order when placing translated editions
  // of the same title next to one another.
  const selected = contentLanguagesInNavbarOrder([...requested]);
  return selected.length > 0 ? selected : [...DEFAULT_CONTENT_LANGUAGES];
}

export function loadContentLanguages(storage = availableStorage()): ContentLanguage[] {
  if (!storage) return [...DEFAULT_CONTENT_LANGUAGES];
  try {
    const raw = storage.getItem(CONTENT_LANGUAGES_STORAGE_KEY);
    return raw == null
      ? [...DEFAULT_CONTENT_LANGUAGES]
      : normalizeContentLanguages(JSON.parse(raw));
  } catch {
    return [...DEFAULT_CONTENT_LANGUAGES];
  }
}

export const contentLanguages = writable<ContentLanguage[]>(loadContentLanguages());

export function setContentLanguages(values: readonly ContentLanguage[]): void {
  const next = normalizeContentLanguages([...values]);
  availableStorage()?.setItem(CONTENT_LANGUAGES_STORAGE_KEY, JSON.stringify(next));
  contentLanguages.set(next);
}

export function toggleContentLanguage(code: ContentLanguage): void {
  contentLanguages.update(current => {
    // At least one catalog must remain active. If this is the only
    // selected language, leave it selected instead of silently jumping
    // back to the default language.
    if (current.length === 1 && current[0] === code) return current;
    const next = current.includes(code)
      ? current.filter(value => value !== code)
      : [...current, code];
    const normalized = normalizeContentLanguages(next);
    availableStorage()?.setItem(CONTENT_LANGUAGES_STORAGE_KEY, JSON.stringify(normalized));
    return normalized;
  });
}
