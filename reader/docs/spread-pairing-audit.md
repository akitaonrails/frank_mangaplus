# Spread-pairing audit: unmarked spreads in One Piece

Audit date: 2026-09-19. Branch: `feat/spread-aware-page-pairing`.

## What triggered it

A report that the double-mode page order disagreed with the printed
tankobon in some of the first three One Piece chapters, in both English
and Brazilian Portuguese.

## What the data shows

One Piece chapters 1-3 in both editions (English `title_id=100020`,
Portuguese `title_id=100149`) come back from `manga_viewer_v3` with
`split=yes` and every page typed `MangaPage.type=0` (SINGLE), 1080x1620.
There are no RIGHT (2) / LEFT (1) split markers at all. The Romance Dawn
double-page splash itself arrives as two consecutive type-0 singles with
no marking.

This contradicts the assumption that `split=yes` always delivers a
two-page spread as a RIGHT half followed by its LEFT half. That holds for
the titles the code was validated against; One Piece is a counterexample.

## Consequence for the reader

`startsSpread` and `coverBindsSolo` in `src/lib/readerLogic.ts` key only
on the RIGHT-then-LEFT type pair. With no markers, spread detection never
fires and `buildPageGroups` falls to its default, `coverBindsSolo` true,
producing cover-solo grouping `[cover][1,2][3,4]...`. This is the intended
default (see the "a chapter with no spread binds its cover solo" test).

## Verification: the reported bug does not reproduce

Three independent checks agree the cover-solo output matches print for
ch1-3 in both languages:

1. The Romance Dawn splash renders whole and correctly oriented (Luffy on
   the left, crew and title on the right, seam continuous) under the
   reader's RTL row-reverse pairing.
2. Printed page numbers (OCR'd from the page corners) show each chapter
   opens on an odd, widow-left page whose facing partner lives in the
   previous chapter, so rendering the first page solo is print-correct and
   every following pair lands on the right parity. Sampled numbers all fit
   a single contiguous mapping per chapter, so there is no mid-chapter
   parity break.
3. An A4 print booklet of ch3 EN was compared against the physical volume
   and matches.

## The real finding: correct by parity luck, not by detection

The pairing is right here because these chapters happen to open on a
widow-left page and the only real spread (Romance Dawn) happens to sit at
an odd offset. The code cannot see the unmarked spread, so it cannot
protect it: shift the chapter by one leading page and the splash would be
torn across two frames with no way for the current logic to notice, and a
markerless chapter that should pair from page 1 would be mispaired
end-to-end.

## Recommendation (not yet implemented)

Keep the RIGHT/LEFT `type` markers as the primary signal: where they are
sent they are cheap and unambiguous, and the existing pairing already
handles them. Add the image-based heuristic only as a fallback for the
scenario where the markers are absent, rather than replacing marker-based
detection.

In that fallback, infer spreads from the images: a landscape page is a
pre-merged spread served whole, and two portrait pages whose inner edges
continue across the seam are a split spread. Anchor the per-chapter parity
on that evidence, and fall back to cover-solo only when there is none. This
removes the parity-luck dependency for titles like One Piece that never set
the markers, without changing behaviour for titles that do.

Practical notes for whoever implements it:

- The landscape-aspect check is free from metadata: `MangaPage` already
  carries `width` and `height`, so no decode is needed. It does not help
  One Piece, though: the Romance Dawn splash is delivered as two portrait
  halves (1080x1620 each), not one wide image. The inner-edge check is the
  one that catches that case, and it needs pixels.
- "Inner edge carries ink" alone is not enough. A panel that bleeds to the
  page edge looks the same, which made a naive ink-fraction probe noisy
  during this audit. The robust signal is to correlate the two inner-edge
  strips and confirm the art actually continues across the seam, not just
  that ink is present.
- The fallback only helps a chapter that has an unmarked spread to anchor
  on. A chapter that is genuinely all single pages still has no cheap way
  to know its parity (page-number OCR is fragile), but there is no spread
  to tear there, so cover-solo stays acceptable.
