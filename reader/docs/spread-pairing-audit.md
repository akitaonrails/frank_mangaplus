# Spread-pairing audit: unmarked spreads in One Piece

Audit date: 2026-09-19. Branch: `feat/spread-aware-page-pairing`.
Corrected: 2026-09-27, after re-measuring both editions (see "Correction
of 2026-09-27" and "Parity luck already fails" below).

## What triggered it

A report that the double-mode page order disagreed with the printed
tankobon in some of the first three One Piece chapters, in both English
and Brazilian Portuguese.

## What the data shows

One Piece chapters 1-3 in the English edition (`title_id=100020`) come
back from `manga_viewer_v3` with `split=yes` and every page typed
`MangaPage.type=0` (SINGLE), 1080x1620. There are no RIGHT (2) / LEFT (1)
split markers at all. The Romance Dawn double-page splash itself arrives
as two consecutive type-0 singles with no marking.

### Correction of 2026-09-27

The first version of this audit said the same of the Portuguese edition
(`title_id=100149`). Re-measured on 2026-09-27 with the same request, that
does not hold. Portuguese ch1 (chapter 1009174) marks two spreads: `2r`/`3l`
at offsets 1-2 (the Romance Dawn splash) and `50r`/`51l` at offsets 49-50,
typed RIGHT (2) then LEFT (1), at both `high` (1073x1620) and `super_high`.
Portuguese ch2 and ch3 carry no marked spread. English ch1 (chapter
1000486) still returns all 53 pages as type 0. Whether the server added the
Portuguese markers after the audit or the original check did not cover
that edition, the data cannot tell.

This contradicts the assumption that `split=yes` always delivers a
two-page spread as a RIGHT half followed by its LEFT half. That holds for
the titles the code was validated against; One Piece is a counterexample.

## Consequence for the reader

`startsSpread` and `coverBindsSolo` in `src/lib/readerLogic.ts` key only
on the RIGHT-then-LEFT type pair. With no markers, spread detection never
fires and `buildPageGroups` falls to its default, `coverBindsSolo` true,
producing cover-solo grouping `[cover][1,2][3,4]...`. This is the intended
default (see the "a chapter with no spread binds its cover solo" test).

Portuguese ch1 now takes the marker path instead: its first spread sits at
offset 1, so `coverBindsSolo` returns true by detection, and the grouping
is the same cover-solo grouping the English edition reaches by default.

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
an odd offset. Automatic detection cannot see the unmarked spread: shift
the chapter by one leading page and the splash would be torn across two
frames, and a markerless chapter that should pair from page 1 would be
mispaired end-to-end. The manual correction described below now gives the
reader a way to fix that missing server metadata.

## Parity luck already fails

The failure predicted above shows up in current chapters. Each case below
is an English chapter with no markers whose first spread sits at an even
offset, so the cover-solo default tears it. Where the Portuguese edition
serves the same chapter, with the same page count, it marks the spread.
Every spread was also confirmed by viewing the two pages side by side.

| English chapter | Spread (pages) | Offset | Portuguese chapter marking it |
|-|-|-|-|
| One Piece #1191 (`100020`, chapter 1029978) | 1-2, colour cover spread | 0 | 5001167: `1r`/`2l`, plus `9r`/`10l`, `13r`/`14l`, `17r`/`18l` |
| Dandadan #1 (`100171`, chapter 1009921) | 3-4, title spread | 2 | 5000479: `3r`/`4l`, plus `55r`/`56l`, `67r`/`68l` |
| Dandadan #2 (`100171`, chapter 1009922) | 23-24 | 22 | 5000480: `23r`/`24l`, plus `29r`/`30l` |
| Dandadan #215 (`100171`, chapter 1026818) | 1-2, title spread; also 20-21, closing spread | 0 and 19 | not served: the Portuguese edition lists only ch1-3 and 244-246 |

Dandadan #215 is also the first chapter on record whose spreads mix
parities, a case the probe behind issue #9 did not find. Its title spread
sits at offset 0 and its closing spread at offset 19, so no single parity
keeps both whole. With markers, `buildPageGroups` would pair from page 1
and render page 19 solo to keep 20-21 together. Without them, the
cover-solo default keeps 20-21 whole and tears the title spread.

This is a property of these titles, not of the English edition. The probe
behind issue #9 found 112 marked spreads in 50 of the 108 first chapters
of the English serializing catalog, so marked spreads are common in
English titles. One Piece and Dandadan are English titles that do not
mark them.

The missing markers are server-side. One Piece chapters 1-3 and
#1191-#1193 were requested in both editions (12 chapters) under seven
combinations each: `viewer_mode` vertical and horizontal, `split` yes and
no, `img_quality` super_high, high and low, `country_code` US and BR. Every
English response returns all pages as type 0, and `split=no` returns the
same page count with no DOUBLE (3) page. With `split=no` the Portuguese
chapters merge each marked pair into one DOUBLE page (#1191 goes from 18
pages to 14).

A raw decode of the protobuf, without this repository's sparse `.proto`,
finds no other field carrying the information. English `MangaPage`
entries carry fields 1, 2, 3 and 6 (URL, width, height, merch store URL)
and never field 4. `MangaViewer` and `Page` carry the same field numbers in
both editions. Every page of an English chapter has the same dimensions
(784x1145 for #1191, 1400x2100 for ch1), while the Portuguese spread
halves are narrower than its single pages (1403 against 1444 for #1191).
No request parameter tried recovers the spread information for these
English titles.

## Resolution: manual pairing shifts

For chapters with no API-marked spread, the reader now exposes a
pairing-shift button and the `P` shortcut in double mode. Chapters with
any authoritative RIGHT/LEFT pair keep the control disabled. Corrections
are persisted per chapter as zero-based page offsets:

- A shift at offset 0 inverts the automatic cover-solo versus
  pair-from-page-1 decision.
- A later shift makes its page solo, then resumes pairing from the next
  page.
- Multiple saved shifts correct mixed-parity chapters such as Dandadan
  #215.
- Removing a shift also clears later corrections whose parity depended
  on it.
- A chapter containing any RIGHT/LEFT pair from the API ignores all
  stale manual shifts and uses automatic marker-based grouping.

This is deterministic and user-directed; it does not pretend the client
can infer unmarked art seams from metadata the server did not send.

## Possible automatic follow-up

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

Practical notes for an automatic fallback:

- The landscape-aspect check is free from metadata: `MangaPage` already
  carries `width` and `height`, so no decode is needed. It does not help
  One Piece, though: the Romance Dawn splash is delivered as two portrait
  halves (1080x1620 each), not one wide image, and comparing widths does
  not help either, because every page of these English chapters has the
  same size. The inner-edge check is the one that catches that case, and
  it needs pixels. The four chapters in "Parity luck already fails" behave
  the same way (portrait halves), which makes them the natural test cases:
  each has a spread at an even offset, where the current default is wrong,
  and #215 adds the mixed-parity case.
- "Inner edge carries ink" alone is not enough. A panel that bleeds to the
  page edge looks the same, which made a naive ink-fraction probe noisy
  during this audit. The robust signal is to correlate the two inner-edge
  strips and confirm the art actually continues across the seam, not just
  that ink is present.
- The fallback only helps a chapter that has an unmarked spread to anchor
  on. A chapter that is genuinely all single pages still has no cheap way
  to know its parity (page-number OCR is fragile), but there is no spread
  to tear there, so cover-solo stays acceptable.
