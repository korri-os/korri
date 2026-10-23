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

## Capability map

C1  Loading: a bare wall, "Reading the files."; Error: the message line speaks it; Empty: NOTHING ON FILE on the wall; Ready: a framed cover
C2  The wall: the focused cover framed, a wide one in a wide frame, neighbours darkened; no art: NO PICTURE ON FILE; a section is its name painted between frames
C3  The message line: "Last opened 9 days ago; played 7 times; 3 h 20 m in all."; resumable: the magenta HELD gem on the frame and "// HELD" on the FILE line
C4  The PLACE pill prints the place (PLACE: HERE / PLACE: ZAO), left/right or A flips it; a one-place game dims its gem; PLAY or A on the wall opens it there
C5  Down on the wall opens the game's COMMANDS section: a gem row per action, dim when disabled, its description spoken; a destructive one asks "Sure? A yes."
C6  The logo screen, up from PLAY: A opens the character grid, the wall hangs only titles that hold the draft and the logo reads FIND; B clears
C7  The message line speaks the kicker, then the detail, the hourglass turning; the host's actions (CANCEL) take the pills' places; the wall hangs its game
C8  The PLAY gem stays orange, the FILE line ends "// OPEN" and the message line speaks the kicker with the title
C9  The message line names the game: "Spelunky did not open. <reason>"; RETRY (when canRetry) and DISMISS take the pills' places; the wall hangs that game
C10 "RE-READ FILES", the last row of the SETTINGS section, and the pill offered under a read error
C11 COMMANDS' RUN WITH row: a row per route, program and build, a SAVED gem on the saved one; warnings spoken; the foot band reads STALE, CONFLICT or BUSY
C12 SETTINGS opens the DEVICE section: each group a teal band with its title, each item a row "LABEL  value"; the message line speaks the description
C13 A on a row: a choice steps left/right; text and sensitiveText open the character grid (dots, a clearLabel row); an action runs, asking first when it must
C14 Saving: the row's value becomes the turning hourglass; Problem: the message line speaks the message and B dismisses it
C15 The ACTIONS band at the foot of the SETTINGS section: one gem row per device action, dim when disabled; A runs one
C16 IDENTITY's BACK UP row: the password on the character grid, then the frame hangs the QR code and the message line prints the secret as two 20-character lines
C17 IDENTITY's SWITCH row, FROM BACKUP: secret then password on the grid, TRANSFER / DELETE gems, then "Other devices will stop trusting this one. Sure?"
C18 IDENTITY's SWITCH row, BY BUNKER: the URI on the character grid, the draft scrolling left, then the same TRANSFER / DELETE gems and trust question as C17
C19 IDENTITY's RETIRED KEYS row opens a KEYS section, one key per row in short form; A offers DELETE, which the message line gates with "Backed up? A yes."
C20 The message line: Working speaks the operation with the hourglass, Problem the message, Switched "New owner:" and the key; B dismisses
C21 The four-bar signal meter on the upper bezel, three lit for 72%, and NETWORK's row "hummingbird-lake 72%"; Wired prints WIRED, no bars; Offline dark bars
C22 The NETWORK section: SCAN, networks strongest first with bars, LOCK, KNOWN; the band reads SCANNING; Connecting and Failed are spoken; no adapter: NO WI-FI
C23 A on a network row joins; secured and not known opens the grid for the password, a rejected one reopens it; HIDDEN NETWORK asks the name, then the password
C24 The chassis over the game, its title in the band: gem rows; a toggle shows its label, a choice steps, a range is a teal bar and value; dim rows say why; B closes
C25 The opening: the message line speaks each step; a CONTROLLERS section lights a gem per pad HEARD; NETWORK, NAME and IDENTITY run as sections; BEGIN finishes
C26 The upper bezel: the clock at its left, "KORRI 0.4.1 9A1FB22" at its right before the signal meter, both in Micro 5
C27 Nothing beyond the idle motion: the hourglass blinks, a gem glints every 4 s, the logo dither scrolls. No attract, no screensaver
C28 options lights SETTINGS, menu lights PLAY, system does nothing; COMMANDS is down on the wall and FIND up from PLAY, so the d-pad, A and B reach everything
C29 The viewport becomes a two-column character grid, the message line the draft with a caret after KORRI:; d-pad moves, A types, B deletes, menu commits
C30 Focus is a white pill label and orange gem, the viewport's white border, or a row's pink gem, always drawn; motion is steps(), off under reduced motion
C31 Over its game's frame, the message line asks the kicker, then the detail; the host's actions take the pills' places in the host's order and words
C32 The FILE line becomes a teal bar filled to done over total, "1.2/3.4 GB" after it; with no total "1.2 GB IN" and no bar; with no progress the hourglass alone
C33 The message line adds "It needs a 3.4 GB download first." ("It needs a download first." with no size), and the FILE line ends "// DL 3.4 GB"
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
