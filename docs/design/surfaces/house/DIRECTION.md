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

## Capability map

C1  Loading: only the hall bulb lit, caption "Waking up." Error: the caption names what failed, the room stays. Empty: bare planks, "The shelf is empty." Ready: boxes on the shelf
C2  Boxes on the den shelf; a section is its own plank with the caption painted on the plank end; title and subtitle read in the caption strip; no cover: a plain box with a white label
C3  The caption: "Last played 9 days ago, 7 plays, 3 hours." or "Not played yet."; resumable: the box sits in a console slot with a green light (Celeste in the den's first slot)
C4  A takes the box; every slot that can take it glows amber (den console, zao in the attic); the figure walks it there and A puts it in. One place lights one slot, never two
C5  Holding a box away from a console, A turns it over: the back-of-box actions list in the caption strip, dimmed when disabled; a destructive one asks "Really? A yes, B no"
C6  The typewriter: type part of a title and boxes that do not match go dark on the shelf; done walks the figure to the first lit box
C7  The console's slot light blinks amber, the CRT shows the kicker and detail, and the host's actions (Cancel) sit on the caption line as stops the d-pad reaches
C8  The CRT (den, or the attic set for zao) shows the kicker over the box cover and the slot light is steady; the figure sits on the floor in front of it
C9  The CRT goes to snow; the caption names the game: "Spelunky did not start. <reason>"; "A tries again" when it can, "B puts it back" dismisses
C10 The main switch in the fuse box on the landing: A flips it, the house goes dark and comes back to the hall bulb (the Loading state)
C11 No answer yet
C12 The kitchen notice board: one paper note per group, its title at the top; each line is "label  value"; the description reads in the caption when the line is focused
C13 A on a note line: a choice flips with left/right; text sends the figure to the typewriter (masked as dots, "clear" tears the line off); an action fires; destructive asks in the caption with its confirm label
C14 Saving: the line is written in pencil (grey) until it sets; Problem: a red-pinned slip on the board with the message, B takes it down
C15 Slips pinned on the fuse box door, one per device action, faded when disabled; A runs one
C16 The drawer in the bedroom: A on "Back up" asks a password at the typewriter, then a luggage tag hangs from the key: QR code on one face, the secret text on the other; B puts it away
C17 The drawer's "Put in a key from a backup": secret and password at the typewriter, keep or throw out the old key (transfer/delete), caption asks "Other devices will not know you. Go on?"
C18 The drawer's "Use a key kept elsewhere": the bunker URI at the typewriter, then the same keep/throw-out choice and the same question as C17
C19 Old keys on hooks at the back of the drawer, one per retired key with its short form on a tag; A offers Throw out, which first asks "Backed up? A yes"
C20 Working: the drawer light blinks and the caption reads the operation; Problem: a slip in the drawer with the message; Switched: a new key on the ring and the caption reads the owner key; B clears
C21 The router in the hall: Offline is a dark router and no tag; Wired is a cable into the wall and a tag reading "wired"; Wi-Fi is the SSID on the tag with one to four green bars
C22 A at the router unrolls the tag into a list down the hall wall: strongest first, bars, padlock if secured, tick if known; the light blinks while scanning; the tag reads "Joining…" or the failure, and "Type it again" when the password was rejected; no adapter: an empty shelf
C23 A on a listed network joins it; a locked one not yet known sends the figure to the typewriter for the password; the last line, "A hidden network", asks for the name first
C24 No answer yet
C25 First boot is moving-in day: bare rooms, and each step furnishes one: a pad in the den per controller heard, the router, the name on the fuse box, the key in the drawer; "Settle in" finishes
C26 The wall clock in the hall shows clockLabel in slot amber; buildLabel is painted on the fuse box on the landing
C27 No attract. After 2 minutes the figure sits down where it stands and the room lights dim one step, except the hall bulb; any press stands it up
C28 `options` walks the figure to the kitchen board, `menu` to the hall router, `system` to the fuse box; everything else is left/right along a floor, up/down on the stairs, A and B
C29 A at the typewriter: the caption strip becomes a 40-wide letter grid, one typed line over three rows of 40 cells; the d-pad walks it, A strikes, B deletes, done hands the line to the router or drawer that asked
C30 The figure is the only focus and is always drawn; it walks stop to stop at a few pixels a frame; under reduced motion it appears at the next stop and no bulb blinks
