# msx-3: Famicom command-adventure, the game list is the command column

**Colours (16 total, Famicom register).** Ground `#000000`; border and text white `#FCFCFC`; accent (cursor, tags) Megami Tensei magenta `#D800CC`; dim grey `#7C7C7C` (inactive cursor, truncation tail, column headers); speaker green `#58D854` (KORRI:, signal and stat bars); quantise set: blues `#0058F8` `#3CBCFC`, teal `#008888`, orange `#F87800`, pink `#F85898`, green `#00A800`, skins `#F8D8A8` `#E09060` `#A05030`, deep blue `#0000A8`. Covers are quantised at runtime to the 13-colour set with a 4x4 Bayer dither; the accent and the speaker green never appear inside a cover.

**Type.** Pixel Operator 8 (Jayvee Enaguas), base64 from bauripalash/ttf-pixeloperator-fork. Its licence is CC0 1.0, not SIL OFL as the brief assumed; the real text is in OFL.txt with a note. 8 px on 8 px lines, letter-spacing -1 px everywhere (the face's 2 px gaps read loose at 1 px strokes). One face; the HELD tag, the build label and the original covers use a hand-drawn 3x5 caps bitmap.

**Frame and zoom.** A 256x224 screen on black. `plan(width,height)` returns only `zoom = max(1, floor(min(w/256, h/224)))`, centred at integer offsets; below 256x224 it stays at x1 and clips from the top-left (states/tiny-200x150.png). Nothing else depends on size.

**Final layout (logical px).** Side margins are 4, not 8: a 128 px cover needs a 128 px interior, so the portrait window is 134x134 at (4,8), the command box 112x134 at (140,8), and the extra 8 px let "Celeste Classic" + HELD share a line. Message box 248x40 at (4,144): three lines plus the ▼ row. Places table 248x32 at (4,184): a title line (PLACES FOR SPELUNKY (PC), build label far right, long titles truncate before the system) and up to two rows PLACE | ROUTE with values This device/local and zao/stream; the game's facts stay in the message box, not repeated per row. Clock tag at (10,4) and signal tag at (113,4) overlap the window's top border. Bottom 8 px black.

**Skeleton.** One command column: up/down moves ▶ through the twelve games, a dashed line, and Settings / Network / Identity, and the portrait window, message box and places table redraw for the focused line. Confirm on a one-place game plays it at once; on a two-place game, confirm (or right) moves the live ▶ into the places table (the list keeps a hollow grey ▷), up/down or left/right picks the row, confirm plays there, left or B returns. Confirm on a system line opens it inside the portrait window as a Megami Tensei status block with a ▶ on its rows; B returns. On play the message reads "Opening X on Y." and the ▼ becomes a blinking ■; B clears it.

**Memorable element.** The cover as the portrait: as big as the window allows at integer scale, dithered into the Famicom palette; the rows or columns a non-square cover does not reach carry drifting 08.png noise bands (accent and dim grey, 1–2 px, 200 ms steps) so the window is always full; square covers get no bands.

**Text entry (not drawn).** The portrait window becomes a character table in fixed Megami Tensei columns (a-z, A-Z, 0-9, symbols), the message box's first line becomes the draft with a block cursor, d-pad moves, A types, B deletes, menu commits; a 40-character identity string wraps onto the box's three lines.

**Decisions taken alone.** A sentence that will not fit three lines loses trailing clauses (the table still shows those facts; see ?case=long). Truncated titles end in a grey "..". The focused row keeps white text; only ▶ carries the accent. Focusing a system line already shows its block read-only; confirm enters it. ?case=many scrolls with a 4 px cut-off row at the edge that has more. Idle motion: ▼ blinks at 1 Hz, the live ▶ nudges 1 px every 2 s, NO PICTURE noise bands drift every 160 ms; steps() only, off under prefers-reduced-motion.

## Capability map

C1  Loading: "Reading the files.", a blinking ■; Error: the message box speaks it; Empty: "No games on file. Settings, Network and Identity are below the line."
C2  One command line per game, grey ".." when cut, a HELD tag; a section a grey caption line; the cover fills the portrait, noise bands fill a wide one's gaps
C3  The message box: "Last opened 9 days ago; played 7 times; 3 h 20 m in all."; resumable: the HELD tag and "Celeste Classic is waiting where you left it."
C4  One place: confirm plays at once; two places: the ▶ moves into the places table, the list keeps a hollow ▷, the person picks a row and confirm plays there
C5  Left on a game line (or options) turns the portrait into its command block: a ▶ row per action, grey when disabled, the description in the message box
C6  A Find line above Settings under the dashed line: confirm opens the character table, the column keeps only titles that hold the draft; B clears
C7  The message box speaks the kicker, then the detail, the ■ blinking; the host's actions (CANCEL) are places table rows; the column stays on its game
C8  The game's line carries an OPEN tag in the HELD caps and the message box speaks the kicker: "Spelunky is open on zao."
C9  The message box names the game: "Spelunky did not open. <reason>"; RETRY (when canRetry) and DISMISS are places table rows the ▶ reaches
C10 "Re-read files", the last row of the Settings block, and a places table row under a read error
C11 The command block's "Run with" row: a ▶ row per route, program and build, a SAVED tag on the saved one; warnings and the state in the message box
C12 Settings opens its status block in the portrait: a title line per group, a row "Label value" per item, a stat bar for a level; the description below
C13 Confirm on a row: a choice steps left/right; text and sensitiveText open the character table (dots, a clearLabel row); an action runs, asking when it must
C14 Saving: the row's value becomes a blinking ■; Problem: the message box speaks the message and B dismisses it
C15 An Actions title line at the foot of the Settings block, a ▶ row per device action, grey when disabled; confirm runs one
C16 The Identity block's "Back up" row: the password in the character table, then the portrait shows the QR code and the message box prints the secret
C17 Identity's "Switch" row, from backup: secret then password in the table, Transfer / Delete rows, then "Other devices will stop trusting this one. Sure?"
C18 Identity's "Switch" row, by bunker: the URI in the character table, wrapping over the box's three lines, then the same rows and trust question as C17
C19 Identity's "Retired keys" row opens a block of keys, one per row in short form; Delete asks "Backed up? A yes, B returns." first
C20 The message box: Working speaks the operation with the blinking ■, Problem the message, Switched "New owner:" and the key; B dismisses
C21 The signal tag on the portrait's top border, four green bars; WIRED in the HELD caps; Offline grey bars; the Network block's first row names the SSID and %
C22 The Network block: Scan, networks strongest first with stat bars, LOCK and KNOWN tags; scanning, Connecting and Failed are spoken; no adapter: "No Wi-Fi here."
C23 Confirm on a network joins; secured and not known opens the character table for the password, a rejected one reopens it; "Hidden network" asks the name first
C24 Over the game, the command box titled with the overlay: a ▶ row per control, a toggle its label, a choice steps, a range a stat bar; grey rows say why; B closes
C25 The opening: the message box speaks each step; the portrait lists controllers, tagged HEARD; Network, name and Identity run as their blocks; "Begin" finishes
C26 The clock tag on the portrait's top border; "korri 0.4.1 9a1fb22" far right on the places table's title line, in the grey caps
C27 Nothing beyond the idle motion: the ▼ blinks, the live ▶ nudges, noise bands drift. No attract, no screensaver
C28 options on a game line opens its command block, menu jumps the ▶ to Settings, system does nothing; left opens the command block too, so the d-pad reaches all
C29 The portrait becomes a character table in fixed columns, the message box's first line the draft with a block cursor; d-pad moves, A types, B deletes, menu commits
C30 The magenta ▶ marks focus, always drawn, a grey ▷ the list's place; the column scrolls, a cut-off row at the edge; steps() only, off under reduced motion
C31 On its game's line, the message box asks the kicker, then the detail; the host's actions are places table rows in the host's order and words, ▶ on the first
C32 Under the kicker, a green stat bar lit to done over total, "1.2/3.4 GB" beside it; with no total "1.2 GB in", no bar; with no progress the blinking ■ alone
C33 The message box adds "It needs a 3.4 GB download first." ("It needs a download first." with no size), and the game's line carries a DL tag
