# Mania — zone select

> **Superseded in part (2026-10-20).** The refinement brief, `docs/design/surfaces/zone/REFINEMENT.md`, supersedes this document's **Palette** (the "eight colours" rule: each screen group now has its own sampled palette, switched by `data-ground`) and its **Type** ("one face: Anton": Zone draws its own pixel capitals, Zone Caps, and sets Korri's facts in Jersey 10), and restates every length and the plan's thresholds on an integer art-pixel grid. Where the two disagree, the refinement brief wins. The rest stands as the record of the mockup.

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


## Capability map

C1  Loading: WAITING cards in the band (COMPETITION's form); Error: the blue dialog cut by black wedges with the message and one RETRY parallelogram; Empty: one NO GAMES card; Ready: the band
C2  Cards in catalog order; a section is a black header parallelogram above the band, redrawn as focus crosses a run; subtitle takes the tag row; cover uncropped; no cover = stripe well
C3  Save-card facts on the focused card, as MANIA MODE's CPZ card carries x03: clock glyph + last played, x7 plays, play time under the tags; the CONTINUE ribbon marks resumable
C4  The action row under the focused card: PLAY HERE / PLAY ON ZAO (CONTINUE HERE when resumable); a one-place game gets one button; A on the card drops into the row
C5  GAME OPTS. (MANIA MODE's Y keycap): options opens rows for the focused card, label and action parallelograms, dimmed when disabled, description under the row; destructive ask NO / YES
C6  A on the GAMES banner (or options on the band) raises the keyboard panel as a filter: each letter trims the band to matching cards; B clears and drops it
C7  The action row becomes one black parallelogram with the kicker, detail under it, and the cover well greys as PAUSED greys the game; host actions (CANCEL) sit beside it as parallelograms
C8  The band greys and the focused card's action row reads the kicker as one lit parallelogram; cards cannot be entered until Browsing returns
C9  The OPTIONS dialog: blue panel cut by black wedges, kicker as headline, reason under it, gameTitle in yellow; DISMISS and RETRY (when canRetry) as NO / YES parallelograms
C10 The same dialog with one RELOAD parallelogram when the catalog is in Error; A runs reload()
C11 No answer yet
C12 OPTIONS tab: Mania's OPTIONS rows, each group a black header (as CONTROLS, VIDEO), label parallelogram left, value right, description under the focused row, red underline on focus
C13 Choice: red triangles flank the value, left/right change it; text/sensitiveText raise the keyboard (dots, X RESET for clearLabel); action rows fire on A, confirmed by the blue NO / YES dialog
C14 Saving: the value parallelogram blinks until Korri answers; Problem: the blue dialog with the message and one OK parallelogram, A dismisses
C15 Device actions head the OPTIONS rows as a SYSTEM group of action rows; system jumps to it
C16 SAVE DATA tab: a vertical save-card select; the BACK UP card asks the password on the keyboard, then its well shows the QR and the secret scrolls on a white slab under it
C17 The SWITCH card (the NEW card form): keyboard takes the secret then the password, TRANSFER / DELETE as two parallelograms with red triangles, then the dialog THIS DEVICE LOSES TRUST
C18 No answer yet
C19 Retired keys are greyed save cards after the owner's card; DELETE on one opens the blue dialog BACKED UP? NO / YES and fires only on YES
C20 Working: the card's well greys with the operation in a black parallelogram; Problem: the blue dialog with OK; Switched: the owner card redraws with the new key under a SWITCHED ribbon; B dismisses
C21 The foot badge: SSID, four bars and 72% for Wifi; WIRED with full bars for Wired; OFFLINE with empty bars for Offline
C22 OPTIONS › WI-FI rows strongest first: SSID left, bars and % right, lock when secured, KNOWN tag; scanning shows --'--"-- dashes; NO WI-FI ADAPTER as one row; the attempt as a line under the list, TRY AGAIN when rejected
C23 A on a row joins it, the keyboard panel asking the password when needed; HIDDEN NETWORK is the last row and asks the SSID then the password
C24 PAUSED: red band with the title, game greyscale behind, yellow wedge of overlay controls then each group; values change with red triangles, dimmed rows show why, destructive ask NO / YES; B dismisses
C25 First boot: a COMPETITION row of WAITING cards that flip to a controller's name when heard, then Wi-Fi and device-name OPTIONS rows, then a NEW save card for identity; last A is finishOnboarding
C26 TIME 21:47 in the foot is clockLabel (the timer hides when absent); buildLabel is the tiny black string at the foot's right
C27 Nothing: the two orbit rings keep drifting; no attract and no screensaver
C28 menu jumps to the tabs; options opens GAME OPTS. for the focused card; system jumps to the SYSTEM group on OPTIONS; up to the tabs, then left/right and A, reaches it all
C29 The keyboard panel from the foot strip: black slanted keys rise over the action row with the snap, white slab with a red caret; A types, B deletes, options shifts, menu confirms; 40 chars scroll the slab
C30 Focus is always one of: pink edge and red triangles on a card, pink underline on a button, pink bar and lift on a tab; snaps bounce 340 ms; prefers-reduced-motion stops all motion
C31 The OPTIONS dialog: blue panel cut by black wedges, kicker as headline, detail under it, gameTitle in yellow when there is one; one parallelogram per host action in the host's order and words, never cast as NO / YES
C32 The Busy parallelogram fills with a mint bar to done over total, 1.2 / 3.4 GB in white caps; with no total 1.2 GB and no bar; with no progress the parallelogram alone
C33 An orange DOWNLOAD tag joins the card's tag row with the size (DOWNLOAD 3.4 GB), DOWNLOAD alone with none
C34 No answer yet
C35 No answer yet
C36 No answer yet
C37 No answer yet
C38 No answer yet
C39 No answer yet
C40 No answer yet
C41 No answer yet
C42 No answer yet
C43 No answer yet
C44 No answer yet
C45 No answer yet
