import {
  DEFAULT_CLANG,
  DEFAULT_COUNTRY,
  DEFAULT_LANG,
  isContentLanguage,
  type ContentLanguage,
} from './lang';

export type ContentLocale = {
  lang: ContentLanguage;
  clang: ContentLanguage;
  country: string;
};

type SearchParamsReader = Pick<URLSearchParams, 'get'>;

function supportedLanguage(value: string | null, fallback: ContentLanguage): ContentLanguage {
  return value != null && isContentLanguage(value) ? value : fallback;
}

/** Resolve the locale tuple carried between Search, detail, and reader URLs. */
export function contentLocaleFromSearchParams(params: SearchParamsReader): ContentLocale {
  return {
    lang: supportedLanguage(params.get('lang'), DEFAULT_LANG),
    clang: supportedLanguage(params.get('clang'), DEFAULT_CLANG),
    country: params.get('country') || DEFAULT_COUNTRY,
  };
}

/** Keep URLs compact by omitting default locale values. */
export function contentLocaleSuffix(clang: ContentLanguage, country: string): string {
  const params = new URLSearchParams();
  if (clang !== DEFAULT_CLANG) params.set('clang', clang);
  if (country !== DEFAULT_COUNTRY) params.set('country', country);
  const query = params.toString();
  return query ? `?${query}` : '';
}

export function readerHref(
  chapterId: number,
  clang: ContentLanguage,
  country: string,
): string {
  return `/reader/${chapterId}${contentLocaleSuffix(clang, country)}`;
}

export function titleDetailHref(
  titleId: number,
  clang: ContentLanguage,
  country: string,
): string {
  return `/title/${titleId}${contentLocaleSuffix(clang, country)}`;
}

export function titleDetailRequestArgs(
  titleId: number,
  lang: ContentLanguage,
  clang: ContentLanguage,
  country: string,
) {
  return { titleId, lang, clang, countryCode: country };
}

export function chapterPagesRequestArgs(
  chapterId: number,
  clang: ContentLanguage,
  country: string,
) {
  return {
    chapterId,
    imgQuality: 'super_high',
    viewerMode: 'vertical',
    clang,
    countryCode: country,
  };
}
