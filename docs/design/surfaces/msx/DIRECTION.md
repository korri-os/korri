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
