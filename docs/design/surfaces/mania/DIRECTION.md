# Mania — zone select

**Artifact.** The Sonic Mania (2017) menu: a saturated blue ground with navy diagonal stripe bands, slanted zone cards, heavy italic caps in white with a hard ink drop edge, yellow chevrons, rounded badges, and a stage timer in the corner. Vector shapes at any size; pixel art only inside the card thumbnails.

**Palette (7).** Mania blue `#1f5be6` (ground); navy `#0c1f66` (cards, tabs, stripes); ink `#081040` (drop edges only); white `#ffffff`; yellow `#ffd21e` (focus edge, chevrons, active tab, timer label); red `#e6322a` (zao badges, Comet Courier placeholder); green `#22c34d` (the lit "go" button, the Continue ribbon).

**Type.** One face: Rubik Italic variable (SIL OFL 1.1, fetched from google/fonts `ofl/rubik/Rubik-Italic[wght].ttf`, embedded as base64; licence in `fonts/OFL.txt`). Weight 900 for every word on screen; weights 500–800 of the same italic for the small facts (sub-lines, pills, SSID, build string). No second family. Chosen because its black italic has the rounded, slightly squared bowls of Mania's menu lettering and an authentic heavy italic rather than a slanted roman.

**Skeleton.** One horizontal band of slanted zone cards (the 12 games in catalog order) with the focused card centred and large; up reaches three slanted mode tabs (GAMES, OPTIONS → Settings + Wi-Fi, SAVE DATA → Identity); down reaches the focused card's action row; the bottom strip is a stage timer (clock), a Wi-Fi badge and the build string.

**Memorable element.** The focused zone card: it lifts, tilts four degrees further than its neighbours, gains a yellow edge, and two yellow double-chevrons nudge at it from either side. Everything else holds still.

**Voice.** Single verbs in caps: PLAY, CONTINUE, PLAY HERE, PLAY ON ZAO. No hints, no sentences.

**plan(width, height)** (in `index.html`, all thresholds named):
- `visible = width >= 1000 ? 5 : 3` cards; the rest slide out of the band, chevrons dim at the ends.
- `tall = height >= 600`: strips are 64/62/54 px (tabs/actions/foot) when tall, 50/48/42 otherwise.
- Focused card height = min(380, band height − 2×28); card ratio 0.82; side cards 0.72 of the focused height.
- If `focusW + (visible−1)·sideW + slant overhang` exceeds `width − gaps − chevrons − 48`, every card scales down by the same factor so the full set always fits; nothing is cut.
- The band and the action row are centred as one group between tabs and foot; tabs and action row never leave the screen.
- Titles fit two lines: the JS shrinks the italic caps to 55% of the card's base size before it would wrap a third line (Zelda).
- Readouts yield first: the SSID truncates with an ellipsis, the build string is already tiny.

**States built.** Card focus (default), tab focus (`?zone=tabs&tab=1`), action focus (`?zone=actions&action=1`), pressed (A on a button), one-place and two-place cards, resumable (Celeste: green ribbon, button says CONTINUE), no-cover (Comet Courier: red stripes, "CC"), long title (Zelda), first/last card (one chevron dims). Reduced motion removes the bounce and the chevron nudge.

**Text entry.** A slanted keyboard panel in the card language slides up from the bottom strip with a bounce and covers the action row: 4 rows of slanted navy keys (letters, digits, symbols as a fourth row), the typed string shown as a white slab above the keys with a yellow caret, d-pad moves across keys, A types, B deletes, `options` toggles shift/symbols, `menu` confirms. The 40-character identity string uses the same panel; the slab scrolls horizontally to keep the caret in view.
