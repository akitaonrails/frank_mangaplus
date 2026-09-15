// Pure helper for the content-language dropdown trigger label. Kept out
// of the Svelte component so it's unit-testable. The picker itself is a
// multi-select (several catalogs can be merged at once), so the trigger
// summarizes the selection rather than showing a single value.

import { CONTENT_LANGUAGES, type ContentLanguage } from './lang';

/**
 * Short label for the dropdown trigger, given the active selection:
 *   - []            → "None"        (shouldn't happen; the store keeps ≥1)
 *   - [EN]          → "EN"
 *   - [EN, ESP]     → "EN, ESP"
 *   - [EN, ESP, +3] → "EN +3"
 *   - all nine      → "All languages"
 * Badges are emitted in the canonical CONTENT_LANGUAGES order, not
 * selection order, so the label is stable regardless of click order.
 */
export function contentLanguageSummary(selected: readonly ContentLanguage[]): string {
  const badges = CONTENT_LANGUAGES.filter(l => selected.includes(l.code)).map(l => l.badge);
  if (badges.length === 0) return 'None';
  if (badges.length === CONTENT_LANGUAGES.length) return 'All languages';
  if (badges.length <= 2) return badges.join(', ');
  return `${badges[0]} +${badges.length - 1}`;
}
