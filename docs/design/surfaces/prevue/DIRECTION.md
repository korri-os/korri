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
