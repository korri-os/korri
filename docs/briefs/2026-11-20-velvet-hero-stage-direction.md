# Velvet: the whole hero on the stage (direction brief)

Owner decision (2026-11-20): SteamGridDB heroes (1920 x 620, about 3.1 to 1)
are the stage's standard art, and heavy cropping is not acceptable. This
brief proposes three stage compositions, recommends one, and states its cost.
It changes no code.

Mockups: `~/code/sandbox/korri-mockups/hero-stage/` (outside the repository:
its `art/` is third-party art that may not be committed; `CREDITS.md` there).
`plan.js` there is the layout as one pure function, a port of the pick's part
of `src/plan.ts` plus the three compositions. The review deck is
`deck.json` in that folder.

## Recommendation

**A, the Band.** The hero at the window's full width from the top edge, in
its own shape, never cut. The rest of the stage is the stage light that
Velvet already draws (the art's colour, #422). Where the room under the
picture holds the words, they stand there; where it does not, the logo stands
over the picture's lower left and Play makes a bar under the picture with the
facts beside it.

Cost, in short: about 43 % of a 16:9 window is light, not picture, so the
pick is less immersive than today's full bleed; the Stage pane alone and tall
windows show a quiet field of light under the words; and the work touches
`src/plan.ts`, `src/art.ts`, `VelvetStage`, `VelvetPick` and their tests
(see "Cost" below).

## The problem

The pick and the detail cut the hero to fill the stage
(`VelvetStage.slint`: `fit = max(W / w, H / h)`; `src/view.ts` sends
`fill` true for `home` and `detail`). The share of the hero's width that
shows is `min(1, (W / H) / 3.1)`:

| Window | Shown today | Cut today |
| --- | --- | --- |
| 1920 x 1080 (TV, Odin 2 Portal, Thor top) | 57 % | 43 % |
| 1280 x 720 | 57 % | 43 % |
| 1240 x 1080 (RP Mini V2, Thor bottom) | 37 % | 63 % |
| 640 x 480 (RG353M) | 43 % | 57 % |
| 900 x 412 (phone, landscape) | 71 % | 29 % |
| 412 x 900 (phone, portrait) | 15 % | 85 % |

Verified: the 1920 x 1080 figure in Velvet's own frames (`VELVET_FRAMES`,
`cargo test --features fixture --test velvet_package frames`, scenario
`home`): the test hero `hero-ember.png` (640 x 206) shows as a zoomed sun.
The other rows follow from the formula.

Real heroes make the cut worse than the number says. Of 12 heroes in the
mockup set (5 from SteamGridDB, 7 from the Steam CDN in the same 1920 x 620
format), Chrono Trigger and Sonic Mania put their subjects in the right
third: a centred 57 % cut loses Crono and half of Knuckles. A 16:9 cut also
keeps a 30 % dimmer over the whole picture (`velvet-30`).

## The three compositions

All three keep every capability on screen at every size (Play, the facts,
the Menu pill, the clock, the row). All three are pure functions of the
window's width and height, plus one fact of the window's content: whether
the Floor's strip shares it (ADR 0086: one window, or the Stage pane alone).

