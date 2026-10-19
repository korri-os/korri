# msx — a Metal Gear transceiver terminal, seen through 2064

**Frame.** 320×224 logical pixels, integer-zoomed to fit the container (×3 at 1280×720, letterboxed; ×2 at 640×480).
Below 320×224 it stays at ×1 and clips: a console screen has a fixed grid. Hard 1-px borders, no gradients, no blur.

**Colours (20, MSX2 register).** The six that carry the screen:
- night `#0b0920` screen ground; navy `#141c56` panel fill; indigo `#2a2a8c` header bands
- lilac `#9a8cf0` borders and file numbers; cyan `#40e8ff` focus band, systems, clock
- magenta `#ff40d8` the HELD tag only; amber `#ffc040` SYS files and the device name
Plus ice, white, dim, plum, violet, void, green (signal bars), teal, rust and four skin tones
(`#ffd8b0 #e8a070 #a05838 #502820`) that covers quantise into. The cover is drawn on a 64×64 canvas
and every pixel is snapped to the nearest of these 20 colours at runtime.

**Type.** Ark Pixel 10px proportional (TakWolf, SIL OFL 1.1, fetched from the
ark-pixel-font GitHub release 2026.10.16, latin subset, embedded as base64; licence text in `OFL.txt`).
A true 1-px-stroke bitmap face with real lower case on a 10-px line, the closest libre thing to an MSX 8×8 ROM
font that still reads at ×2. One size everywhere; the clock is the same face at exactly 2×. The sys icons, the
no-cover file and the "mini" operator portrait are hand-drawn 8×8 / 32×32 bitmaps in the script.

**Skeleton.** A one-line-per-file list on the left (three pinned SYS files, a rule, then FILE
001–012 with number, title, system) and on the right a dossier for the focused file (cover as
portrait, title, system, last opened, plays, PLAY ON places) above a Metal Gear transceiver
strip where the device speaks.

**Memorable element.** The transceiver: the "mini" portrait window, the 2× cyan clock as the frequency
readout with the build label under it, Korri's dry message with a blinking cursor, and the signal meter.

**Input.** Up/down moves the list (focus is a solid cyan row, always visible). Left/right switches
THIS DEVICE / zao on a two-place file. A plays, B backs. Settings, Network and Identity are the three SYS
files at the top of the same list; focusing one fills the dossier. Stress cases: `?case=empty|one|long`, `?focus=N`.

**Text entry (not drawn).** The transceiver strip becomes a frequency entry: the message area
turns into a line editor showing the draft with the cursor, and three rows of the character set
(a–z, A–Z, 0–9, symbols) appear above it in the dossier area. D-pad moves across the rows,
A types the glyph, B deletes, `options` toggles the case row, `menu` commits. A 40-character
identity string is entered the way Snake dials 140.85: one glyph at a time, draft always shown.

**Decisions taken alone.** 320×224 over 256×224 (a title plus a system needs 32+ columns). One-line rows,
so Celeste's HELD tag and Zelda's wrap both show on the idle screen. HELD is the surface's word for a frozen
session. Build label printed as `korri 0.4.1 9a1fb22`, no middle dot.

## Capability map

