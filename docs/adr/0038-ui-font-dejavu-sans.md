# 0038: UI font: DejaVu Sans (task 2.10, Q2)

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
PLAN §4 left the UI font open (open question 2): an open humanist sans in the spirit of
Lucida Grande, with Fira Sans (ADR 0013's placeholder), Source Sans 3, IBM Plex Sans and
Noto Sans as candidates. The owner compared them, plus DejaVu Sans, side by side at the
type scale in the gallery (Regular weight; Display, Title, Headline, Body and Small
samples with look-alike glyphs and numerals) and picked DejaVu Sans.

## Decision
1. **DejaVu Sans Book 2.37** is the UI font. `neue_theme::typography::UI_FAMILY` is
   `"DejaVu Sans"`, and `crates/neue-paint/fonts/DejaVuSans.ttf` replaces
   `FiraSans-Regular.ttf` (and its `OFL.txt`). The type scale sizes are unchanged. The
   neue-platform demo and the neue-comp platform test embed the same file.
2. **Licence.** DejaVu is not under the OFL: it is the Bitstream Vera licence plus
   public-domain changes (`fonts/LICENSE-DejaVuSans.txt`). That licence allows use,
   copying, embedding and redistribution with any software, only forbidding sale of the
   fonts by themselves and the use of the "Bitstream Vera" and "DejaVu" names for
   modified fonts. It is on the FSF's list of GPL-compatible free licences, so it is
   compatible with our GPL-3.0-or-later binaries. The licence text ships next to the font.
3. **Goldens.** The 88 goldens that contain text (gallery buttons, title bars, button
   materials) were re-blessed after inspecting samples.
4. **Comparison tooling is not kept.** The temporary Fonts page needed multi-font
   registration (`extra_fonts` on `neue_platform::Options` and `neue_kit::Settings`,
   `Label::family`) and about 2.2 MB of candidate fonts. All of it was removed after the
   decision; it remains in the branch history if a future font question needs it.
   `NEUE_GALLERY_PAGE=<n>` (open the gallery on page n) stays, for screenshots.

## Consequences
- Only Regular is bundled, as before. Bold and italic need adding when a widget needs
  them (DejaVu Sans Bold and Oblique are in the same release, same licence).
- DejaVu is wider than Fira Sans at the same size, so labels and buttons are wider.
  Layout is content-sized so nothing is clipped, but fixed-width layouts later should be
  checked against it.
- Like Fira Sans, DejaVu Sans covers Latin, Greek, Cyrillic and many symbols, but no CJK
  (ime_probe note, ADR 0030 fallback story unchanged).
- Every `neue-paint` user still embeds the font (about 750 KB, up from 450 KB).

## Alternatives considered
- **Fira Sans, Source Sans 3, IBM Plex Sans, Noto Sans** (all OFL): rejected by the
  owner's visual comparison.
- **Keeping the comparison page** in the gallery: needs five bundled fonts and a
  platform API that only it uses.

## Revisit when
- A weight or style beyond Regular is needed.
- The Terminal monospace face is chosen (the rest of Q2).