| | A Band | B Bleed | C Beside |
| --- | --- | --- | --- |
| The picture | Full width from the top edge, its own shape | Full width; its own edge rows (and columns) stretched to the window's edges under a veil | Its own shape, about 56 % of the width, at the right, the words in a column at the left |
| Hero shown | 100 % at every size | 100 % at every size | 100 % at every size |
| Hero share of a 1920 x 1080 window | 1920 x 620 (57 % of the height) | 1920 x 620 plus the bleed | 1075 x 347 (32 % of the height) |
| Under the picture | The stage light (#422) | The picture's foot, stretched to the window's foot | The stage light, on all sides |
| Words | Under the picture when the room holds them; else logo over its lower left, Play bar under it | Logo, facts, Play in the lower bleed, over the picture's foot when room is short | A column beside the picture; falls back to A under 1.45 width / room height |
| Real-hero risks seen | Pale heroes (Chrono Trigger) light the top edge white; a dark logo on the dark house | A shape on the picture's foot row becomes a vertical pole or stripe (10 of 10 heroes in the deck); pale heroes make a grey plane; on a phone the streaks fill the top third | The picture reads as a thumbnail; 1240 x 1080 and 640 x 480 show it at 21 % and 24 % of the height |
| New pixels | None (same hero) | None (Slint `source-clip-*` on the same image) | None |
| Distance from today's code | Small: `fill` false already draws a band; the new parts are the plan and the logo's place | Medium: four stretched slices per hero image, two per crossfade layer | Medium: a new layout and its fallback |

Verdict on each (opinion, from the renders):

- **A** holds every real hero I tried, at every size, and the layout does not
  move when the focus moves: every hero has one shape, so the logo and Play
  stand still while the picture changes. That is the fluid part.
- **B** is the most filled stage on the TV and on the Thor's top display,
  and where the foot row is calm it reads as a velvet curtain. But every one
  of the 10 heroes in the deck has its subject or a strong shape on its foot
  row, and in the bleed renders (1920 x 1080, both windows) each of them
  shows at least one distinct pole or bright stripe: Zelda's red pole, Super
  Metroid's blue column, Pokémon Emerald's yellow line, Hollow Knight's white
  pole, Celeste's red hair, Chrono Trigger's grey plane, a blue water drop in
  Wario Land 4 (the faintest). A flourish that depends on luck in the art
  does not earn its place.
- **C** shows the whole picture and gives the words their own column, but it
  shrinks the art to a card. It is the least cinematic and loses most on the
  squarish handhelds. Its structure survives inside A, as A's rule for a
  wide-short window (the band narrows and keeps the right edge).

## Direction (A, the Band)

The stage is a cinema with the screen at the top of the house: the game's
picture runs edge to edge in its own shape, its foot sinks into the dark,
and below it the house is lit by the picture's own colour. The logo stands
where the picture lets it: over the picture's lower left when the window is
short, under the picture when the room holds it. Play and the facts make one
line under the picture, as Steam's library page does with the same 3.1 to 1
art. Nothing is cut; nothing moves when the game changes, only the picture
and the light cross-fade. The one bold thing is still the stage light.

## Token plan

No new colour, type or motion token. The palette, faces and motion table in
`DIRECTION.md` stand.

| Kind | Name | Value | Note |
| --- | --- | --- | --- |
| Colour | House down, House light, Usher, Ink, Exit, Velvet, the light | unchanged | `ui/tokens.slint` |
| Type | Big Shoulders ExtraBold, Archivo semi-condensed | unchanged | |
| Plan | `HERO_RATIO` | 3.1 | exists |
| Plan (new) | `BAND_SHARE` | 0.8 | the most of the room's height the band may take before it narrows |
| Plan (new) | `UNDER_LOGO` | 0.55 | the words stand under the band when a logo there is at least this share of `markH` |
| Stage | band foot | the last 22 % of the band falls to Velvet 90 %, then the light rises back over 60 % of the band's height | today the band's lower 60 % falls to House down |
| Stage | dimmer | none over the band | today `velvet-30` over the whole picture |
| Stage | shade behind a logo over the band | a radial pool of Velvet 60 % at the logo's foot | not over a pale picture under a dark logo (Korri's `color` for each) |
| Motion | `drift-scale` | 0.03, was 0.10 | the rest drift pushes in at most 3 %: a short cut at rest, gone on any press |
| Motion | `t-backdrop`, `t-light` | unchanged | cross-fade, no dark dip |

## The layout as a function of size

`roomBottom` is the strip's top less a gap in one window, else the window's
foot less `padY`. `rest` is the words under a logo: gap, a fact line, gap,
the buttons, and in the detail the foot's facts.

1. Band height `h = W / 3.1`.
2. `under` when the window is narrow (`W / H < 0.9`), or when
   `min(markH, roomBottom - h - 2.2 gap - rest) >= 0.55 markH`.
3. If not `under`, the bar (gap and the buttons) must stand under the band:
   `h <= min(0.8 roomBottom, roomBottom - bar - 0.3 padY)`. When it is not,
   the band yields height and width together, keeps the right edge, and its
   left edge falls into the dark (8 % of its width), where the logo stands.
4. The buttons stand side by side when they fit the width, else one under
   the other. A button never yields; the band yields first.
5. Yield order: the gaps, then the logo (its box), then the facts on the bar
   (last first, as `factLines` does today; the detail says every fact), then
   the band's size. Never the buttons, the Menu pill or the row.

The plan's numbers at the real windows (from `plan.js`):

| Window | Band | Words | Logo box | Bar or words at |
| --- | --- | --- | --- | --- |
| 1920 x 1080, strip | 1920 x 620 | logo over | 211 | y 650 |
| 1920 x 1080, Stage pane alone | 1920 x 620 | under | 159 | y 656 |
| 1280 x 720, strip | 1280 x 413 | logo over | 141 | y 436 |
| 1240 x 1080, strip | 1240 x 400 | under | 219 | y 430 |
| 640 x 480, strip | 640 x 207 | logo over | 70 | y 224 |
| 412 x 900, strip | 412 x 133 | under | 90 | y 153 |
| 900 x 412, strip | 731 x 236, right edge | logo over | 80 | y 254 |
| 320 x 240, strip | 318 x 103 | logo over | 36 | y 119 |
| 320 x 240, detail | 183 x 59, right edge | logo over | 36 | buttons one under the other |

