# Arcade: the cabinet's own game-select screen

**Skeleton.** A 320×240 logical frame integer-scaled with black bars (×3 at 1280×720, ×2 at 640×480; `plan(w,h) = floor(min(w/320, h/240))`, min 1): credit/clock line and ROM string on top, marquee heading, a horizontal strip of 7 visible slot cards with the focused slot large in the middle, the focused game's high-score table below, INSERT COIN, and the operator line as the last focus stop.

**Palette** (saturated primaries on phosphor black, nothing tinted):
- Phosphor black `#000000`
- Marquee yellow `#F8D800` (heading, focus frame, 1P side, INSERT COIN)
- Cabinet red `#E82820` (heading shadow, CONTINUE? banner, cursor flash)
- Neo blue `#2858F8` (slot frames, labels, ROM string, operator, chrome shadow)
- Chrome cyan `#A0E8F8` (service readout, chrome band)
- Phosphor white `#F8F8F8` (values)

**Type.** *Press Start 2P* (SIL OFL 1.1, fetched from google/fonts, `fonts/`): marquee face for SELECT GAME, the focused title, INSERT COIN, and the no-cover house card. *Silkscreen* (SIL OFL 1.1, same source): compact 8px bitmap face for the table, the top line and the operator line. A hand-drawn 3×5 sticker font (SVG rects, in the file) for slot numbers, the CONTINUE? banner and the small no-cover card, because the small 38×28 slot cannot hold either fetched face. Everything sits on the logical pixel grid; `zoom` keeps it there.

**The one memorable element.** The focused title in hard-banded SNK chrome: six colour steps over the 16px em (white, cyan, blue, white horizon, yellow, red) with a one-step blue cast shadow; no gradient, no anti-aliasing.

**Table rules.** Title at 16px if it fits 300px, else 8px (Zelda). Rows: SYSTEM, PLAYS, LAST, then one row per place as 1P/2P; with two places the chosen side is yellow and marked YOUR SIDE. 0 plays and Never render as `0` and `NEVER`.

**Attract mode.** 30 s idle on the select screen hard-cuts to DEMO: one game's cover at 1× or 2× (integer only), its chrome title, its facts line, PRESS START blinking, cycling every 4 s through the 12 in catalog order; any press returns to the select screen with the previous focus intact. Reduced motion: no attract, no blink, static yellow focus frame. `?attract` shows the state; `?still` freezes blink and countdown for screenshots.

**Focus model.** Strip: left/right rotate the ring of 12 (wraps), down enters the side rows. Side rows: up/down between 1P/2P, A plays on that side, B back to strip; down past the last row reaches OPERATOR. Operator: left/right across SETTINGS · NETWORK · IDENTITY (yellow block = focus), A opens it as a service page, up/B returns. `menu` jumps straight to OPERATOR. Wi‑Fi `hummingbird-lake 72%` is the service readout on the top line; device `mini` closes the operator line.

**Text entry.** High-score initials entry, extended: a horizontal character strip (A–Z, a–z, 0–9, symbols, SPACE, END) scrolled with left/right and fast-stepped with up/down between groups; A enters the character into the next slot, B deletes the last slot; the entry is shown one character per slot above the strip like a name-entry line, with a blinking cursor on the active slot and the slot count (`12/40`) at the right.

**Cover art.** 7 covers from the Pico fixtures; Petal Quest, Zelda, Wario Land 4 and Super Metroid have no fixture picture and are drawn as 32×24 pixel SVGs in the same idiom. Comet Courier (no cover) gets the house card: black marquee, red top band, blue bottom band, title in marquee type.

## Capability map

