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

## Capability map
C1  Loading: "Fetching page 200." while the header counter rolls (RELAY 147); Error: "Page 200 did not arrive", OK tries again; Empty: "No games on this service yet."
C2  Page 200 columns Pg, Game, System, Played; a section a cyan heading row; long titles wrap indented; a 12 x 5 mosaic cover, else the page number double high.
C3  Band rows 20 and 21: "Last played 9 days ago.", "7 plays.", total time after it; a resumable game adds "Resumes mid-game." in cyan.
C4  Band row 19: "Play on" with the chosen place a yellow cell, left/right move it, A plays there; one place reads "Plays on This device" or "Streams from zao".
C5  Page 20n, the game's own page: an action per row, description in cyan beneath; a disabled row loses its yellow number; a destructive row asks again in red.
C6  Page 250 Find: the entry page's character table fills row 4 and the listing under it keeps only titles holding those letters, each with its page number.
C7  Busy: the band turns red with the kicker double height and the detail on row 20; Fastext becomes the host's actions, red Cancel first.
C8  Running: page 20n with the kicker double height on the blue band; the header counter stops at RELAY 20n while the game is live.
C9  Problem: a red band naming the game on row 19, kicker double height, reason on rows 20 and 21; Fastext becomes red Dismiss and green Retry when it can.
C10 Reload: ask for the page again: roll 200 on the header and press A, which is also the error page's "Try again with OK".
C11 Page 20n subpage 2: a route per row (runnerBuild, program), red warning rows under it, the saved route yellow; state on the band; Fastext holds the actions.
C12 Page 600: each group a cyan heading row; each item a row, label white, value yellow at column 20, description on the next row indented in cyan.
C13 Choice: left/right roll the yellow value cell; text and secret: the entry page, a secret as mosaic blocks until Show; action: A on the row; confirm: green key, red Cancel.
C14 Saving: the value cell flashes, as teletext flash does, until it holds; Problem: a red band under the setting with the message, B clears it.
C15 Page 600's last block under a cyan DEVICE heading: one action per row, run with A.
C16 Page 690 row "Export backup" opens the entry page for the password; page 691 prints the secret as text across rows and again as a mosaic-block QR beside it.
C17 Page 692 Restore: the entry page for the secret, then the password, a Transfer/Delete row rolled with left/right, a red trust-loss row, green Confirm key.
C18 No answer yet
C19 Page 690 lists each retired key over two rows (64 hex in 40 cells); red Delete appears on Fastext only after the "Backed up" row is set to yes.
C20 Working: the counter rolls with the operation on the band; Problem: a red band with the message; Switched: the new key over two rows in yellow; B dismisses.
C21 Page 650's title band: "Offline", "Wired", or "Wi-Fi hummingbird-lake 72%"; the row 22 caption over the red key carries the one word.
C22 Page 650: a network per row strongest first, mosaic bars, a padlock cell when secured, "known" in cyan; NoAdapter a whole-page message; scanning rolls the counter; the attempt in the band, "OK to type it again"; green Scan.
C23 A on a network row opens the entry page for its password and joins; the last row "Hidden network" asks the SSID first, then the password.
C24 Page 300 in mix mode, text over the live picture as a set drew it: cyan group headings, a control per row, toggles and choices rolled left/right, a range shows its number.
C25 Pages 101 to 104 on first boot: controllers (a row each, green when heard), Wi-Fi (650), device name (entry page), identity; blue key Next, then Done.
C26 The clock at row 0 column 34 in yellow; the build label on page 600's title band, and row 22 says "600 Build" above the yellow key.
C27 Nothing: the page stays as it is, as a teletext page did; only the row 0 clock changes.
C28 options jumps to the yellow Settings key, menu to the blue Index key (both carry a keycap glyph); system does nothing; d-pad, A and B reach every page.
C29 The entry page: the field on row 4 (rows 4 and 5 for a 63-character password), the 95-character table on rows 8 to 19 walked with the d-pad, A writes, B deletes; Fastext becomes Delete, Space, Show/Hide, Done.
C30 Focus is reverse video: the yellow row, the yellow cell or the grown Fastext key, always one; pages cut, nothing tweens, reduced motion changes nothing.
C31 Confirm: the band turns magenta with the kicker double height, the game named on row 19 when it has one, the detail on rows 20 and 21; Fastext takes the host's actions in the host's order and words, red, green, yellow, blue.
C32 Progress: row 21 of the red band is a mosaic bar filled to done over total, "1.2 of 3.4 GB" after it; with no total only "1.2 GB so far", no bar; with no progress the header counter rolls alone.
C33 Band row 21 after the play facts, in cyan: "Needs a 3.4 GB download first."; with no size, "Needs a download first."
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
