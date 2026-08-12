import { beforeEach, describe, expect, it } from 'vitest';
import {
  CONTENT_LANGUAGES_STORAGE_KEY,
  contentLanguages,
  loadContentLanguages,
  normalizeContentLanguages,
  setContentLanguages,
  toggleContentLanguage,
} from './contentLanguagePreference';
import { get } from 'svelte/store';

function installLocalStorageStub(): Storage {
  const values = new Map<string, string>();
  const storage: Storage = {
    get length() {
      return values.size;
    },
    clear: () => values.clear(),
    getItem: key => values.get(key) ?? null,
    key: index => Array.from(values.keys())[index] ?? null,
    removeItem: key => void values.delete(key),
    setItem: (key, value) => void values.set(key, value),
  };
  globalThis.localStorage = storage;
  return storage;
}

beforeEach(() => {
  installLocalStorageStub();
});

describe('content language preference', () => {
  it('defaults to English for missing, empty, or invalid values', () => {
    expect(normalizeContentLanguages(null)).toEqual(['eng']);
    expect(normalizeContentLanguages([])).toEqual(['eng']);
    expect(normalizeContentLanguages(['unknown'])).toEqual(['eng']);
  });

  it('accepts every supported language and removes duplicates', () => {
    expect(
      normalizeContentLanguages([
        'eng',
        'esp',
        'fra',
        'ind',
        'ptb',
        'rus',
        'tha',
        'vie',
        'deu',
        'eng',
      ]),
    ).toEqual(['eng', 'esp', 'fra', 'ind', 'ptb', 'rus', 'tha', 'vie', 'deu']);
  });

  it('normalizes selections to the navbar language order', () => {
    expect(normalizeContentLanguages(['deu', 'ptb', 'eng', 'vie'])).toEqual([
      'eng',
      'ptb',
      'vie',
      'deu',
    ]);
  });

  it('loads a persisted selection and recovers from corrupt JSON', () => {
    localStorage.setItem(CONTENT_LANGUAGES_STORAGE_KEY, JSON.stringify(['ptb', 'eng']));
    expect(loadContentLanguages(localStorage)).toEqual(['eng', 'ptb']);

    localStorage.setItem(CONTENT_LANGUAGES_STORAGE_KEY, '{broken');
    expect(loadContentLanguages(localStorage)).toEqual(['eng']);
  });

  it('persists an updated selection', () => {
    setContentLanguages(['eng', 'ptb']);
    expect(JSON.parse(localStorage.getItem(CONTENT_LANGUAGES_STORAGE_KEY) ?? '[]')).toEqual([
      'eng',
      'ptb',
    ]);
  });

  it('toggles languages but never leaves the selection empty', () => {
    setContentLanguages(['eng']);
    toggleContentLanguage('eng');
    expect(get(contentLanguages)).toEqual(['eng']);

    toggleContentLanguage('ptb');
    expect(get(contentLanguages)).toEqual(['eng', 'ptb']);

    toggleContentLanguage('eng');
    expect(get(contentLanguages)).toEqual(['ptb']);
    toggleContentLanguage('ptb');
    expect(get(contentLanguages)).toEqual(['ptb']);
  });
});
