# Mania — zone select

**Artifact.** Sonic Mania's menus (TIME ATTACK, MANIA MODE, OPTIONS): a mint ground with orbit rings and dots, thick diagonal bands top-left to bottom-right, black parallelogram panels with white chunky caps, a pink offset underline on the current item, red triangles as the cursor, one yellow-on-blue hexagonal title banner. Vector shapes at any size; pixel art only inside the cover wells.

**Palette (8, picked from the references).** mint `#3fe0a0` (ground, stripe band, signal bars); black `#000000` (every panel, card body, cover well); white `#ffffff` (caps); yellow `#ffd21e` (GAMES banner lettering, HERE tag, TIME label, A keycap); blue `#2d5be8` (banner, orbit rings, system tag, well border, lettering edge); orange `#f7931e` (big orbit ring and its dots, bands); red `#e8322a` (cursor triangles, ZAO tag, ribbon rule); pink `#e889b0` (focused card edge, lit underline, B keycap). Drop shadows are black at 45%.

**Type.** One face: Anton Regular (SIL OFL 1.1, fetched from google/fonts `ofl/anton/Anton-Regular.ttf`, embedded as base64 by `build.py`; licence in `fonts/OFL.txt`). Rubik Italic is gone: Anton's tall condensed heavy caps sit much closer to Mania's chunky menu lettering than a black italic does. All words are caps; white caps carry a 2px blue lower-right edge on black, a black outline on colour; the GAMES banner is yellow with a black extrusion.

**Skeleton (unchanged).** Top strip: B BACK keycap hint left, three mode tabs centred (GAMES = hex banner, OPTIONS → Settings + Wi-Fi, SAVE DATA → Identity), A CONFIRM keycap hint right. A horizontal band of game cards in catalog order, the focused one centred and larger. The focused card's action row under it. A foot strip: TIME 21:47 as a stage timer, the MINI device tag, the Wi-Fi SSID badge with signal bars, the build string tiny in black. Left/right moves between cards, up to tabs, down to actions; every card, tab and button is a native `<button>` that receives DOM focus.

**Card.** Based on the MANIA MODE save card (vertical, in a horizontal band) carrying the TIME ATTACK zone card's contents: black body with a hard offset shadow; the cover UNCROPPED at its own aspect inside a blue-edged black well (letterboxed, never cropped); the title in white caps, two lines max, shrunk to fit; a diagonal orange/blue/mint band; a tag row of joined parallelograms: system (blue), HERE (yellow), ZAO (red). A two-place game wears both HERE and ZAO on every card, focused or not (Wario Land 4). Celeste's continue mark is a black header strip with white/blue/red stripes reading CONTINUE, as on MANIA MODE's NEW card. No-cover games (Comet Courier, and the four that previously borrowed hue-rotated Pico art: Petal Quest, Zelda, Wario Land 4, Super Metroid) show three Mania stripe bands in their own colour order with outlined initials.

**Focus.** Cards: pink 4px edge, centred, larger, and two red triangles pointing at it that nudge; the end triangle dims at the first/last card. Buttons: pink offset underline on the lit (default) action, red triangles added on the focused one. Tabs: pink underline and a 4px lift (no triangles, tabs sit shoulder to shoulder).

**Memorable element.** The GAMES hex banner in yellow-on-blue extruded caps with a white band through it, used once; everything else is white caps on black.

**Motion (CSS).** The track and card size snap with one overshoot bounce (`cubic-bezier(.34,1.56,.64,1)`, 340ms); the red triangles nudge; the two orbit rings drift one turn in 140s/200s. `prefers-reduced-motion` turns all of it off.

**plan(width, height)** (in `src.html`, thresholds named):
- `visible = width >= 1000 ? 5 : 3` cards; the rest fade out of the band.
- `tall = height >= 600`: strips are 74/66/58 px (top/actions/foot) when tall, 56/52/46 otherwise; type sizes follow the same two steps.
- Focused card height = min(400, band − 2×26); card ratio 0.78; side cards 0.68 of the focused height; triangles 36 px wide (26 below 1000).
- If `focusW + (visible−1)·sideW` exceeds `width − gaps − 2×(tri+16) − 48`, every card scales down by the same factor so the full set fits; nothing is cut.
- The ground SVG uses `preserveAspectRatio="xMidYMid slice"`, so rings and bands scale with the container.
- Titles are measured at the planned card width (not mid-transition) and shrink to at most 50% before a third line would appear.
- Readouts yield first: the SSID truncates with an ellipsis; the build string is already tiny.

**Decisions taken without asking.** Mint ground over yellow (TIME ATTACK/MANIA MODE are the two card-band screens). Anton replaces Rubik. B keycap letter pink, A keycap letter yellow (Mania's A is green, which is not in the palette). The four borrowed covers became stripe wells rather than hand-drawn art. "mini" moved from the tab strip to the foot as a black tag.

**Text entry.** A black parallelogram keyboard panel rises from the foot strip with the same one-bounce snap and covers the action row: four rows of black slanted keys with white caps (letters, digits, a symbols row), the typed string on a white slab above with a red caret triangle, the focused key marked with the pink underline. D-pad moves across keys, A types, B deletes, `options` toggles shift/symbols, `menu` confirms. The 40-character identity string uses the same panel; the slab scrolls horizontally to keep the caret in view.
