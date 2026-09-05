// MANGA Plus represents a title's language as an int32 enum on the wire.
// The API endpoints (get_title_detail, get_chapter_pages, search) take
// language as a three-letter string code instead. This module is the
// single source of truth for that mapping plus the locale defaults.

export const ENGLISH = 'eng';
export const SPANISH = 'esp';
export const FRENCH = 'fra';
export const INDONESIAN = 'ind';
export const PORTUGUESE_BR = 'ptb';
export const RUSSIAN = 'rus';
export const THAI = 'tha';
export const VIETNAMESE = 'vie';
export const GERMAN = 'deu';

export const DEFAULT_LANG = ENGLISH;
export const DEFAULT_CLANG = ENGLISH;
export const DEFAULT_COUNTRY = 'US';

export const CONTENT_LANGUAGES = [
  { code: ENGLISH, wireEnum: 0, label: 'English', badge: 'EN' },
  { code: SPANISH, wireEnum: 1, label: 'Español', badge: 'ESP' },
  { code: FRENCH, wireEnum: 2, label: 'Français', badge: 'FRA' },
  { code: INDONESIAN, wireEnum: 3, label: 'Bahasa Indonesia', badge: 'IND' },
  { code: PORTUGUESE_BR, wireEnum: 4, label: 'Português (BR)', badge: 'PT-BR' },
  { code: RUSSIAN, wireEnum: 5, label: 'Русский', badge: 'RUS' },
  { code: THAI, wireEnum: 6, label: 'ไทย', badge: 'THA' },
  { code: VIETNAMESE, wireEnum: 9, label: 'Tiếng Việt', badge: 'VIE' },
  { code: GERMAN, wireEnum: 7, label: 'Deutsch', badge: 'DEU' },
] as const;

export type ContentLanguage = (typeof CONTENT_LANGUAGES)[number]['code'];

export function isContentLanguage(value: string): value is ContentLanguage {
  return CONTENT_LANGUAGES.some(language => language.code === value);
}

export function contentLanguagesInNavbarOrder(
  selected: readonly ContentLanguage[],
): ContentLanguage[] {
  const wanted = new Set(selected);
  return CONTENT_LANGUAGES
    .map(language => language.code)
    .filter(code => wanted.has(code));
}

// Derived from CONTENT_LANGUAGES so the frontend declares each wire value
// exactly once. Rust fixture tests independently verify the corresponding
// API map against captured protobuf responses. Enum 8 remains intentionally
// unmapped until the API identifies it.
const LANG_ENUM_TO_CODE = new Map<number, ContentLanguage>(
  CONTENT_LANGUAGES.map(language => [language.wireEnum, language.code]),
);

const CODE_TO_LANG_ENUM = new Map<ContentLanguage, number>(
  CONTENT_LANGUAGES.map(language => [language.code, language.wireEnum]),
);

export function contentLanguageWireEnum(code: ContentLanguage): number {
  const wireEnum = CODE_TO_LANG_ENUM.get(code);
  if (wireEnum == null) throw new Error(`Unsupported content language: ${code}`);
  return wireEnum;
}

export function langCode(lang: number): ContentLanguage {
  return LANG_ENUM_TO_CODE.get(lang) ?? DEFAULT_LANG;
}

export function titleContentLanguage(lang: number): ContentLanguage | null {
  const code = LANG_ENUM_TO_CODE.get(lang);
  return code ?? null;
}

export function languageBadge(lang: number): string | null {
  const code = LANG_ENUM_TO_CODE.get(lang);
  if (code === ENGLISH) return null;
  const supported = CONTENT_LANGUAGES.find(language => language.code === code);
  if (supported) return supported.badge;

  // Keep every non-English card visibly labelled even if the API adds a
  // language before this mapping is updated.
  return code?.toUpperCase() ?? `LANG ${lang}`;
}

export function filterByContentLanguages<T extends { language: number }>(
  titles: T[],
  selected: readonly ContentLanguage[],
): T[] {
  const wanted = new Set(selected);
  return titles.filter(title => {
    const code = titleContentLanguage(title.language);
    return code != null && wanted.has(code);
  });
}

function normalizedTitleName(name: string): string {
  return name.normalize('NFKC').trim().replace(/\s+/g, ' ').toLowerCase();
}

export function mergeTitles<T extends { titleId: number; name: string }>(
  lists: readonly (readonly T[])[],
): T[] {
  const seen = new Set<number>();
  const groups = new Map<string, T[]>();
  for (const list of lists) {
    for (const title of list) {
      if (seen.has(title.titleId)) continue;
      seen.add(title.titleId);

      const normalizedName = normalizedTitleName(title.name);
      // Do not collapse unrelated malformed titles whose names are empty.
      const key = normalizedName.length > 0
        ? `name:${normalizedName}`
        : `id:${title.titleId}`;
      const group = groups.get(key);
      if (group) group.push(title);
      else groups.set(key, [title]);
    }
  }
  return [...groups.values()].flat();
}

export function mergeTitlesByContentLanguages<
  T extends { titleId: number; name: string; language: number },
>(titles: readonly T[], selected: readonly ContentLanguage[]): T[] {
  return mergeTitles(
    contentLanguagesInNavbarOrder(selected).map(code =>
      titles.filter(title => titleContentLanguage(title.language) === code),
    ),
  );
}

export function titleHref(title: { titleId: number; language: number }): string {
  const clang = titleContentLanguage(title.language) ?? DEFAULT_CLANG;
  const query = clang === DEFAULT_CLANG ? '' : `?clang=${encodeURIComponent(clang)}`;
  return `/title/${title.titleId}${query}`;
}
