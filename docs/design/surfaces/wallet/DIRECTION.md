# Wallet

**Skeleton.** A vertical stack of passes with one pulled forward: up/down moves which pass is pulled, A plays the pulled game pass, left/right picks a gate (THIS DEVICE, ZAO) on a two-place pass, B drops the pass back into the stack. Settings, Wi-Fi and Identity are three issuer cards pinned in the pocket at the top of the stack (inset, narrower than game passes, no cover); their compressed edges carry their live values (mini; hummingbird-lake 72%; 1 person, Backed up), so the route to them is visible from the library.

**Memorable element.** The stack compressing around the pulled pass: every other pass shows only its top edge (44 px at 720 high, 32 px at 480), the pulled pass comes forward to full height, 12 px wider, with one hard offset shadow. Passes that do not fit fold to 3 px coloured slivers at the ends of the stack, so the full length of the wallet is always implied.

**Colours.** Lining `#2f2d2a` (background), lining ink `#cfc8ba`, hairline `#4a4640`, bone `#f7f3e9` (light pass ink), ink `#1c1a18` (dark pass ink), drop `#141311` (the one shadow). Each pass's own colour is derived from its cover: the cover's most saturated tone, moved in lightness until it clears 4.5:1 against one of the two inks; the ink that clears becomes the pass's text colour (hand-picked for the mockup; the rule is the spec). Comet Courier has no cover: hue = hash(title) mod 360 at 48% saturation, 42% lightness, with its title set large in a darker shade as the strip. Issuer cards use a neutral family: bone `#d6d1c4`, slate `#3a4b63`, brass `#e4b94f`.

**Typeface.** Bricolage Grotesque, variable (weight 200–800, width 75–100, optical size), SIL Open Font License 1.1 (`OFL.txt` beside this file; source: google/fonts GitHub, ofl/bricolagegrotesque), embedded as base64 TTF. One family does every job through its axes: 600 for pass names and values, 400 for labels, width 85 for issuer lines, width 80 / 500 for the issuer cards so they read as a different kind of card, 800 / width 78 for the no-cover title strip.

**Pass anatomy.** Edge row (name left, Resume chip and system right) → cover strip → fields (Last played, Plays, Gate) → foot: a barcode that is the play tally, one 2 px bar per play at a pitch sized so 112 plays fit the pass; 0 plays shows "No plays yet". Nothing invented: the bars are the count, not timestamps.

**plan(width, height)** — all in `index.html`:
- `passW = min(width − 16, 720)`: centred with empty margins at 1280, fills the width at 640.
- `edge = height ≥ 600 ? 44 : 32`; issuer edge `40 : 24`; strip `64 + (height − 480)/6`, clamped 64–104; pulled pass widens by `12 : 4`.
- Visible compressed edges `n = floor((stackH − pulledH − hidden·3) / (edge − 3))`, split half below, the remainder above; the rest become slivers. 1280×720 → 3 above, 3 below. 640×480 → 2 above, 2 below.
- The issuer cards never scroll; a pulled issuer opens in place and the game region starts below it.
- Edge names truncate with an ellipsis; the pulled name wraps (Zelda wraps at 480 wide, see the code path).

**Stress set.** 0 plays (Tide Pool, Petal Quest), 112 plays (Tetris), 1 play (Comet Courier), no cover, long title, two gates, streams-from-zao gate, Never as last played, 3 → 12 passes at any height (n goes to 0 and everything becomes slivers before anything becomes unreachable). Not drawn: blank library (an empty pocket with the three issuer cards; proposed), loading (edges render without values), errors (an issuer card would carry the message as its field).

**Text entry.** A pass flips to its back: the back is the same colour, carries the text field across its top (the full 40-character identity string fits at 18 px on a 720 pass, wrapping at 640) and a keyboard grid below it (10 × 4 keys plus a row of Space, Delete, Shift, Done); d-pad moves across the grid, A types, B deletes, holding B flips the pass back without saving; the status row still shows the clock.

**Decided without asking.** Issuer cards are pinned rather than scrolling, so the route stays on screen with Spelunky pulled. Gate names are in caps because the brief names them so; nothing else on the surface is capitalised.
