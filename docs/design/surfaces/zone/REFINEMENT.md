# Zone refinement brief: from "Mania-flavoured" to "a nod a Mania fan smiles at"

PNG names below are from `/tmp/zone-all/` without the `src-pages-`/`src-ui-organisms-` prefix and the hash, so `ZoneGames.Idle.rg353m` is `src-pages-ZoneGames.page.b48c00e45664.Idle.rg353m.png`. Hexes marked *sampled* were read with ImageMagick from the person's reference screenshots (`.forgerie/attachments/attachment-20261019-1215*`). The `.mp4` was not opened. Its timing is not in this brief, so check motion against it before you lock M2.

## 1. Verdict

Zone gets Mania's *objects* right, but its *material* is wrong. The skeleton reads as a nod. These screens are close to Mania's own: the PAUSED red band and yellow wedge (`ZonePaused.default.rg353m`), the save cards with the NEW-style striped header (`ZoneSaveData.default.rg353m`), the 1P/2P/3P controller cards with a WAITING seat (`ZoneOnboarding.FourControllers.*`), the dialog cut by black wedges (`ZoneStatusLayer.Problem.*`), the title card with an ACT disc (`ZoneGames.Idle.*`), and the triangles, offset underline and tag rows on the cards (`ZoneGames.default.*`). A devotee sees in under a second that the material is a web page in costume:

- Every word is Anton, a smooth vector condensed face with a blue drop. Mania's lettering is pixel caps with a hard dark outline and a two-tone fill.
- Every menu stands on one mint ground with *translucent* smooth rings and *translucent* diagonal bands. In `ZoneGames.default.screen-720p` an olive smear shows where the orange band crosses the mint. Mania's menus have opaque pixel rings and no bands, because the bands belong to the title card. Mania also changes the ground per menu.
- OPTIONS and RUN WITH are bare mint with no rings (`ZoneOptions.default.*`, `ZoneRunWith.default.*`).
- Dialogs and the keyboard are royal blue `#2d5be8`. Mania's dialog is sky `#31A2F7` (sampled).
- The keycaps are white discs with pink letters. Mania's are dark discs with coloured letters.
- The GAMES banner is flat yellow, without Mania's white band through the letters and without the red and green zig-zag lip.
- PAUSED stands in for the game with grey hatching that reads as "image missing".

The craft also slips in places:

- Korri's sentences sit on bare ground (`ZoneRunWith.Conflict.rg353m`).
- Words break inside themselves (`ZoneOptions.Narrow.rg353m` "Config/ured", `ZoneRunWith.default.rg353m` "/nix/store/8n1x2-/retroarch").
- Rows are sliced by the tab strip (`ZoneOptions.ActionConfirm.rg353m` "PLAYER SEATS").
- A flat dark-green veil covers the page under status (`ZoneStatusLayer.BusyDetail.*`).
- Loading shows washed-out ghost cards (`ZoneGames.Loading.*`).

## 2. Big decisions

### (a) Type: draw Zone's own outlined pixel caps, plus one libre pixel face for facts

| Option | Verdict | Why |
|---|---|---|
| Keep Anton | Reject | This is the main tell. No CSS outline turns a vector grotesque into pixel lettering. |
| Libre pixel face only (Jersey 15/20, Pixelify, Silkscreen, Micro 5 rendered and compared) | Reject for display | The nearest, Jersey 15 (OFL), has a 15-px cap and 2-px strokes. It is too wide and too light against Mania's ~10-art-px cap with 2-px strokes, which I measured on the 1080p references at 4.5×. Silkscreen and Press-Start-style faces read as "generic retro". |
| **Draw "Zone Caps", our own, plus Jersey 10 for facts** | **Take** | This is the only way to land Mania's weight and proportion without tracing it. It also gives tabular digits, which Anton lacks (see README "Anton has no tabular figures"). |

