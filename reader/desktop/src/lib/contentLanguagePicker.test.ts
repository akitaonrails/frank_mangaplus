import { describe, it, expect } from 'vitest';
import { contentLanguageSummary } from './contentLanguagePicker';
import { CONTENT_LANGUAGES } from './lang';

describe('contentLanguageSummary', () => {
  it('shows the single badge for one language', () => {
    expect(contentLanguageSummary(['eng'])).toBe('EN');
    expect(contentLanguageSummary(['ptb'])).toBe('PT-BR');
  });

  it('lists up to two badges in canonical order, not selection order', () => {
    expect(contentLanguageSummary(['eng', 'esp'])).toBe('EN, ESP');
    // Selection order reversed → same canonical output.
    expect(contentLanguageSummary(['esp', 'eng'])).toBe('EN, ESP');
  });

  it('collapses three or more to first badge + overflow count', () => {
    expect(contentLanguageSummary(['eng', 'esp', 'fra'])).toBe('EN +2');
    expect(contentLanguageSummary(['fra', 'eng', 'esp', 'ptb'])).toBe('EN +3');
  });

  it('says "All languages" when every catalog is selected', () => {
    const all = CONTENT_LANGUAGES.map(l => l.code);
    expect(contentLanguageSummary(all)).toBe('All languages');
  });

  it('handles the empty selection defensively', () => {
    expect(contentLanguageSummary([])).toBe('None');
  });
});
