# Surface direction mockups

Single-screen, high-fidelity mockups of candidate Korri surfaces, made in
October 2026 to find directions whose *skeleton* differs from every surface
already built. A surface is a deployable with its own navigation model, not a
theme; two directions with the same answer to "what moves when you press a
direction" are one surface with two skins, and one of them was cut.

Each folder holds one self-contained `index.html` (fonts and pictures inlined;
opens from a checkout with no server), screenshots at 1280×720 and 640×480,
and a `DIRECTION.md` with the palette, the typeface and its licence, the
skeleton in one sentence, the memorable element, and the text-entry decision.
Font licence texts sit beside the file when a libre face was embedded.

Every mockup draws the same screen from the same content so skeletons can be
compared: the library in idle state, twelve games, Spelunky focused with two
places it can play (this device and `zao`), Celeste Classic resumable, Comet
Courier without cover art, one long title. The brief the agents worked from is
summarised at the end of this file.

## Kept

| Folder | Direction | Skeleton | Status |
|---|---|---|---|
| `teletext/` | Broadcast teletext service | Numbered pages, 40×24 cells, Fastext colour row as the last focus stop | Usable |
| `catalogue/` | Mail-order catalogue | Paper spreads, item numbers, order form at the back | Usable; refined 2026-10-19 (print texture, plates, A–Z order, loop clearance, vector covers) |
| `workbench/` | Amiga Workbench 2.x desktop | Overlapping windows and drawers; tile at small sizes | Usable. Cover-derived icons are ruled out (see below); use per-system chassis icons with the title as label |
| `adventure/` | LucasArts SCUMM room | One painted scene, 3×3 verb grid, inventory; sentence line "Use X with Y" | Under review |
| `house/` | Little Computer People cutaway | Side-view rooms, a figure walks; one capability per room | Under review |
| `prevue/` | Prevue Guide | Fixed split: preview above, grid below | Look kept, grid rejected: a time axis makes no sense for a launcher. Needs a grid whose axes are things a person scans a library by |
| `msx/` | MSX2 / Metal Gear / 2064: Read Only Memories | File list, dossier, transceiver strip | Under review |
| `arcade/` | Multi-game cabinet select screen, 8/16-bit era | Slot strip, high-score table as facts, operator menu for settings | Under review. Kept as is; `arcade-modern/` is its sibling, not its revision |
| `arcade-modern/` | Naomi-era cabinet attract and select screens, about 1998 to 2002 | Two states: an attract reel of tilted title cards, and a rotating ring with the focused game on a lit plinth, a ranking panel, linked-cabinet places, a test-mode service line | Under review. `shot-attract-1280x720.png` shows the attract state |
| `mania/` | Sonic Mania menus | Slanted zone cards, three mode tabs, action row | Under review |
| `wallet/` | Digital passes | Vertical stack, one pass pulled forward, places as gates | Under review |

Built surfaces, for the comparison: Pico (`surfaces/pico`, layered screens with
cartridges on a shelf) and Boxbuster (`surfaces/boxbuster`, fixed camera spots
in a 3D store). A modern-console / streaming direction is planned as a
from-scratch rebuild of the retired Shift surface; its first mockup was dropped
(below) and it has not been redrawn.

## Dropped, and why

Do not propose these again without a new argument.

| Direction | Skeleton | Why it was dropped |
|---|---|---|
| Dial | One tuning band with mode bands | Abstract instrument with no world behind it; drifted to a tidy modern-app look |
| Board | Split-flap departures board, zero depth | Same: an instrument, not a place or an artifact with its own grammar; read as a table |
| Transit diagram | Beck-style lines and interchanges | Thin at twelve games; too much white card |
| Corkboard | Free 2D field of pinned paper | Dropped on review |
| Mosaic | Justified photo-library wall | Dropped on review: generic modern product UI |
| Palette | Command palette with ranked list and preview | Dropped on review: generic modern product UI |
| Console | Hero, rails, overlay panels (Shift's intended end) | Dropped on review: flat hero, cropped tiles, a rail collapsed at 640×480. The direction stays open; this drawing does not |

Lesson from three rounds: the directions that survived are each a real
artifact with hard formal rules that the mockup obeyed to the letter. The ones
that failed were either abstract instruments or contemporary app idioms that
any well-made product could wear.

## Parity bar

Pico is the only surface that covers the whole treaty
(`contracts/surface/korri-surface.ts`), so "do everything Pico does" is the
same as "house every field of `SurfaceModel` and every method of
`SurfaceHost`". A direction that goes forward must have an answer for each:

- Onboarding: controllers heard, Wi-Fi, name, identity, finish
- Catalog: browse, find by text, game detail, play history, launch locations
- Status: Busy with kicker, detail and actions; Problem with retry and dismiss; running
- Runner choice: routes, saved choices, warnings, actions
- Settings: groups of items; choice, text, range and action interactions; saving and failed states
- Device actions and per-game actions
- Identity: export backup with password (a QR code), switch from backup or NIP-46, delete a retired key
- Network: connection, scan, join a listed or hidden Wi-Fi with a password
- Gameplay overlay: controls and groups, a range held while it moves, dismiss
- Clock, build label, attract or idle
- Text entry for all of the above with a d-pad, A and B alone

Text entry is the item that sinks metaphors, which is why every
`DIRECTION.md` records that decision even though no mockup draws it.

## Cover art in a 4-colour surface

`docs/research/workbench-icon-quantise-drawn-covers.png` and
`docs/research/workbench-icon-quantise-real-covers.png` show real scanned box
art and hand-drawn pixel covers quantised to the Workbench 2.x palette at 48,
64 and 96 palette pixels, with an 8-colour column for comparison. On real
scans, 4 colours give noise at 48 and 64 and read for about half the covers at
96; 8 colours at 64 do better than 4 at 96. Hand-drawn pixel covers flatter the
method and should not be used to judge it. Conclusion: a 4-colour surface
cannot derive icons from covers at icon size; it needs a per-system chassis
icon with the title as the label, and shows the cover large only in the
game's own window.

## The shared brief, in short

- One screen: the library idle, Spelunky focused with two places, Celeste
  resumable, Comet Courier without a cover, Zelda's long title. The screen must
  make the skeleton legible, including how Settings, Network and Identity are
  reached.
- Layout is a function of the container: the same file must hold at 1280×720
  and 640×480 with nothing unreachable.
- Show only facts Korri publishes; invent only the surface's own words.
- No network at runtime; fonts embedded; licences recorded.
- Avoid the generated-design tells unless the artifact itself calls for one,
  and then own it fully.
- Spend boldness in one place.
