import { describe, it, expect } from 'vitest';
import {
  langCode,
  CONTENT_LANGUAGES,
  ENGLISH,
  GERMAN,
  PORTUGUESE_BR,
  SPANISH,
  VIETNAMESE,
  DEFAULT_LANG,
  DEFAULT_CLANG,
  DEFAULT_COUNTRY,
  contentLanguagesInNavbarOrder,
  filterByContentLanguages,
  languageBadge,
  mergeTitles,
  mergeTitlesByContentLanguages,
  titleContentLanguage,
  contentLanguageWireEnum,
  titleHref,
} from './lang';

const UNKNOWN_WIRE_ENUM = Number.MAX_SAFE_INTEGER;

describe('langCode', () => {
  it('maps the known enum values', () => {
    for (const { code, wireEnum } of CONTENT_LANGUAGES) {
      expect(langCode(wireEnum)).toBe(code);
      expect(contentLanguageWireEnum(code)).toBe(wireEnum);
    }
  });

  it('falls back to the default for unknown values', () => {
    expect(langCode(UNKNOWN_WIRE_ENUM)).toBe(DEFAULT_LANG);
    expect(langCode(-UNKNOWN_WIRE_ENUM)).toBe(DEFAULT_LANG);
  });
});

describe('content language helpers', () => {
  const editions = Object.fromEntries(
    CONTENT_LANGUAGES.map((language, index) => [
      language.code,
      { titleId: 100000 + index, language: language.wireEnum },
    ]),
  ) as Record<(typeof CONTENT_LANGUAGES)[number]['code'], { titleId: number; language: number }>;
  const english = editions.eng;
  const spanish = editions.esp;
  const french = editions.fra;
  const indonesian = editions.ind;
  const portuguese = editions.ptb;
  const russian = editions.rus;
  const thai = editions.tha;
  const german = editions.deu;
  const unsupported = { titleId: 100150, language: UNKNOWN_WIRE_ENUM };
  const vietnamese = editions.vie;
  const supported = [
    english,
    spanish,
    french,
    indonesian,
    portuguese,
    russian,
    thai,
    german,
    vietnamese,
  ];

  it('maps title enums to selectable content languages', () => {
    expect(supported.map(title => titleContentLanguage(title.language))).toEqual(
      supported.map(title => langCode(title.language)),
    );
    expect(titleContentLanguage(UNKNOWN_WIRE_ENUM)).toBe(null);
  });

  it('filters one or several selected languages', () => {
    const titles = [...supported, unsupported];
    expect(filterByContentLanguages(titles, ['ptb'])).toEqual([portuguese]);
    expect(filterByContentLanguages(titles, ['eng', 'deu', 'vie'])).toEqual([
      english,
      german,
      vietnamese,
    ]);
  });

  it('uses navbar order regardless of activation order', () => {
    expect(contentLanguagesInNavbarOrder(['deu', 'ptb', 'eng', 'vie'])).toEqual([
      'eng',
      'ptb',
      'vie',
      'deu',
    ]);
  });

  it('keeps distinct translated title ids while deduping repeated ids', () => {
    const englishEdition = { ...english, name: 'One Piece' };
    const portugueseEdition = { ...portuguese, name: 'One Piece' };
    expect(mergeTitles([[englishEdition], [portugueseEdition, englishEdition]])).toEqual([
      englishEdition,
      portugueseEdition,
    ]);
  });

  it('groups every same-name edition in navbar language order', () => {
    const englishOnePiece = { ...english, name: 'One Piece' };
    const portugueseOnePiece = { ...portuguese, name: 'One Piece' };
    const englishDandadan = {
      titleId: 100022,
      language: contentLanguageWireEnum(ENGLISH),
      name: 'Dandadan',
    };
    const portugueseDandadan = {
      titleId: 100151,
      language: contentLanguageWireEnum(PORTUGUESE_BR),
      name: 'Dandadan',
    };
    const anotherEnglishTitle = {
      titleId: 100021,
      language: contentLanguageWireEnum(ENGLISH),
      name: 'Someone Hertz',
    };
    expect(mergeTitles([
      [englishOnePiece, englishDandadan, anotherEnglishTitle],
      [portugueseDandadan, portugueseOnePiece],
    ])).toEqual([
      englishOnePiece,
      portugueseOnePiece,
      englishDandadan,
      portugueseDandadan,
      anotherEnglishTitle,
    ]);
  });

  it('normalizes case and whitespace when grouping translated editions', () => {
    const englishEdition = { ...english, name: 'One Piece' };
    const portugueseEdition = { ...portuguese, name: '  ONE   PIECE  ' };
    expect(mergeTitles([[englishEdition], [portugueseEdition]])).toEqual([
      englishEdition,
      portugueseEdition,
    ]);
  });

  it('groups same-name favorites in the Library across active languages', () => {
    const englishOnePiece = { ...english, name: 'One Piece' };
    const portugueseOnePiece = { ...portuguese, name: 'One Piece' };
    const portugueseJujutsu = {
      titleId: 100152,
      language: contentLanguageWireEnum(PORTUGUESE_BR),
      name: 'Jujutsu Kaisen',
    };
    const favorites = [englishOnePiece, portugueseJujutsu, portugueseOnePiece];

    expect(mergeTitlesByContentLanguages(favorites, ['ptb', 'eng'])).toEqual([
      englishOnePiece,
      portugueseOnePiece,
      portugueseJujutsu,
    ]);
  });

  it('filters Library editions without removing or collapsing hidden favorites', () => {
    const englishOnePiece = { ...english, name: 'One Piece' };
    const portugueseOnePiece = { ...portuguese, name: 'One Piece' };
    const germanOnePiece = { ...german, name: 'One Piece' };
    const favorites = [portugueseOnePiece, germanOnePiece, englishOnePiece];

    expect(mergeTitlesByContentLanguages(favorites, ['eng'])).toEqual([englishOnePiece]);
    expect(mergeTitlesByContentLanguages(favorites, ['ptb'])).toEqual([portugueseOnePiece]);
    expect(mergeTitlesByContentLanguages(favorites, ['deu', 'ptb', 'eng'])).toEqual([
      englishOnePiece,
      portugueseOnePiece,
      germanOnePiece,
    ]);
    expect(favorites).toEqual([portugueseOnePiece, germanOnePiece, englishOnePiece]);
  });

  it('labels every translated card and keeps unknown enum labels generic', () => {
    expect(languageBadge(contentLanguageWireEnum(ENGLISH))).toBe(null);
    const translated = CONTENT_LANGUAGES.filter(language => language.code !== ENGLISH);
    expect(translated.map(language => languageBadge(language.wireEnum))).toEqual(
      translated.map(language => language.badge),
    );
    expect(languageBadge(UNKNOWN_WIRE_ENUM)).toBe(`LANG ${UNKNOWN_WIRE_ENUM}`);
    expect(languageBadge(-UNKNOWN_WIRE_ENUM)).toBe(`LANG ${-UNKNOWN_WIRE_ENUM}`);
  });

  it('builds locale-aware detail links for each translated edition', () => {
    expect(titleHref(english)).toBe(`/title/${english.titleId}`);
    expect(titleHref(portuguese)).toContain(`clang=${PORTUGUESE_BR}`);
    expect(titleHref(spanish)).toContain(`clang=${SPANISH}`);
    expect(titleHref(german)).toContain(`clang=${GERMAN}`);
    expect(titleHref(vietnamese)).toContain(`clang=${VIETNAMESE}`);
  });
});

describe('defaults', () => {
  it('exposes the locale defaults the desktop ships with', () => {
    expect(DEFAULT_LANG).toBe('eng');
    expect(DEFAULT_CLANG).toBe('eng');
    expect(DEFAULT_COUNTRY).toBe('US');
  });
});