## Wireframes

1920 x 1080, one window (TV, Odin 2 Portal):

```
+--------------------------------------------------------------+
| HERO 1920 x 620, whole                    4:19 AM  [ Menu ]  |
|                                                              |
|                                                              |
|  [LOGO over the lower left]                                  |
|__________________ foot falls into the dark __________________|
|  [> Play]  GBA  Played 2 days ago  14 h 20 min  This device  |
|                                                              |
|  the stage light (art colour)                                |
|  [tile][tile][tile][tile][All games]                         |
+--------------------------------------------------------------+
```

1920 x 1080, the Stage pane alone (Thor top display; the Floor is below):

```
+--------------------------------------------------------------+
| HERO 1920 x 620, whole                    4:19 AM  [ Menu ]  |
|                                                              |
|______________________________________________________________|
|  [LOGO, 159 high]                                            |
|  GBA  Played 2 days ago  14 h 20 min  This device            |
|  [> Play]                                                    |
|  the stage light                                             |
+--------------------------------------------------------------+
```

1240 x 1080 (RP Mini V2, one window):

```
+------------------------------------------+
| HERO 1240 x 400        4:19 AM [ Menu ]  |
|__________________________________________|
|  [LOGO, 219 high]                        |
|  GBA  Played 2 days ago  14 h 20 min     |
|  This device                             |
|  [> Play]                                |
|  [tile][tile][tile][tile][All games]     |
+------------------------------------------+
```

640 x 480 (RG353M) and 320 x 240:

```
+--------------------------------+   +----------------+
| HERO 640 x 207  4:19 [Menu]    |   | HERO 318 x 103 |
| [LOGO]                         |   | [LOGO]   [Menu]|
|________________________________|   |________________|
| [> Play] GBA  Played 2 days... |   | [> Play] GBA ..|
| [t][t][t][t][All]              |   | [t][t][t][t][A]|
+--------------------------------+   +----------------+
```

412 x 900 (phone, portrait) and 900 x 412 (phone, landscape):

```
+------------------+   +----------------------------------------+
| HERO 412 x 133   |   |dark | HERO 731 x 236, right edge [Menu]|
|__________________|   |[LOGO over the dark and the fade]        |
| [LOGO]           |   |_____|__________________________________|
| GBA Played 2 ... |   | [> Play] GBA Played 2 days ago 14 h... |
| This device      |   | [t][t][t][t][All]                      |
| [> Play]         |   +----------------------------------------+
|                  |
|  the light       |
| [t][t][All]      |
+------------------+
```

The detail, 1920 x 1080 (Confirm on a game; no strip):

```
+--------------------------------------------------------------+
| HERO 1920 x 620, whole                    4:19 AM  [ Menu ]  |
|______________________________________________________________|
|  [LOGO, 165 high]                                            |
|  [> Play on This device] [Play on aka]  ... More             |
|                                                              |
|  GBA  Played 2 days ago  14 h 20 min  This device            |
+--------------------------------------------------------------+
```

## Stress set

