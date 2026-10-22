# Prevue: the Korri Guide channel

The artifact is the Prevue Guide of about 1993 to 1999, as it ran on a cable head-end Amiga. The screen is cut in two at a fixed line. The top 34% is the promo: a preview window and a sidebar. The bottom is the listings grid. The clock and a crawl sit in the bar between the two parts.

**Skeleton.** A two-axis grid: the channels are the rows (1 Settings, 2 Wi-Fi, 3 Identity, then 4 PICO-8 to 11 SNES), the slots are the columns (Today, Yesterday, This week, This month, Earlier, Never), and every game is a listing in the cell of its system and its last-played slot.

## Colours (hand-picked, flat, no gradients)
| Name | Hex | Job |
|---|---|---|
| Prevue blue | `#1f3fd2` | promo field, channel column, service-row blocks, slot header |
| Cell navy | `#0a1868` | game listing cells, logo extrusion |
| Marquee yellow | `#f8d81c` | clock, channel numbers, service channel tabs, focus, the logo face |
| Caption white | `#f6f6f0` | titles and listing text |
| Grid black | `#000000` | the grid lines, the bar and the Amiga drop shadow on all text |

Two derived bevel tints (`#25339a`, `#5a74f0`) draw the one-pixel top-left highlight on cells. They are fixed shades of the blues, not new hues.

## Type
**Kenney Pixel** (CC0, from the Pico woff2 folder), sized only in whole multiples of its 9-row em, so each glyph pixel is a square block. Every letter has a hard 1-glyph-pixel black drop shadow, as Prevue's Amiga character generator did. The face is chunky, proportional and squarish. It is not on the used list (Geist, TeX Gyre Heros Condensed, Spleen, monogram, m5x7, Tiny5). Its own K looks like an H, so the KORRI wordmark is drawn as pixel rects in the script, not typed.

## The one memorable element
The extruded **KORRI Guide** wordmark: yellow pixel letters with a three-step navy extrusion and a black cast shadow, beside a white "Guide" plate. Everything else stays in the grid's flat discipline.

## How the screen reads
- The slots are calendar slots counted back from Saturday 17 October, so they agree with the table. Wario Land 4 (Monday) is This week. Spelunky (8 Oct) is This month. Lantern Keep (26 Sep) is Earlier.
- The service channels carry wide "programme blocks" across the slots. Each block is a focus stop: Name, Sleep after, Brightness; the network, Scan, Join hidden; Backup state, Back up, Switch, Retired keys. Their channel tabs are marquee yellow, so they read as the channel's own rows.
- The cell marks: two pips (filled = This device, hollow = zao) on two-place games, a "zao" flag on stream-only games, and a yellow `▮▮ PAUSED` flag on Celeste Classic. The sidebar small print carries the pip legend and the build label.
- Focus turns the cell marquee yellow with a white rule. In a stacked cell, up and down step through its games before they leave the row.

## Size (plan(w,h) in the script is the single decision)
The split is a fixed 34% of the height. One glyph pixel is `max(2, floor(h/360))` px. Rows are two text lines high. At 1280×720 all 11 rows and all 6 slots fit. At 640×480 rows 1 to 7 show and row 8 is cut at the bottom. Slots narrower than about nine characters run off the right edge, and a cut fifth slot shows that more exist. The channel column and the slot header stay pinned while the grid scrolls to keep the focus in view. The sidebar text wraps. The channels hint drops because the service rows name themselves on screen. Long titles clamp to two lines with an ellipsis, and the full title is in the sidebar.

## Attract (not drawn)
After 60 s with no input, the guide runs as Prevue did. The grid crawls up one row every 4 s, wraps from channel 11 back to channel 1, and is never blank. The preview window and the sidebar cycle through the listings that are on screen, 8 s each, in slot order. The crawl in the bar always moves left at a steady pixel rate. Any input stops attract and returns the focus to the listing that had it. Under reduced motion, rows and the crawl step in whole jumps and do not scroll.

## Text entry: the remote's channel-entry row
Text entry takes over the bar between the two parts. The clock stays, and the crawl gives way to a one-line keypad strip, like a cable box's on-screen text entry. Left and right step along one row: `a–z`, then `0–9`, then the symbols, then `⌫ space ✓`. Up and down switch the row's case and set (`abc`, `ABC`, `123`, `#+=`). A types the letter, and B deletes it. The value under entry shows in the sidebar in big type, 20 characters per line, so a 40-character identity string reads as two lines. A Wi-Fi password shows as dots, and the newest letter stays visible for 1 s. The grid stays on screen above the strip, dimmed, so the person can still see which channel and which block they are filling in.

## Capability map

