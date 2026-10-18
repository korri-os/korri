# RELAY: Korri as a teletext service

**Skeleton.** Every screen is a numbered 40 x 24 page: 100 is the index, the 200s are the library (200 the list, 200 + n one game), 600 Settings, 650 Network, 690 Identity. Row 0 is the header: requested page, service name with page counter, device name, clock.

**Memorable element.** Row 23, Fastext: four solid keys of 10 cells each, with their page numbers and contents on row 22 (`650 Wi-Fi`, `690 Keys`, `600 Build`, `100 Pages`). A focused key grows up over its caption into a two-row block.

**Colours.** The eight teletext colours only, solid, no blends.
- Black `#000000`: the page, and the letterbox around it.
- White `#FFFFFF`: titles and facts.
- Yellow `#FFFF00`: page numbers, the clock, the focused row, the chosen place.
- Cyan `#00FFFF`: column heads, systems, hints.
- Blue `#0000FF`: the title band and the detail band.
- Red `#FF0000`, green `#00FF00`: Fastext, and the mosaic covers. Magenta is not used on P200.

**Type.** monogram by datagoblin (CC0), copied from the Pico font folder and embedded as base64 woff2. It is a 6 x 9 bitmap: 5-wide glyphs, 7-row capitals, 2-row descenders. Those are the proportions of the SAA5050 teletext generator. A cell is 8 x 10 glyph pixels and the glyph is drawn 4/3 wide, as a 4:3 set drew it. The title uses real double height.

**Grid.** The cell size comes from the container: the page is `min(100cqh, 75cqw)` tall, 4:3, letterboxed. 1280 x 720 gives 24 x 30 px cells, 640 x 480 gives 16 x 20. Nothing reflows. A title longer than 15 cells continues on the next row, indented, as listings pages did. Covers are 12 x 5 cells (24 x 15 blocks), two colours per cell. The detail band starts at column 12 with or without a cover. With no cover, the side column shows the game's page number in double height (Comet Courier, 207).

**Wi-Fi state and build.** Wi-Fi state lives on 650, build label on 600. Row 22 says so above the keys that go there.

**Input (treaty only: direction, confirm, back, options, menu).** d-pad + A + B alone reach everything; no chords, no treaty gap.
- Up/down walk the list. Up from the first game lands on the header page number. Down from the last game lands on Fastext, the last focus stop.
- On a game with two places, left/right choose the place in the band; A plays there.
- On Fastext, left/right choose a key; A follows it.
- On the header, left/right choose a digit, up/down roll it, A goes to the page. This is the "type a page number" path.
- B: from Fastext or the header, back to the list. From the list, to the blue Index key.
- `options` jumps to yellow Settings, `menu` to blue Index. Those two keys carry a small black keycap glyph. Red and green have no shortcut and are reached by focus.

**Text entry.** A 40-character secret fills exactly one row, so the entry page shows it whole:
- The field is row 4, one character per cell, a yellow cursor cell, `17/40` at the right of row 3. A secret shows as white mosaic blocks until it is shown.
- Rows 8 to 19 hold the character table: 95 printable characters, 16 per row, every other cell. The d-pad walks the table. A writes the character to the cursor cell and moves the cursor on. B deletes the cell before the cursor. Up from the top of the table lands on the field, where left/right move the cursor.
- On that page, Fastext changes to red Delete, green Space, yellow Show/Hide, blue Done.
- A Wi-Fi password can be 63 characters, so that field takes rows 4 and 5 (80 cells). It never scrolls sideways.

**States.** `?state=empty|loading|error` draws the other whole-page states. `?focus=n` focuses game n. `?zone=keys&key=n` focuses a Fastext key.