| Case | What the Band does | Deck toggle |
| --- | --- | --- |
| Loading: the hero is not in yet (ADR 0065) | The light alone; the words stand where they will stand with the picture, so nothing moves when it arrives | State: Hero not yet in |
| Partial: no logo | The title in Big Shoulders caps, fitted to three lines in the logo's box | State: No logo |
| Ideal | Hero and logo | default |
| Error, empty library | Unchanged: the curtain and the empty page draw no hero | not drawn |
| No hero, a poster or no art | Unchanged: the card at the left and the plate or the light | not drawn |
| Longest title (The Legend of Zelda: Oracle of Seasons) with no logo | Three lines, fitted to the logo's box; the title gives way before a button | Hero: Zelda OoS, State: No logo |
| Long fact line (Ready to resume, a date, a time, a place) | One line on the bar; facts that do not fit leave, last first; the detail says them all | State: Ready to resume, at 640 x 480 |
| Missing values (Never played, no time played) | The fact is left out, as today | Hero: Tetris |
| Pale hero (Chrono Trigger, `color` #cdbba8) | No shade behind a dark logo over it; the corners of the vignette turn grey on white | Hero: Chrono Trigger |
| Bright hero (Sonic Mania) | The logo carries its own outline; reads | Hero: Sonic Mania |
| Dark hero (Super Metroid, Hollow Knight) | The band's foot vanishes into the house; the band reads as the top of the room | Hero: Super Metroid |
| Subject at the picture's foot (Celeste, Wario Land 4) | The foot fade is 22 %, so faces stay lit; today's band fade covers 60 % | Hero: Celeste |
| Dark logo on the dark house | Not solved: see "Open" | Hero: Chrono Trigger at 900 x 412 |
| Row counts 0, 1, many | Unchanged: the strip is the Floor's | |

## Copy

No new string. Every visible string is one Velvet already says:
`Play`, `Resume`, `Play on <place>`, `More`, `Menu`, the clock label from
Korri, the system badge (`GBA`), `Ready to resume`, `Played 2 days ago`,
`14 h 20 min`, `This device`, `Never played`, `All games`.

## Eye flow (1920 x 1080, one window)

| First | Then | Then | Quiet | The one filled action |
| --- | --- | --- | --- | --- |
| The picture's subject | The logo | Play | The facts, the clock, the strip | Play |

Checked in the renders: the eye lands on the subject in the band, then on the
logo at its lower left, then on Play, the only filled shape on the screen.

## Held back

| Considered | Why not |
| --- | --- |
| A blurred copy of the hero behind the band | Live blur is forbidden (the authoring gate). A stored blurred copy needs korrid to make and publish another asset; that is a schema change with no producer yet (#422 says so for the colour). Possible later, not needed for A |
| The bleed (B) as a flourish on top of A | A pole or bright stripe on all 10 heroes in the deck |
| A slow pan across a cut picture (Ken Burns) | Every frame is a cut picture; the owner ruled out heavy cuts |
| A mirror of the picture under it, as a stage floor | The software renderer has no flip or rotation |
| A frame or rule around the picture | Decoration with no information |

## Grounding

Steam's library page puts the same 3.1 to 1 art (`library_hero`) at the top
at full width, the logo over its lower left, and the Play button with the
last-played and play-time facts on a bar under it. This is where the format
comes from. Inferred from memory of the shipped product, not checked against
a screenshot here: the Mobbin tools are not available in this runtime, so no
Mobbin reference was searched.

## Cost

| Item | What |
| --- | --- |
| Less picture | On 16:9 the band is 57 % of the height; the rest is light. On tall windows the picture is small (15 % of a portrait phone's height). Today's fill feels bigger, because it is a cut |
| Quiet field | In the Stage pane alone and on tall windows, the light under the words is empty. It is the stage light, not a hole, but it carries no information |
| `src/plan.ts` | New fields for the band (height, x, `under`, the logo's box, the bar's row count) and the two constants; tests at one pixel either side of `UNDER_LOGO` and `BAND_SHARE`, and the sweep |
| `src/view.ts`, `src/art.ts` | `fill` false for `home` and `detail`; the pick and the detail lay out from the band |
| `VelvetStage.slint` | The band's foot (22 % not 60 %), no `velvet-30` over a band, the narrowed band at the right edge with a left fade, the logo's shade |
| `VelvetPick.slint` | The logo over or under the band, the bar (buttons and facts on one line) |
| `drift-scale` | 0.10 to 0.03 (opinion: a 10 % push cuts 10 % at rest) |
| Gates | `the_backdrop_has_no_seam` must hold with the new foot; `contrast-gate` should cover a logo's shade over a picture |
| Depends on #422 | The light below the band is the art's colour. Without #422 it is the title's gel, which works but does not match the picture |

What A does not solve: a dark logo on the dark house (below), and the empty
light on tall windows.

What could still go wrong: a hero whose top edge is bright under the clock
(the top shade helps; check on device); the radial vignette greys the
corners of a pale hero; the thresholds are guesses until seen on the Thor
and the TV at viewing distance.

## Open

- **A dark logo on the dark house.** Chrono Trigger's logo (`color`
  #59272d) is hard to read where it stands on the house (900 x 412, and on
  the Stage pane alone). Today's fill has the same problem over a dimmed
  picture. Proposal: when the logo's `color` is dark (lightness under 0.3),
  the title in caps stands in its place, unless the logo stands over a pale
  picture. The `color` Korri sends for a logo is the art's dominant colour;
  on Hades it reads #020101 for a red and white logo, so this rule needs
  #422's extractor (ignore near-black) before it can be trusted.

## Checks

- `review-deck check deck.json --toggles` in the mockup folder: 273 frames
  Passed (7 devices, 2 pages, every toggle once, 6 tour moments), run with
  Chromium flags that make raster deterministic (`--disable-partial-raster
  --run-all-compositor-stages-before-draw --num-raster-threads=1`). Without
  them, repeat captures differed by 1 level in at most 1 of 255 per pixel.
- Velvet's own frames at 1920 x 1080, 1280 x 720, 1240 x 1080, 640 x 480
  and 480 x 800 for `home`, `detail` and `row`, through
  `velvet_package.rs` (`frames`), to confirm today's cut.

## The one thing not to lose

The picture is whole and stands still: every hero has one shape, so the
words never move when the game changes, and only the picture and its light
cross-fade.
