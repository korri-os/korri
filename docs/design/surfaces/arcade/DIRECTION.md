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