- **Zone Caps spec.** Uppercase only. Glyph cell 7 art px wide (6 plus a 1-px gap; M and W 8+1; I 2+1; space 4). Cap 10 art px. Strokes 2 px. Diagonals are 1:1 stairs. Round letters (O C G Q S D U 0 6 8 9) get a 1-px 45° notch on each outer corner instead of a rounded corner. That notch is Zone's own signature, from its parallelogram lean, and is not Mania's shape.
  - Coverage: A–Z, 0–9 (all 7 wide, so tabular), `' " : . , - / % ? ! & + ( ) # @ … × ▲`, and Latin-1 and Latin Extended-A capitals. Accents go in 3 px of headroom above the cap.
  - Draw it on a grid. Never trace a screenshot. Source: `src/fonts/zone-caps.grid`, a text grid, built once by a tool script into `ZoneCaps.ttf`. Ship it under OFL beside Anton's OFL.txt, then delete Anton.
- **Facts face.** Jersey 10 (Sarah Cadigan-Fried, OFL, google/fonts `ofl/jersey10`; I checked the license and the latin-ext subset). It has an 8-px cap, 1-px strokes and lowercase. Use it for everything Korri publishes in its own case: titles in verbatim mode, SSIDs, paths, keys, values. Use it also for Korri's sentences (reason, detail, description, message, warnings), now set *as written*. Today they are upper-cased Anton.
  - The split encodes information: Zone's own words are in Zone Caps, Korri's facts are in Jersey 10. Mania does the same, with caps menus and mixed-case names on LEADERBOARDS (reference `…121515-979`).
  - Font size is `14px × --zone-px`. I rendered it crisp at 28 px. Check 42 px at 720p and snap the line box to whole art px.
