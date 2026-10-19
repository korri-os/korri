# msx-2: a gallery wall inside a 2064 chassis
**Colours** (pixel-read from reference/04.png; 21 named colours in total, all in `P` in index.html):
chassis lavender `#8795be`, chassis outline `#3a1e7c`, chassis highlight `#dbc9e5`, pill black `#000000`,
gem pink `#fa93fd`, focus orange `#f2a24b`; plus message white `#ffffff`, speaker green `#b0e898`,
teal `#91f2da` (bands, hourglass), cyan `#3dc1d1` (indicator, signal), gem light `#ffdcfe`, cream `#fffbd7`,
plum `#590f63`, wine `#9f3257`, magenta `#ff3fae` (HELD gem), violet `#7b47c9`, dark teal `#2f8f8a`,
four skin tones `#ffd3b0 #e0a070 #b06040 #84302c`. Covers are quantised at runtime to `COVER_SET` with a 4x4
Bayer dither; neighbours are darkened (RGB x0.42) and re-quantised, which lands two palette steps down.
**Type**: two faces with roles, both by Sarah Cadigan-Fried, SIL OFL 1.1, fetched from google/fonts ofl/, embedded
base64, licences in OFL.txt. Jersey 10 at 18.667px (its 1-px grid: cap 10; line box 12 px by hand) for the message
line, pill caps, logo word and section previews. Micro 5 at 11px (cap 5) ONLY for the device prints: clock, FILE
line, build label, device name under the logo; it also letters the test card and drawn covers (pictures, not UI).
**Frame and zoom**: logical 320x180; `plan(width,height)` returns only `zoom = max(1, floor(min(W/320, H/180)))`
as CSS `zoom` on `#screen`; x4 fills 1280x720, x2 is 640x360 letterboxed at 640x480; below 320x180 it stays x1 and
clips right/bottom. Every edge is an integer; canvases are pixelated; corners are stepped masks; no gradients.
**Skeleton**: the viewport is a carousel of the catalog (left/right slides along the wall), the five pills are the
commands (up enters them at PLAY, down from IDENTITY or B returns), and the message line is the only voice.
**Memorable element**: a gallery wall inside the 2064 chassis. One drawn room fills the 196x112 viewport (back
wall with a dither band falling off upward, a strip window top-left, a shelf top-right, a floor line with a
lighter strip over warm boards); the focused cover hangs centred in a 2-px frame (lavender outer, dark inner) at
the largest integer scale with 8 px of wall around it; the previous and next games' frames peek in at both edges,
darkened, 6 px from the focused frame. 128-px covers show at x1/2 (64 px) by that rule; Spelunky 128x60 at x1.
**Text entry** (not drawn): the viewport becomes a two-column character grid in the 07.png manner (pill rows a-z /
A-Z / 0-9 / symbols); the message line becomes the draft with a blinking caret after `KORRI:`; d-pad moves, A types,
B deletes, menu commits. A 40-char identity string is ~300 px, wider than the 296-px line, so the draft scrolls left
to keep the caret visible once it passes 38 characters.
**Decisions taken alone**: Jersey 10 pill caps need 100-px pills (`PLACE: HERE` is 74 px plus two gems), so the left
panel is 104 px and the viewport 196 px; chassis x 4..315, y 3..134. The build label sits on the upper-right bezel
(left of the signal meter): on the lower bezel it collided with `FILE 01/12 // CELESTE CLASSIC // PICO-8 // HELD`.
A title longer than the lower bezel is cut with `…` and the message line carries it whole; a word wider than the
line breaks mid-word so the sentence stays at two lines. Focused pill text goes white as in 04. A one-place game
dims the PLACE gem and reads HERE. Confirm on PLAY/viewport blinks the orange gem (or the viewport's white border)
for 2.4 s with "Opening …"; B cancels.
Keys: arrows, Enter, Escape (back), o (options = SETTINGS), m (menu = PLAY). Idle motion: hourglass blink, gem glint
every 4 s, logo dither scroll; all steps(), off under prefers-reduced-motion. Stress set: states/shoot.sh.
