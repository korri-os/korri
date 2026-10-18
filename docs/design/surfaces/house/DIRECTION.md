# HOUSE: Korri as a cutaway dollhouse

**Skeleton.** Korri is a three-storey house with the front wall gone. Every capability is a thing in a room. A small figure walks to it, and a one-line caption names what the figure stands in front of.

**Rooms.** Ground floor: den (game shelf, TV, this device's console), kitchen (notice board: Device settings), hall (router and SSID tag: Wi-Fi; wall clock 21:47). First floor: bedroom (drawer of keys: Identity; typewriter: text entry), landing (fuse box with the build painted on it). Attic: zao, the older console. Every room is a rectangle, and the rooms touch.

**Memorable element.** The gap in the shelf. The figure stands under it with Spelunky's box in hand. Two empty slots glow amber: one in the den console, one on zao in the attic. The choice of place is the choice of which console the figure carries the box to. Celeste Classic sits in the den console's first slot, with a green light: a paused session.

**Colours.** 24 colours, picked by hand. The first 16 sit near the C64 palette. Named:
- Night `#1C1846`: the sky, which takes all spare height.
- Brick `#883932`: the cut outer walls.
- Joist `#3A2A10`: floors, stairs and the roof void.
- Den orange `#8B5429`: the den's striped paper and the shelf.
- Slot amber `#FFB43C`: a slot that will take what the figure carries. It is also the clock digits.
- Lamp `#FFE9A8`: bulbs and paper notes. Light green `#94E089` is only for "paused, can resume" and Wi-Fi bars.

**Type.** Two faces, drawn from the Pico folder's glyph tables (`x11-5x7.glyphs.txt`, `tom-thumb.glyphs.txt`) pixel by pixel into the canvas, so text is on the house's grid. No woff2 rendering, no anti-aliasing.
- X11 fixed 5x7 for the caption strip: a home computer's status line, with real lowercase.
- Tom Thumb 3x5 for paint on things: room names, the notice board, the SSID tag, the fuse box and the clock. One glyph pixel is one house pixel.

**Scale rule.** `s = max(1, floor(min(W/320, H/240)))`. The canvas is `floor(W/s)` by `floor(H/s)` logical pixels, scaled by nearest neighbour. The house is a fixed 320 x 216 frame, centred and on the ground, above a 24-row caption strip. Spare width becomes garden (a tree and a street lamp when each side has 40 or more pixels). Spare height becomes sky. 640 x 480 gives s = 2 (320 x 240). 1280 x 720 gives s = 3 (426 x 240). The house never stretches and never goes to a fractional scale. `plan(W, H)` in `index.html` holds this rule.

**Caption strip.** It carries one sentence in a flat house voice, with no exclamation marks. The hint "A at a console puts it in. B puts it back." goes on the same line when it fits, and below the caption when it does not. A two-line caption (the Zelda title at 640) takes the hint's row. The hint text stays on the focused stop's button label.

**Input.** The figure is the only focus indicator. Each stop is a native button, and the figure draws the focus. Left and right walk from thing to thing and through the doorways. Up and down take the stairs (hall to landing to attic). A uses the thing in front of the figure: take a box, put it in a lit slot, read the board, open the drawer. B puts back what the figure carries or steps away. `options` walks the figure to the kitchen board. `menu` walks it to the hall.

**Stress cases.** No cover: Comet Courier is a plain box with a white label. Long title: no titles on the shelf, so the caption carries the title and wraps to two lines (`?stop=box9`). Zero games: bare planks and "The shelf is empty." More than 12: the shelf gets a second row above, then a second bookcase on the den's back wall. Loading: only the hall bulb is lit, and the caption reads "Waking up." Error: the caption names what failed, for example "The router is not answering." The room stays.

**Text entry (not drawn).** A uses the typewriter in the bedroom. The caption strip is replaced by a 40-wide letter grid: one typed line of 40 cells over 3 rows of 40 character cells (95 printable characters plus space, delete and done). One cell is 8 pixels wide, so 40 cells fill the 320-pixel house exactly. A 40-character identity string fills the typed line exactly. A 63-character Wi-Fi password takes two typed lines. The d-pad walks the grid. A strikes the letter, B deletes, and done hands the line back to the router or the drawer that asked for it. The house slides up by the grid's extra rows, so the bedroom and the figure stay in view, and the roof ridge leaves the top.

**Query parameters.** `?stop=<id>` places the figure (`box0` to `box11`, `tv`, `board`, `clock`, `router`, `drawer`, `typewriter`, `fuse`, `zao`, and the stairs). `?held=0` puts Spelunky back on the shelf.
