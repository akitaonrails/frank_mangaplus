import { describe, expect, it } from 'vitest';
import {
  chapterPagesRequestArgs,
  contentLocaleFromSearchParams,
  contentLocaleSuffix,
  readerHref,
  titleDetailHref,
  titleDetailRequestArgs,
} from './contentLocale';
import {
  CONTENT_LANGUAGES,
  DEFAULT_CLANG,
  DEFAULT_COUNTRY,
  DEFAULT_LANG,
} from './lang';

describe('content locale routing', () => {
  it('defaults missing or unsupported URL values', () => {
    expect(contentLocaleFromSearchParams(new URLSearchParams())).toEqual({
      lang: DEFAULT_LANG,
      clang: DEFAULT_CLANG,
      country: DEFAULT_COUNTRY,
    });
    expect(
      contentLocaleFromSearchParams(
        new URLSearchParams('lang=unknown&clang=unknown&country=BR'),
      ),
    ).toEqual({ lang: DEFAULT_LANG, clang: DEFAULT_CLANG, country: 'BR' });
  });

  it('passes every selected content language to detail and chapter requests', () => {
    for (const { code } of CONTENT_LANGUAGES) {
      const locale = contentLocaleFromSearchParams(
        new URLSearchParams(`clang=${code}&country=BR`),
      );
      expect(titleDetailRequestArgs(101, locale.lang, locale.clang, locale.country)).toEqual({
        titleId: 101,
        lang: DEFAULT_LANG,
        clang: code,
        countryCode: 'BR',
      });
      expect(chapterPagesRequestArgs(202, locale.clang, locale.country)).toEqual({
        chapterId: 202,
        imgQuality: 'super_high',
        viewerMode: 'vertical',
        clang: code,
        countryCode: 'BR',
      });
    }
  });

  it('preserves translated locale through detail, reader, next chapter, and back links', () => {
    for (const { code } of CONTENT_LANGUAGES) {
      const detail = titleDetailHref(101, code, 'BR');
      const chapter = readerHref(202, code, 'BR');
      const nextChapter = readerHref(203, code, 'BR');

      for (const href of [detail, chapter, nextChapter]) {
        const url = new URL(href, 'https://desktop.invalid');
        const resolved = contentLocaleFromSearchParams(url.searchParams);
        expect(resolved.clang).toBe(code);
        expect(resolved.country).toBe('BR');
        if (code !== DEFAULT_CLANG) expect(url.searchParams.get('clang')).toBe(code);
      }
    }
  });

  it('omits default values while preserving either non-default value', () => {
    expect(contentLocaleSuffix(DEFAULT_CLANG, DEFAULT_COUNTRY)).toBe('');
    expect(contentLocaleSuffix('ptb', DEFAULT_COUNTRY)).toBe('?clang=ptb');
    expect(contentLocaleSuffix(DEFAULT_CLANG, 'BR')).toBe('?country=BR');
    expect(contentLocaleSuffix('ptb', 'BR')).toBe('?clang=ptb&country=BR');
  });
});