- **Fallback.** Zone Caps → Jersey 10 → system sans. A CJK title therefore drops out of the pixel register. Accepted gap.
- **Scale.** One art pixel is `--zone-px = max(1, floor(min(W, H) / 240))`, from the container, in the plan: 640×480 → 2 (canvas 320×240), 1280×720 → 3 (426×240, Mania's own canvas), 1920×1080 → 4, 480×800 → 2, 1280×300 → 1. It is integer only. Mania itself scales 4.5× at 1080p; Zone floors to 4 and gives the spare art pixels to layout, which keeps everything crisp.
  - Type sizes are integer multiples of the cap: `--zone-caps-1` (cap 10: rows, tabs, tags, hints, buttons), `--zone-caps-2` (cap 20: banner, dialog headline and buttons, PAUSED rows, focused card title, group headers), `--zone-caps-3` (cap 30: title-card name, PAUSED, tally heading).
  - **Intrinsic rule:** `useZoneFit` steps 3×→2×→1× only, never fractionally. At 1× a word that still does not fit wraps between words. A fact truncates with "…" and shows whole in its row's description line when focused. Nothing becomes unreachable. Only size steps down.
  - The plan's thresholds are restated in art px (`plan(wArt, hArt)`; for example `visible = wArt >= 333 ? 5 : 3`). Re-measure each one: they are guesses until measured.
- **Lettering inks** (`ZoneCaps` variants; a zero-blur `text-shadow` ring 1 art px wide makes the outline, and a hard-stop `background-clip:text` makes the two-tone fill):

| Ink | Face | Lower 3 rows | Outline 1 px | Drop 1 px ↘ | Used for |
|---|---|---|---|---|---|
| `white` | `#F7F3F7` | `#B5BEE7` | `#000000` | `#000000` on grounds; none on black | all labels |
| `hud` | `#F7DB08` | `#F7DB08` | `#3151AD` | `#3151AD` | TIME, PLAYS, LAST PLAYED, FIND, tally labels, game titles in dialogs |
| `banner` | `#F7DB00` | white band rows 4–6 `#E7E3E7` | `#000000` | 2 px `#F78E18` | the one banner |
| `ink` | `#000000` | — | none | none | on white bars (ZONE, NEW, card headers) |
| `danger` | `#F75131` | `#C64100` | `#000000` | none | destructive verbs |

Cost: two to three days to draw and kern about 140 glyphs, and the build tool. Every `useZoneFit` call has to move to integer steps. The contrast gate must judge outlined text as face against outline (see §5). CJK falls back to a non-pixel face.

### (b) Pixel register: yes, the whole surface stands on the art-pixel grid

- **Must.** `ZoneGround` draws to a `<canvas>` of `wArt × hArt`, scaled by CSS with `image-rendering: pixelated`. The rings become stair-stepped pixel circles, 6 art px thick, opaque, each with 1–2 filled "planet" dots. Drop the two diagonal bands and the opacities (`--zone-ring-*-opacity`, `--zone-band-*-opacity`): bands belong only to the title card. The drift is a `render(t)` clock at 15 fps with the dots snapped to art px, and it is seekable.
- **Must.** Every length token (slant, drop edge, padding, gaps, borders, the card's 4-px frame) is a whole number of `--zone-px`. The drop edge is 2 art px, 3 when focused. The underline is 2 art px.
- **Should.** Panels lean at exactly 45° (slant = panel height), as all of Mania's do. Their edges are stairs, from one pure helper, `zoneStairClip(wArt, hArt)`, that returns the `clip-path` polygon with one step per art px. The dialog wedges and the PAUSED wedge use it too.
- **Should.** Every translate in motion is rounded to art px (§2d).
- **Cost.** The canvas ground needs an allowance in the authoring gate (it is drawing, not state), and the clip polygons get long. On a 1080p TV the stairs are 4 CSS px. That is the point.

### (c) Palette per screen group (every hex sampled unless marked †)

| Group (Mania screen it nods to) | Ground | Ring A | Ring B | Drop/shade † | Group extras |
|---|---|---|---|---|---|
| GAMES, FIND, first boot (TIME ATTACK / COMPETITION) | `#4AD39C` | `#39B2CE` | `#F78E18` | `#2E9E72` | card stripe `#9CE7C6` `#5A71E7` `#5AA2B5`; tags system `#5A71E7`, HERE `#F7DB08`, CONTINUE `#000000` |
| OPTIONS, RUN WITH (OPTIONS) | `#EFC300` | `#84A2B5` | `#F78E18` | `#B58F00` | full-width row band `#31A2F7` with a black wedge at its left |
| SAVE DATA (MANIA MODE save select) | `#31AAC6` | `#F57128` | `#4AB8AC` | `#23809A` | card lower window `#9CE7C6`, stripe `#5AA2B5` † |
| GAME OPTS. layer (MANIA MODE's GAME OPTS., formerly SECRETS) | `#33ACC7` | `#F57128` | `#4AB8AC` | `#23809A` | — |
| PAUSED | band `#EF285A`, wedge `#F7DB08` | — | — | — | stand-in in greys `#5A5A5A`/`#8C8C8C` † |
| Title card, idle attract | `#F7CB00` | — | — | — | bands `#4261B5` `#F78E18` `#F75131` `#63C3A5`, act disc `#F78E18`, ZONE bar `#F7F3F7` |
| Results tally (SPECIAL STAGE results) | `#EFC300` | `#84A2B5` | `#F78E18` | — | bar `#C64100` over a black 1-px underside |
| Shared everywhere | panel `#000000` | dialog `#31A2F7` | banner `#0020B5` | focus pink `#CE6584` | cursor red `#E70000`; banner lip red `#E70000` and green `#18A800` †; keycap disc `#292029`, B `#EF285A`, A `#639F3D`; veil `#000000` dither |

This retires the "eight colours" rule in the README and DIRECTION. Each group is a token set (`--zone-ground`, `--zone-ring-a`, `--zone-ring-b`, `--zone-shade`) that `ZoneSurface` switches with a `data-ground` attribute on the shell. Cost: about 30 tokens, and every pair has to be added to the contrast gate.

### (d) Motion (a render(t) clock, seekable via `window.seek`, translates rounded to art px; `prefers-reduced-motion` shows end states)

**M1 card snap and focus travel**

| Element | Kind | Trigger | Property | From → to | Start | Duration | Easing |
|---|---|---|---|---|---|---|---|
| band track | focus | left/right | translateX | old → new centre (art px) | 0 | 220 ms (was 340, over the 250 ms focus budget) | `cubic-bezier(.34,1.56,.64,1)` |
| focused / old card | focus | left/right | scale | 0.68 ↔ 1 | 0 | 220 ms | same |
| red triangles | idle | always on focused card | translateX | 0 → 2 art px toward card | — | 480 ms loop | `steps(2)` |
| pink underline | focus | focus moves | translate | (0,0) → (2,2) art px | 0 | 60 ms | `steps(1)` |

**M2 menu transition (tab change, page change)**

| Element | Kind | Trigger | Property | From → to | Start | Duration | Easing |
|---|---|---|---|---|---|---|---|
| outgoing panels | structural | tab chosen | translateX | 0 → −wArt | 0 | 160 ms | `cubic-bezier(.5,0,.9,.4)` |
| new ground colour | structural | same | 45° stair wipe from top-right | 0% → 100% | 60 | 280 ms | `cubic-bezier(.2,.8,.2,1)` |
| banner | structural | same | translateY | −8 art px → 0 | 160 | 200 ms | `cubic-bezier(.34,1.56,.64,1)` |
| incoming rows / cards | structural | same | translateX | +wArt → 0, 20 ms stagger, 6 at most | 160 | 240 ms | `cubic-bezier(.34,1.3,.64,1)` |

The total stays under 500 ms. Focus lands at 160 ms and is never off screen.

**M3 title-card wipe (launch, and idle in)**

| Element | Kind | Trigger | Property | From → to | Start | Duration | Easing |
|---|---|---|---|---|---|---|---|
| 4 bands | structural | PLAY/CONTINUE pressed | translate | (+wArt, −hArt) → rest, 40 ms stagger | 0 | 320 ms | `cubic-bezier(.2,.9,.3,1)` |
| name panel | structural | same | translateX | −wArt → 0 | 200 | 260 ms | `cubic-bezier(.34,1.4,.64,1)` |
| ZONE bar | structural | same | translateX | +wArt → 0 | 260 | 260 ms | same |
| act disc | structural | same | scale | 0 → 1 | 420 | 160 ms | `steps(4)` |
| everything | structural | status leaves Busy for this game | reverse; bands exit toward bottom-left | rest → off | 0 | 400 ms | `cubic-bezier(.5,0,.9,.4)` |

**M4 dialog.** The sky panel scales scaleY 0→1 from its centre in 180 ms, `steps(6)`, while the black wedges slide in from ±wArt over the same 180 ms. Close reverses in 140 ms.

**M5 tally.** Rows enter with 300 ms stagger. Each value counts 0 → value in 1200 ms, `steps(60)`, and its bar width grows 0 → share in step with it. Any input jumps to the end state. The tally exits at the next input, or after 8 s, in 300 ms.

**M6 idle attract loop, 6000 ms per game, seamless.** M3 in (0–700 ms), hold (to 2500), bands out (to 2900), the "stage" pans translateX 0 → −32 art px, linear, to 5600, then the next game's M3. PRESS ANY BUTTON blinks 1000 ms `steps(2)`.

**M7 ground.** Ring dots orbit at one turn per 140 s and 200 s, linear, at 15 fps.

Deliver a take per the motion-mockup skill at 640×480 and 1280×720 for M1–M6, plus reduced motion.

## 3. Fan moments (each mapped to a Korri fact)

| Nod | Korri fact | Where | Why a fan reads it as a nod, not a copy | Call |
|---|---|---|---|---|
| **Launch title card**: game title on the black panel, ZONE on the white bar, ACT disc = **playCount + 1** (1 when absent) | `title`, `playCount`, status `Busy.gameId` | over GAMES from PLAY until status leaves Busy for that game; the busy strip stays on top of it | the composition is Mania's, but the name is the player's game, "ZONE" is the surface's own name, and the act is *your* next session | **Must** |
| **Results tally** after a session | the same game's `playCount` rose by exactly 1 between Running and Browsing; TIME = Δ`totalPlaytimeSeconds`, shown as `72'14"`; PLAYS = new count; TOTAL = new total as `3H 12M` | a layer over GAMES, SPECIAL STAGE yellow, red bars count up | the tally count-up is the most recognised beat of the classic games; the numbers are Korri's | **Should** (no tally when the delta is ambiguous) |
| **Facts line as the HUD**: `hud` ink labels, white values; absent values as TIME ATTACK's empty record `--'--"--` (TIME) and `--` (others) | `lastPlayedAt`, `playCount`, `totalPlaytimeSeconds` | GAMES facts line | HUD lettering, and dashes for "no record" as on TIME ATTACK's table; it states absence instead of hiding it | **Must** (lettering), **should** (dashes) |
| Foot TIME as the HUD timer | `clockLabel`, verbatim | foot | it already is; it only needs `hud` ink | **Must** |
| RINGS counter for plays | — | — | it would relabel a fact with a word that means something else; it is also a Sonic word, so it tips into parody | **Reject** |
| Lives-style counter `PLAYS ×07` | `playCount` | facts line | the save card's `×03` | **Could** |
| **Idle attract**: title card → the game's wide art as the "stage", panning, with the HUD → next game; PRESS ANY BUTTON blinking | `title`, `coverArtUrl`, `clockLabel`, `playCount` | idle layer (C27) | the title-screen attract loop; the words are true, because any input wakes it | **Must** |
| Static noise in an empty window | catalog `Loading` | the wells of the Loading cards | MANIA MODE's used save window shows noise before its art | **Should** |
| Save select NEW card and window | `localBackupAvailable`, `retiredPublicKeys`, status | SAVE DATA cards | white header bar with stripes over a light lower window, as MANIA MODE's cards; the art is our own pixel emblems | **Should** |
| Blue Sphere-style checker, earned | `BackupReady` (a backup has just been made) | ground behind the QR well only, blue `#2E4BD6` † / white `#F7F3F7` †, 8-art-px squares | it appears only when you have earned something; it never sits behind the QR's own quiet zone | **Could** |
| Greyscale game behind PAUSED | status `Running.gameId` → that game's `coverArtUrl`, `filter: grayscale(1)`; else a greyscale checker | PAUSED stand-in | Mania greys the live game; the checker echoes the reference's greyed loop ground | **Must** |
| COMPETITION VS | `onboarding.controllers` with exactly 2 heard | first boot, between seats 1 and 2 | Mania's 1P-VS-2P layout; with 3 or 4 seats none, as Mania's 4-up | **Should** |
| Banner with white band and red/green lip; dark keycaps with a trailing slant | — (chrome) | top strip on every tab | the most-seen chrome in every Mania menu; drawn ourselves | **Must** |
| Lettering overhangs its panel's top edge | — | PAUSED rows, title-card name | Mania's CONTINUE/RESTART and GREEN HILL rise above their slabs | **Should** |
| Mega Drive dither veil (a 1-art-px checker of black) | status Busy, Problem, Confirm | over the inert page | era-true transparency; replaces the flat green veil | **Should** |
| SEGA-style boot / jingle | no fact; no sound in the treaty | — | trade-dress risk, and it delays every mount | **Reject** |
| Chaos emeralds, a character, a ring icon, the "GOT THROUGH" line | — | — | Sonic-specific marks, or parody | **Reject** |

## 4. Screen by screen (priority order; M = must, S = should, C = could)

**All screens, first:** M Zone Caps + Jersey 10 + inks (§2a). M `--zone-px` grid and canvas ground (§2b). M `data-ground` palettes (§2c). M keycaps: `ZoneKeycap` disc `#292029` with a 1-px `#000000` rim, B letter `#EF285A`, A letter `#639F3D`, word `white` ink caps-1, black 45° tail bar 3 art px longer than the word. M banner: `ZoneBanner` hex `#0020B5`, `banner` ink at caps-2, a 2-art-px lip under it, `#E70000` on the left half and `#18A800` on the right, with four 4-art-px notches at its centre (two red, two green). M no word ever breaks inside itself; a fact truncates with "…" and is whole on focus.

**GAMES** (`ZoneGames.*`)
1. M Ground mint group; no bands; opaque pixel rings.
2. M Facts line in `hud` labels and white values on a black 45° strip. Absent values show the dashes.
3. S Card (`ZoneCard`): 4-art-px black frame; stripe band `#9CE7C6`/`#5A71E7`/`#5AA2B5`; tag row in TIME ATTACK colours. The drop is `--zone-shade`, 2 art px; it replaces the blue `--zone-drop-edge`. The title on the focused card is caps-2, stepping to caps-1 to fit; side cards caps-1.
4. S Loading: three cards whose wells show 2-colour noise (`#000000`/`#F7F3F7`, regenerated every 100 ms; frozen under reduced motion). This replaces the ghost cards. Empty: one card whose header bar reads NO GAMES over a noise well.
5. S Error: the sky dialog (Problem's look) with GAMES as the headline and RELOAD.
6. S Section spines: black, with the caption in `white` caps-1 running up the spine; the hairline spine at 640 px becomes a 4-art-px stripe of the card colours.
7. C `PLAYS ×07`.

**GAME OPTS.** (`ZoneGames.GameOpts.*`)
1. M An opaque sheet, not rows over the visible band. It is a full-width `#33ACC7` panel under the tabs, slid in by M2, with its own rings. The band behind it is fully covered.
2. M Header: black 45° panel with GAME OPTS. in caps-2; under it the game title in Jersey 10 verbatim on a `#5A71E7` tag.
3. S Rows as Mania's SECRETS rows: label panel left, value panel right (RUN). Focus is the pink drop plus red triangles flanking the value. Disabled: `#84A2B5` face, with the reason line always shown in Jersey 10. Destructive RUN in `danger`.

**FIND keyboard** (`ZoneGames.FindOpen.*`)
1. M Panel `#31A2F7` with a black left wedge (the OPTIONS band look), replacing royal blue.
2. M Keys are black 45° caps. Letter keys show their real glyph: lowercase layer in Jersey 10, capitals and symbols in Zone Caps. Space, DELETE and DONE are caps-1. The lit key gets the pink drop.
3. M Slab `#F7F3F7`: the draft in Jersey 10 black, red caret `▲` blinking 1000 ms `steps(2)`, and the count (`4/32`) in `hud` ink outside the slab.
4. S The kept filter tag reads `FIND` in `hud` ink plus the draft in Jersey 10, on black.

**Idle attract** (`ZoneGames.Idle.*`)
1. M Title card per M3/M6 in the title-card palette. The name is caps-3 and overhangs its panel by 3 art px. ZONE is `ink` caps-2 on the white bar. The ACT disc is 52 art px across, holding ACT on a black tab (caps-1) and the number in caps-3; 3 digits step to caps-2. This replaces `ACT PC`. The system moves to a `#5A71E7` tag under the ZONE bar.
2. M Stage phase: the art letterboxed full-bleed on black, with the HUD at the top left (TIME = clock, PLAYS) and PRESS ANY BUTTON as `white` caps-1, bottom centre. With no art, the card's stripe well pattern fills the screen.

**OPTIONS and Wi-Fi** (`ZoneOptions.*`)
1. M OPTIONS ground; rows stand on the full-width `#31A2F7` band, which is the scroll region. The band has a 3-art-px black lip top and bottom, so a cut row is cut *by the band*, never by the tabs (fixes `ActionConfirm`, `Choice`).
2. M Group headers are black panels flush left at caps-2 (Mania's VIDEO). Labels are caps-1. Values are Jersey 10 verbatim, centred in the value panel.
3. M Narrow (240 art px wide or less): the row stacks label over value (reflow), never breaking letters (fixes `Narrow`).
4. M Wi-Fi words become one set, shared with first boot: `LOCKED`, `OPEN`, `SAVED` (replacing `LOCK`, `KNOWN`). Bars (`ZoneBars`) are 4 pixel bars, 2/4/6/8 art px tall, 2 wide, 1 apart; lit `#F7F3F7`, unlit `#3A313A`; the strength stays in Jersey 10.
5. S Saving: the value panel's caret blinks; the value stays.

**RUN WITH** (`ZoneRunWith.*`)
1. M OPTIONS ground and band.
2. M Korri's message sits on a black 45° panel in Jersey 10 as written. Warnings sit on black panels with a 2-art-px `#F75131` left bar, and the text is `#F7F3F7`, no longer red on mint (fixes `Conflict`, `Warnings`).
3. M The build path shows its tail: `…retroarch-1.19.1`, cut in the middle. The whole path is in the description line when its route is focused.
4. S Header: the banner reads RUN WITH; the game title is on a black panel caps-2; the state tag uses the existing colours, with READY moved to `#5A71E7`.

**SAVE DATA and backup QR** (`ZoneSaveData.*`)
1. M SAVE DATA ground (teal group).
2. S Card lower window `#9CE7C6`, with a `#5AA2B5` diagonal and a 16×16 Zone-drawn emblem centred: key (BACK UP), arrow-into-card (FROM BACKUP), link (BUNKER), grey key (RETIRED). Under the emblem, the window's word in caps-1 on a black tag; keys shortened in Jersey 10. The header bar keeps the label in `ink` caps-2.
3. S The status tag at the foot (NEW orange, BACKED UP mint) becomes a full-width bar, as Mania's foot strip.
4. M BackupReady: the QR well stays white with its own quiet zone. The slab is `#F7F3F7` with Jersey 10 groups of four. Dialogs are sky.
5. C The checker behind the QR well (§3).

**PAUSED** (`ZonePaused.*`)
1. M Band `#EF285A`, wedge `#F7DB08`; PAUSED (or Korri's title) in caps-3.
2. M Stand-in: the greyscale running game's art, else the greyscale checker, 16 art px squares (fixes the hatching).
3. S Rows step down-left along the wedge's 45° edge, each one cap-height further left. The lettering overhangs its slab by 4 art px. Values and ranges on the right of each slab; the range bar is `#C64100` on `#3A313A`.
4. S Group labels are `#5A71E7` tags (they exist), in caps-1.

**First boot** (`ZoneOnboarding.*`)
1. M Mint ground. The banner names the step (CONTROLLERS, WI-FI, NAME, IDENTITY). Under its lip, four diamond pips 5 art px wide: filled `#F7DB08` for the current step and the steps done, outlined black for the rest. This replaces the white/yellow/blue step strip.
2. S Controller cards: frame `#E70000` when heard, black when WAITING. The seat tag is a black corner tab with `1P`–`4P` in caps-1 (the per-seat colours go). VS appears when exactly two are heard.
3. S IDENTITY: the save card's header reads NEW, its window START, as on SAVE DATA.

**Busy with progress** (`ZoneStatusLayer.Busy*`, `ZoneGames.BusyOverHome`)
1. M The veil is the 1-art-px black dither over the inert page.
2. M Strip: black 45° panel. The kicker is caps-2, the detail Jersey 10, the game title `hud` caps-1 on its black tab. During a launch it stands at the bottom of the title card (§3).
3. S Meter: fill `#C64100` with a 1-px `#F75131` top lip, track `#3A313A`, 45° ends, 4 art px tall. Figures in `white` caps-1, tabular, at the bar's right end. No end: the sweep stays, in `#C64100` on `#3A313A`.

**Problem and Confirm** (`ZoneStatusLayer.Problem*`, `Confirm*`)
1. M Panel `#31A2F7`, with black wedges left-top and right-bottom from `zoneStairClip`. Headline caps-2 `white`. The reason or detail is Jersey 10 `#F7F3F7` with a 1-px black outline, at 52 characters or fewer per line. The game is `hud` caps-1.
2. M Buttons are black 45° panels in caps-2; the lit one gets the pink drop. Host order and words are unchanged; destructive in `danger`.

**Running** (`ZoneStatusLayer.Running.*`)
1. S A HUD stack at the top left, as the in-game HUD: line 1 the kicker in `hud` caps-1 (PLAYING ON ZAO); line 2 the title in `white` caps-1 on black; OPEN as an orange tag. The page stays usable.

## 5. What not to do

- No Sega or Sonic marks, names, characters, rings-as-icons, emeralds, or the "GOT THROUGH" line. No ripped sprites, fonts or art, and no tracing a screenshot, glyph or banner. Hexes and proportions are the nod; the drawings are ours.
- No SEGA-style boot and no faux jingle; the treaty has no sound. Never relabel a Korri fact with a Sonic word (RINGS, SCORE, LIVES).
- Bands belong to the title card. Do not put them back on menu grounds. Do not run the attract or the checker anywhere unearned. One loud moment per screen.
- Do not give each screen its own focus language (Mania mixes red, white and pink underlines). Pink drop plus red triangles stays the only focus (C30).
- No mixels: never scale Zone Caps by a non-integer, never set a fractional `--zone-px`, never mix art-px grids inside one screen.
- No alpha veils, blur, soft shadows or smooth gradients. Dither and hard stops only. Keep the extended authoring gate.
- Never invent a fact to make a nod work. No act number for a game Korri does not count, no tally when the delta is ambiguous, no SAVE-style emerald count.
- Do not shrink controls to fit: yield type 3×→2×→1×, then wrap or truncate. Layout is `plan(wArt, hArt)`.
- Gates to update in the same change (suggested issues): contrast gate judges outlined text face-against-outline and adds the new pairs; authoring gate allows hard-stop conic dither and the canvas ground; README and DIRECTION drop "eight colours" and "one face: Anton".

## 6. Token plan, wireframes, stress set, copy

**Tokens (new or changed):** `--zone-px` (from plan); `--zone-caps-1/2/3` (10/20/30 art-px caps, font-size = em × `--zone-px`); `--zone-fact` (`14px × --zone-px`); `--zone-ground`, `--zone-ring-a`, `--zone-ring-b`, `--zone-shade` (per `data-ground`); `--zone-dialog #31A2F7`; `--zone-banner #0020B5`; `--zone-pink #CE6584`; `--zone-cursor #E70000`; `--zone-hud #F7DB08`; `--zone-hud-edge #3151AD`; `--zone-white #F7F3F7`; `--zone-white-shade #B5BEE7`; `--zone-danger #F75131`; `--zone-meter #C64100`; `--zone-track #3A313A`; `--zone-key-disc #292029`; `--zone-drop 2` and `--zone-drop-focus 3` (art px); `--zone-snap-time 220ms`.

```
640x480  (--zone-px 2, canvas 320x240)        1280x720 (--zone-px 3, canvas 426x240)
+------------------------------------------+  +---------------------------------------------------------+
|(B)BACK\  [=GAMES=]/OPTIONS/SAVE DATA (A)\|  |(B)BACK\        [==GAMES==] /OPTIONS/ /SAVE DATA/  (A)\  |
|          ^^lip^^               [FIND]    |  |                 ^^lip^^                      [FIND]     |
|  .-ring--.                               |  |   .-ring----------.                                     |
| [side]  >[ FOCUSED CARD ]<  [side]       |  | [s][side] >[  FOCUSED CARD  ]<  [side][s]              |
|                                          |  |                                                         |
| /LAST PLAYED 10 OCT 2026 PLAYS 7 TIME 3H 12M/ (hud)  same, one line                                    |
|        /PLAY/          /OPTS./           |  |              /PLAY/            /OPTS./                  |
|/TIME 21:47/      /ssid ▂▄▆█ 72%/ build   |  |/TIME 21:47/ /MINI/              /ssid ▂▄▆█ 72%/ build   |
+------------------------------------------+  +---------------------------------------------------------+
Title card (both sizes): bands enter from the top right; [HOLLOW KNIGHT] panel at 45% height, [ZONE] bar under it, right-aligned; (ACT 8) disc to the right of the bar; busy strip along the bottom.
```

**Stress set.**

| Case | What happens |
|---|---|
| 0 games | the NO GAMES noise card |
| 1 game | the band is centred and both end triangles are dim |
| 40 games | the band keeps 3 or 5 cards and the edge triangles |
| Longest title (THE LEGEND OF ZELDA: ORACLE OF SEASONS) | caps-2 → caps-1 → wraps between words; it is never cut on the focused card |
| One unbroken word (supercalifragilistic… in `ZoneStatusLayer.ProblemLongReason`) | Jersey 10 breaks it *only* at the line end with a hyphen; this is the single allowed break, for a Korri sentence |
| CJK title | system-font fallback |
| Missing facts | the dashes |
| Running with no `gameId` | the PAUSED checker |
| 480×800 (`--zone-px` 2) | rows stack label over value |
| 1280×300 (`--zone-px` 1) | caps-1 is 10 CSS px; the facts join the action strip as today; the title card uses caps-2 |
| Reduced motion | end states, noise frozen, no blink |

**Copy (new strings only, all Zone's own).** `ZONE` (title-card bar) · `ACT` (disc tab) · `PRESS ANY BUTTON` (attract) · `TIME`, `PLAYS`, `TOTAL` (tally rows) · `--'--"--` and `--` (absent facts) · `VS` (first boot) · `NEW` (first-boot save header, was NEW SAVE) · `LOCKED`, `OPEN`, `SAVED` (Wi-Fi, everywhere) · `×` (could: plays). Every other string is unchanged.

**The one thing that must not be lost:** every letter Zone draws is outlined pixel caps on one integer art-pixel grid. If only one change ships, ship that one: it is what turns the costume into the nod.

**Sources.**
- The person's 15 reference images (all palette samples).
- sonic.fandom.com MediaWiki API, `Sonic_Mania` wikitext, for: Plus renamed SECRETS to Game Options in Mania Mode; the Plus main menu lists modes on the right; the Time Attack timer shows centiseconds.
- google/fonts `ofl/jersey10/METADATA.pb` for the license and subsets.
- Mobbin was not used: the reference for this register is the game, not a shipped app pattern.
- The title-card and menu motion timings are my opinion, not measured. Check them against the attached `.mp4`.