C1  Loading: the strip is empty and the heading reads NOW LOADING; Error: a red ROM ERROR heading with the message in white where the table is; Empty: NO GAMES, SEE OPERATOR; Ready: the strip
C2  Numbered slots in a ring, in catalog order; a section is a blue divider card in the strip with its name; the subtitle is the SYSTEM row; the cover is the DEMO backdrop; no cover gets the house card
C3  The high-score table: PLAYS, LAST and TIME rows; a resumable game wears the red CONTINUE? banner with the countdown on its slot
C4  One place: A on the slot plays; two places: down to the 1P/2P rows, up/down picks the side, A plays there; no side is ever chosen for the player
C5  `options` on a slot opens its own DIP page in place of the table: one row per action, dim when disabled, description on the footer line; a destructive row asks ARE YOU SURE? YES/NO
C6  Up from the strip reaches the heading; A opens the initials strip under it, and each entered letter drops slots whose title does not contain the text; the ring shrinks to the matches
C7  The heading becomes the kicker (STARTING…), the detail under it in Silkscreen, the focused slot flashes; the host's actions (CANCEL) sit where INSERT COIN was as stops
C8  The DEMO layout with the kicker where DEMO was: the game's cover large, its chrome title, the facts line; no PRESS START
C9  A red banner across the table: the kicker, the gameTitle in chrome, the reason in white; stops under it: RETRY (when canRetry) and EXIT
C10 RESET, the last row of the SERVICE page in the operator menu, and the only stop on the ROM ERROR screen
C11 No answer yet
C12 The operator's SETTINGS page: a test-menu page, group titles as cyan headings, rows "LABEL   VALUE" in Silkscreen, the focused row's description on the footer line
C13 Test-menu rows: a choice steps with left/right; text and sensitiveText open the initials strip (asterisks, and a CLEAR stop with clearLabel); an action fires on A; destructive: a red banner with the title and message, YES is the confirm label
C14 Saving: the row's value reads "..." in cyan until it is written; Problem: a red banner with the message, B clears it
C15 The operator's SERVICE page: one row per device action, dim when disabled; A runs it
C16 The IDENTITY page's BACKUP row: the password on the initials strip, then the table area shows the QR code at 1× and the secret in 8px white, two lines of 20
C17 The IDENTITY page's RESTORE row: secret then password on the initials strip, a TRANSFER / DELETE two-stop row, a red banner LINKED CABINETS WILL NOT TRUST THIS ONE, YES/NO
C18 The IDENTITY page's LINK row: the bunker URI on the initials strip, then the same TRANSFER / DELETE row and red banner as C17
C19 The IDENTITY page's RETIRED rows, one per key in short form; A offers DELETE, behind a red banner BACKED UP? YES/NO
C20 The service readout on the top line: Working prints the operation; Problem is a red banner with the message; Switched a banner NEW OWNER with the key in 8px; B dismisses
C21 The service readout on the top line: HUMMINGBIRD-LAKE 72%, WIRED, or OFFLINE
C22 The operator's NETWORK page: SCAN row first, then networks strongest first with %, LOCK and KNOWN marks; SCANNING in the readout; Connecting and Failed in the readout with the message, a rejected password reopens the strip; NO ADAPTER when there is none
C23 A on a network row joins; secured and not known opens the initials strip for the password; the HIDDEN row asks the name then the password
C24 No answer yet
C25 First power-on runs the operator's test menu in order: INPUT TEST (each controller shows OK when heard), NETWORK, CABINET NAME, IDENTITY; the last row, EXIT TO GAME, finishes
C26 CREDIT 21:47 at the top left is clockLabel; the ROM string at the top right is buildLabel
C27 Attract: 30 s idle hard-cuts to DEMO, one game's cover and chrome title every 4 s in catalog order, PRESS START blinking; any press returns with the focus intact; no attract under reduced motion
C28 `options` opens the slot's DIP page, `menu` jumps to OPERATOR, `system` opens the SERVICE page; the rest is left/right on the ring, down to the sides and the operator, A and B
C29 High-score initials entry, extended: a character strip (A–Z, a–z, 0–9, symbols, SPACE, END) scrolled left/right, up/down jumps groups; A enters the next slot, B deletes the last; the line shows above with a blinking cursor and the count (12/40)
C30 Focus is the yellow slot frame (flashing red), the yellow triangle on a side row, the yellow block on the operator line; the ring slides one slot per press; reduced motion: static frame, no slide, no blink
C31 The heading becomes the kicker, the detail under it in Silkscreen, the gameTitle in chrome when there is one; the host's actions sit where INSERT COIN was as stops, in the host's order and words
C32 Under the Busy detail a bar of sticker blocks fills to done over total, 1.2/3.4 GB beside it; with no total 1.2 GB IN and no bar; with no progress only the focused slot flashes
C33 The high-score table adds a DOWNLOAD row with the size (3.4 GB), or NEEDED with none; the slot card wears a DL sticker
C34 No answer yet
C35 No answer yet
C36 No answer yet
C37 No answer yet
C38 No answer yet
C39 No answer yet
C40 No answer yet
C41 No answer yet
