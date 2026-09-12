import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import TitleCard from './TitleCard.svelte';
import {
  contentLanguageWireEnum,
  ENGLISH,
  PORTUGUESE_BR,
} from './lang';
import type { Title } from './types';

function title(language: number): Title {
  return {
    titleId: 100,
    name: 'Example title',
    author: 'Example author',
    portraitImageUrl: 'https://example.invalid/cover.webp',
    language,
  };
}

describe('TitleCard content-language badge', () => {
  it('keeps the default English card unlabelled', () => {
    const { body } = render(TitleCard, {
      props: { title: title(contentLanguageWireEnum(ENGLISH)) },
    });

    expect(body).not.toContain('content-language-badge');
  });

  it('renders the translated-edition badge returned by the language map', () => {
    const { body } = render(TitleCard, {
      props: { title: title(contentLanguageWireEnum(PORTUGUESE_BR)) },
    });

    expect(body).toContain('content-language-badge');
    expect(body).toContain('PT-BR');
  });

  it('makes an unknown non-English enum visible instead of hiding it', () => {
    const unknownWireEnum = Number.MAX_SAFE_INTEGER;
    const { body } = render(TitleCard, { props: { title: title(unknownWireEnum) } });

    expect(body).toContain('content-language-badge');
    expect(body).toContain(`LANG ${unknownWireEnum}`);
  });
});