C1  Loading: the FILES band reads "0 files", the rows print "Reading…" and the transceiver says "Stand by."; Error: the transceiver speaks the message; Empty: "No files. Add a game to the catalog."; Ready: FILE 001 onward
C2  One line per file: number, title, system, HELD tag; a section is a rule in the list with its caption in lilac; the cover is the dossier portrait, quantised to the 20 colours; no cover: the file icon and "no cover"
C3  The dossier: "Last opened 9 days ago", "7 plays", and a "3 h 20 m" line; HELD on the row and in the dossier band when a session can resume
C4  PLAY ON: THIS DEVICE / zao in the dossier; left/right picks, A plays; a one-place file shows one name and A plays it; the choice is never made for the person
C5  `options` on a file flips the dossier to its COMMANDS page: one line per action, dim when disabled, the description spoken by the transceiver; a destructive one makes the transceiver ask "Sure? A yes, B no."
C6  A fourth pinned SYS file, Find: A opens the frequency entry and the list keeps only titles that contain the draft; the band reads "FIND zel, 1 file"; B clears
C7  The transceiver speaks the kicker, then the detail, cursor blinking; the host's actions (CANCEL) print as lines under it that down reaches; the file's row carries an OPENING tag
C8  The file's row tag reads OPEN and the transceiver prints the kicker with the file number
C9  The transceiver names the file: "Spelunky did not open. <reason>"; RETRY (when canRetry) and DISMISS print as lines under it
C10 The terminal's reset: "Re-read all files", the last line of the SYS Settings list, and the line offered under a read error
C11 The file's RUN WITH page in the dossier: one line per route, program and build, the saved route tagged SAVED; route warnings and actions as lines under it; the band carries the state (STALE, CONFLICT, BUSY)
C12 SYS Settings opens into the FILES panel: each group a band with its title, each item a line "label  value"; the dossier shows the focused item's value and description
C13 A on an item: a choice steps with left/right; text and sensitiveText open the frequency entry (dots, plus a CLEAR line with clearLabel); an action runs; destructive: the transceiver asks with the title and message, A is the confirm label
C14 Saving: the item's value becomes the blinking cursor; Problem: the transceiver speaks the message and B dismisses it
C15 Calling the operator (`system`, or A on the mini window): the device actions print as lines in the message area, dim when disabled; A runs one
C16 SYS Identity's "Back up" line: the password on the frequency entry, then the dossier portrait becomes the QR code and the message area prints the secret in 20-character lines
C17 SYS Identity's "Switch from backup": the secret then the password on the frequency entry, TRANSFER / DELETE picked like PLAY ON, then the transceiver asks "Other devices will stop trusting this one. Sure?"
C18 SYS Identity's "Switch by bunker": the URI is dialled on the frequency entry the way Snake dials 140.85, then the same TRANSFER / DELETE and trust question as C17
C19 SYS Identity's "Retired keys" line opens them in the FILES panel, one per line in short form; A offers DELETE, which the transceiver gates with "Backed up? A yes."
C20 The transceiver: Working prints the operation with the cursor, Problem the message, Switched "New owner" and the key; B dismisses
C21 The signal meter at the foot of the transceiver: SSID, eight bars and %; Wired reads "wired" with no bars; Offline reads "no signal" with dark bars
C22 SYS Network opens into the FILES panel: "Scan again" first, then networks strongest first with bars, LOCK, KNOWN; the band reads SCANNING; Connecting and Failed are spoken, a rejected password reopens the frequency entry; no adapter: "No antenna."
C23 A on a network joins it; secured and not known opens the frequency entry for the password; the last line, "Hidden network", asks the name then the password
C24 The transceiver call over the game: the frame freezes behind the panel, title in the band, controls as lines (toggle shows its label, choice steps left/right, range is a frequency dial with the value); dim lines speak their reason; B hangs up
C25 First boot is the opening call: the transceiver speaks each step; controllers list as lines that turn HEARD; Network, name and Identity run as their SYS files; the last line, "Begin", finishes
C26 The frequency readout: the 2× cyan clock, buildLabel under it in lilac
C27 Nothing: the list stays as it is and only the message cursor blinks. No attract, no screensaver
C28 `options` opens the file's COMMANDS page, `menu` jumps to the SYS files at the top of the list, `system` calls the operator; everything else is up/down, left/right, A and B
C29 The transceiver strip becomes a frequency entry: the message area shows the draft with the cursor, three rows of the character set appear in the dossier; d-pad moves, A types, B deletes, `options` toggles case, `menu` commits
C30 The focus is a solid cyan row, always drawn; the list scrolls so it stays in view and a "more" mark shows what is below; the only motion is the cursor blink, off under reduced motion