C1  Loading: a "PLEASE STAND BY" slate in the preview window and empty navy rows; Error: the slate carries the message and the crawl repeats it; Empty: a "NO LISTINGS" slate; Ready: the grid
C2  Rows stay the system channels; the time columns were rejected, so the columns become the `section` captions Korri publishes (Continue, This device, zao); a game is the listing at its system and section, title clamped to two lines, cover in the preview window, no cover: the "No picture on file" slate
C3  The sidebar facts line: Last played, Plays, and Time (the total); the yellow PAUSED flag in the cell for a resumable listing
C4  The "Plays on" tags in the sidebar: one tag, A on the listing plays there; two tags (pips in the cell), A moves focus to the tags, left/right picks This device or zao, A plays
C5  `options` on a listing flips the preview window to its own page: actions as rows under the title, dimmed when disabled, description in small print; a destructive one shows an "ARE YOU SURE?" slate first
C6  Up past channel 1 lands on the bar; A opens the keypad strip; each typed letter hides listings whose title does not contain the text and the column header reads Titles containing "zel"
C7  A slate in the preview window: the kicker in big yellow, the detail under it; host actions (Cancel) as tags under the promo title; the game's listing keeps the focus
C8  The preview slate reads the kicker, the listing gets a yellow ON AIR flag, and the crawl says "Now playing Spelunky on zao"
C9  A slate in the preview window: the kicker, the gameTitle, the reason; tags under the promo: Try again (when canRetry) and Dismiss
C10 The last block on channel 1 Settings, "Reload the guide"; also the only tag on the Error slate
C11 No answer yet
C12 One service channel per settings group, numbered from 1, the group title in its yellow channel tab; each item is a block: label in yellow, value in white; the description shows in the sidebar when focused
C13 A on a block: a choice cycles its choices and lists them in the sidebar; text and sensitiveText open the keypad strip (dots, and a Clear tag with clearLabel); an action fires; destructive shows a slate with the title, message and confirm-label tag
C14 Saving: a yellow SAVING flag in the block's corner; Problem: the preview slate carries the message and B dismisses it
C15 A service channel of its own named Device, after Identity: one block per device action, dimmed when disabled; A runs it
C16 Channel 3's "Back up" block: the password on the keypad strip, then the preview window shows the QR code and the sidebar prints the secret in big type, 20 characters a line
C17 Channel 3's "Switch" block, From backup: secret then password on the keypad strip, a TRANSFER / DELETE tag pair, then a yellow slate "Other devices will stop trusting this one" with a Go on tag
C18 The "Switch" block's second tag, By bunker: the URI on the keypad strip, then the same TRANSFER / DELETE tags and trust-loss slate as C17
C19 The "Retired keys" block reads the count; A opens them as a stacked cell, one line per key in short form; A on a key offers Delete, after a "Backed up?" slate
C20 The preview slate: Working with the operation, Problem with the message, Switched with the new owner key in big type; B or a Dismiss tag clears it
C21 The crawl: "connected to hummingbird-lake 72%", "wired" or "offline"; channel 2 Wi-Fi's first block says the same
C22 Channel 2 grows one row per network, strongest first: %, padlock when secured, KNOWN flag; the Scan block reads SCANNING; Connecting and Failed run in the crawl, a rejected password reopens the keypad strip; no adapter: one block, "No Wi-Fi on this device"
C23 A on a network row joins; a secured one not known asks the password on the keypad strip; the "Join a hidden network" block asks the name first, then the password
C24 No answer yet
C25 No answer yet
C26 The clock in the bar, marquee yellow on the left; buildLabel in the sidebar's fine print
C27 Attract after 60 s: the grid crawls up one row every 4 s and wraps, the preview and sidebar cycle the listings on screen 8 s each, the crawl runs steady; any input returns to the focused listing
C28 `options` flips the preview to the listing's own page (C5); `menu` jumps to channel 1; `system` jumps to the Device channel; everything else is the four directions, A and B
C29 Text entry takes over the bar: the clock stays and the crawl becomes a one-line keypad strip; left/right step the row, up/down switch abc/ABC/123/#+=, A types, B deletes; the value shows in the sidebar in big type, dots for a password
C30 The focused cell is marquee yellow with a white rule, always one on screen; the grid scrolls to keep it in view; under reduced motion rows and the crawl step in whole jumps
C31 A slate in the preview window: the kicker in big yellow, the detail under it, the listing named when it has one; the host's actions as tags under the promo title, in the host's order and words; the crawl repeats the kicker
C32 The Busy slate gains a bar of yellow blocks filled to done over total with "1.2 of 3.4 GB" under it; with no total "1.2 GB so far" and no bar; with no progress the slate alone
C33 The sidebar facts line adds Download 3.4 GB (Download alone with no size); the listing's cell carries a DL flag beside the pips
C34 No answer yet
C35 No answer yet
C36 No answer yet
C37 No answer yet
C38 No answer yet
C39 No answer yet
C40 No answer yet
C41 No answer yet
