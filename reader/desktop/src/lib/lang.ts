// MANGA Plus represents a title's language as an int32 enum on the wire.
// The API endpoints (get_title_detail, get_chapter_pages, search) take
// language as a three-letter string code instead. This module is the
// single source of truth for that mapping plus the locale defaults.

export const DEFAULT_LANG = 'eng';
export const DEFAULT_CLANG = 'eng';
export const DEFAULT_COUNTRY = 'US';

const LANG_ENUM_TO_CODE: Record<number, string> = {
  0: 'eng',
  1: 'esp',
  2: 'fra',
  3: 'por',
  4: 'rus',
  5: 'ind',
};

export function langCode(lang: number): string {
  return LANG_ENUM_TO_CODE[lang] ?? DEFAULT_LANG;
}
