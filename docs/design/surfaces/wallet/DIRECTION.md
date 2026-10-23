# Wallet

**Skeleton.** A vertical stack of passes with one pulled forward: up/down moves which pass is pulled, A plays the pulled game pass, left/right picks a gate (THIS DEVICE, ZAO) on a two-place pass, where no gate is lit until the person picks one (dashed chips until then), B drops the pass back into the stack. Settings, Wi-Fi and Identity are three issuer cards pinned in the pocket at the top of the stack (inset, narrower than game passes, no cover); their compressed edges carry their live values (mini; hummingbird-lake 72%; 1 person, Backed up), so the route to them is visible from the library.

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


## Capability map

C1  Loading: edges without values; Error: a Korri issuer card under Identity carries the message as its field and a Reload chip; Empty: the three issuer cards and a lining line "No passes yet"; Ready: the stack
C2  Passes in catalog order; a section is a lining-ink label between edges (a pocket tab); subtitle takes the edge row's system slot; the cover fills the strip; no cover = title strip
C3  Fields row on the pulled pass: Last played, Plays, Play time and Gate; the barcode foot is the play tally, one bar per play; the Resume chip on the edge row marks resumable
C4  A plays the pulled pass through the lit gate; on a two-gate pass no gate is lit until left/right picks one, A with none lit shakes the gates
C5  options flips the pulled pass to its back: gameActions as rows with their description, dimmed when disabled; a destructive row is red-ruled and needs a second confirm chip
C6  Up past Settings pulls a Find slip from the pocket's top: a thin lining-coloured pass with a text field and the back's keyboard; typed letters fold non-matching passes to slivers
C7  A notification strip drops under the status row in the pass's own colour: pass name, kicker and detail; host actions (Cancel) as chips at its right; the pass stays pulled
C8  The strip stays, reading the kicker, and a Running chip replaces Resume on the pulled pass's edge row; up/down still browse
C9  The strip in the game's colour names gameTitle with the kicker and the reason; Retry (when canRetry) and Dismiss as chips at its right
C10 The Korri issuer card's Reload chip (C1); also the strip's only chip when the model needs a fresh start
C11 No answer yet
C12 The Settings issuer card pulled open in place: each group a run of fields under a width-80 group line, label small, value large, description as a third line on the focused field
C13 Choice: the value cycles with left/right as gate chips do; text flips the card to its back; sensitiveText shows dots and a Clear chip; action is a chip, confirmed on a back face with the message
C14 Saving: the field's value reads Saving… in the label weight; Problem: the field turns red-ruled with the message under it and B dismisses it
C15 Device actions are chips in the Settings card's Actions field, the last row of the card; system pulls the card to that row
C16 Identity card › Back up chip: the back asks the password, then shows the encryptedSecret as a QR with the text as the card's barcode foot
C17 Switch chip: the back takes the secret then the password, Transfer / Delete as two gate-style chips, a "This device loses trust" row with its own confirm chip
C18 No answer yet
C19 The Retired keys field lists retiredPublicKeys as rows; each Delete chip stays dimmed until that row's Backed up chip is lit
C20 Working: the card's edge value reads the operation; Problem: the message as a red-ruled field; Switched: People shows the new ownerPublicKey with a Switched chip; B dismisses
C21 The Wi-Fi card's edge: SSID and 72% for Wifi, Wired for Wired, Offline for Offline
C22 The pulled Wi-Fi card lists networks strongest first (name, %, lock when secured, Known); Scanning… as the edge value; No Wi-Fi adapter as the only field; the attempt is a field under the list, Try again when rejected
C23 A on a row flips the card to its back for the password; the Join a hidden network field (drawn) flips to the back for the SSID then the password
C24 No answer yet
C25 No answer yet
C26 The status row: clockLabel left, buildLabel right; either side empties when absent
C27 Nothing: the stack stays as it is; no attract, no screensaver
C28 menu pulls the Settings card; options flips the pulled pass to its back; system pulls the Settings card to its Actions row; up/down reach every card and the Find slip
C29 A pass flips to its back: the text field across its top (40 characters fit at 18 px on a 720 pass) over a 10 × 4 key grid plus Space, Delete, Shift, Done; A types, B deletes, holding B flips back
C30 Focus is the pulled pass (forward, wider, hard shadow) plus the lit gate chip or lit field; top/height ease 0.18 s; prefers-reduced-motion removes the transition
C31 The notification strip under the status row in the pass's own colour (lining when no game): pass name, kicker and detail; the host's actions as chips at its right, in the host's order and words; the pass stays pulled
C32 The Busy strip draws a hairline bar across its foot, filled to done over total, "1.2 of 3.4 GB" beside it; with no total "1.2 GB so far" and no bar; with no progress the strip alone
C33 A Download field joins the fields row with the size (3.4 GB), or Needed with none
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
