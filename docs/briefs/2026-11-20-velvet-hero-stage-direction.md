# Velvet: the whole hero on the stage (direction brief)

> **Superseded, 2026-11-20.** ADR 0091, section "The Velvet stage",
> supersedes this brief. Velvet did not adopt the Band or the Echo. The
> stage uses a background picture near 16:9 when one exists, and otherwise
> cuts the wide hero to fill. The rest of this brief stays as the record.

Round 2, 2026-11-20. This brief changes no code. Nothing here is built
until the owner approves the design.

Owner decisions (ADR 0091, section "The Velvet stage"):

- A picture near 16:9 fills the stage when one exists (IGDB artworks are
  the likely source).
- The Band is used only when the only art is a wide hero (SteamGridDB,
  about 3.1:1). The hero is shown whole, never cut.
- The bar: "the design can't be shitty". The Band must beat the immersion
  of today's cut-to-fill stage. Being acceptable is not enough.

Mockups: `~/code/sandbox/korri-mockups/hero-stage/` (outside the
repository: its `art/` is third-party art that may not be committed;
`CREDITS.md` there). Round 2 is `band.html` and `band-plan.js`; the motion
spec and tokens are `DIRECTION.md` there. The review deck is `deck.json`
there, round 2.

## Recommendation

**The Echo.** The whole hero stands at the top of the stage, sharp and
still. Behind it, the same picture is cut to fill the whole stage and set
far back: enlarged, under a veil of the house lit by the game's colour. The
hero's foot dissolves into its own echo, so there is no edge where the
picture stops. The stage is picture from edge to edge, as it is today, and
the hero is still whole.

| | Today (cut to fill) | Round 1 Band | Echo (recommended) |
| --- | --- | --- | --- |
| Hero shown | 57 % of its width on 16:9; 15 % on a portrait phone | 100 % | 100 % |
| Stage covered by picture | 100 % | 57 % (16:9), the rest flat light | 100 %: 57 % sharp, 43 % the echo |
| Where the band meets the stage | no band | a dark belt, then flat light | no edge: the foot dissolves into the same picture |
| Words move when focus moves between a 16:9 game and a Band game | — | — | no: the words' places depend on the window only |
| New data from Korri | none | none (but the light needs a colour) | none |

Cost, in short: the echo is the cut picture again, so on some heroes a
large, dim fragment of the subject stands behind the words (a second
Wario hand, Chrono Trigger's legs). The dim copy is drawn with Slint's
nearest-pixel sampling, so it is blocky under the veil at 1.7 times and
more. The work is a new stage composition, not a new constant (see
"Cost").

## The four weaknesses, and how the Echo answers them

| # | Weakness (round 1) | Echo's answer | Cost | Seen on |
| --- | --- | --- | --- | --- |
| 1 | A pale hero makes a white bar at the top (Chrono Trigger) | The band's top falls to House down 60 % over its top 40 % (a valance). The stage's two sides fall to 60 % (left) and 48 % (right). The echo's veil is the game's own dark colour, not grey. Three linear gradients over every band, the same for every picture. | A pale hero still has a bright middle. Its echo is a dull olive field. On Chrono Trigger's top row the mean luminance falls from 115 (round 1) to 46 of 255; the band's mean falls from 148 to 121. A dark hero shows none of it, so nothing is lost there. | Game: Chrono Trigger, pale hero |
| 2 | A dark logo on the dark stage; the stored logo colour is unreliable (Hades `#020101`) | A light rim round every logo: the logo's own shape in House light (Slint `colorize`), eight copies at 1.2 % of its height (alpha 0.50) and eight at 3 % (alpha 0.14), under the logo. It reads no colour from Korri. A logo over the band also stands on the band's shaded foot (House down to 45 %). | 16 extra image draws a logo. A light logo gets a faint glow; a logo with a baked soft shadow (Hollow Knight) gets a slightly brighter edge. A logo smaller than about 40 px (320 x 240) blurs a little. | Logo edge: Light rim / No rim; Window: Stage pane alone; Game: Chrono Trigger |
| 3 | Tall windows and the Stage pane alone leave plain space under the words | The space under the words is the echo: the picture, dim. The words stand where round 1 put them. | On a pale hero the space is a dim plain colour again. The echo carries no information; it is atmosphere. | Window: Stage pane alone; phone portrait |
| 4 | The Band covers 57 % of a 16:9 height; less immersive than full bleed | The whole stage is picture, as today's cut. The sharp hero is in front; its last 35 % dissolves into the echo in 24 strips of falling opacity (Slint has no mask). At rest only the echo drifts (6 %, 18 s each way); the hero stays whole and still. | The double image: shapes in the echo repeat the hero's shapes, larger. It reads as depth on most heroes; on a few it reads as a ghost. | Treatment: Echo / Today; the "At rest" clip |

### With #422 (better, not needed)

#422 asks korrid for a colour a surface can light its stage with. The
colour korrid computes today (`dominant_color` in
`services/korrid/src/game_assets.rs`) drops near-black, near-white and grey
pixels on purpose, so it can never say that a picture is pale or that a logo
is dark. Verified in the source. The Echo therefore decides nothing from it
except the light's hue. If korrid later stores a lightness for each picture
(a schema change; no producer exists, so it is not proposed here), two
things could improve: the rim only on dark logos, and a stronger valance
only on pale heroes. Neither is needed for the design to work.

## The three treatments explored

All three keep round 1's slots (the band at the window's width from the top
edge, the logo over its lower left or under it, Play and the facts on one
line under it) and the same rim. They differ only in how the band meets the
stage.

| | Echo (depth) | Spill (light) | Screen (an object) |
| --- | --- | --- | --- |
| The idea | The same picture, enlarged and far back, fills the stage; the hero dissolves into it | The hero's foot pours into its own light, which runs down the house | The hero is a lit screen inside the house, its light on the room round it |
| Weakness 1, pale | Valance, sides, a tinted echo | Valance, sides; the foot turns to the light colour | A pale screen in a dark room is a cinema; it reads as meant |
| Weakness 2, dark logo | Rim | Rim | Rim |
| Weakness 3, plain space | Filled with the echo | Not solved: a plain field of light. One window: the strip grows into the room (up to 1.5 times), "the shelf rises" | Not solved: the room round the screen is plain light |
| Weakness 4, immersion | Full bleed, like today | Less than today: 57 % picture, the rest coloured light | Least: the picture is inset, 51 % of a 16:9 height |
| Eye lands first | The hero's subject, then the logo, then Play | The same | The screen as an object, then the subject |
| Verdict (opinion) | Beats cut-to-fill: as full, and the hero is whole | Calm and clean, close to round 1; not more immersive than cut | The most intentional; the least immersive |

## Direction (the Echo)

The stage is a cinema in which the film is already running. The game's
picture stands whole across the top of the house, sharp and still; behind
and below it, the same picture fills the room, enlarged and far back in the
dark, lit by the game's own colour. Where the picture ends, it melts into
its echo, so the eye never finds an edge. The words stand in the same
places for every game: a 16:9 game and a Band game differ only in the
picture and the light, which cross-fade. At rest the echo breathes slowly;
the picture in front never moves and is never cut. The one bold thing is
still the stage light, now carried by the picture itself.

## Token plan

No new colour, type or motion token. The palette, faces and motion table in
`docs/design/surfaces/velvet/DIRECTION.md` stand. New plan constants:

| Kind | Name | Value | Note |
| --- | --- | --- | --- |
| Plan | `HERO_RATIO` | 3.1 | exists |
| Plan | `BAND_SHARE`, `UNDER_LOGO` | 0.8, 0.55 | round 1, unchanged |
| Stage | `ECHO_VEIL` | 0.80 | the veil's alpha over the echo |
| Stage | `ECHO_TINT` | 0.35 | the veil's colour: House down 35 % of the way to the stage light |
| Stage | `ECHO_FOOT`, `SLICES` | 0.35, 24 | the share of the band that dissolves, in strips |
| Stage | `VALANCE`, `VALANCE_H` | 0.60, 0.40 | the band's top shade |
| Stage | `WING`, `WING_L`, `WING_R` | 0.60, 0.30, 0.12 | the stage's side shades, full height |
| Logo | `RIM_NEAR`, `RIM_FAR` | 0.012, 0.03 of the logo's height | alpha 0.50 and 0.14 |
| Motion | `drift-scale` | 0.06, was 0.10 | applies to the echo and to a 16:9 picture; the band never drifts |
| Stage | dimmer over a band | none | today `velvet-30` over the whole cut picture; a 16:9 picture keeps it |

## Layout as a function of size

Round 1's plan stands (the rules are in `band-plan.js`, `bandPlan`). The
words' places are a function of the window only, never of the art:

| Window | Band | Logo | Logo box | Words at |
| --- | --- | --- | --- | --- |
| 1920 x 1080, one window | 1920 x 620 | over the band | 211 | y 650 |
| 1920 x 1080, Stage pane alone | 1920 x 620 | under | 159 | y 656 |
| 1280 x 720, one window | 1280 x 413 | over | 140 | y 436 |
| 1240 x 1080, one window | 1240 x 400 | under | 219 | y 430 |
| 640 x 480, one window | 640 x 207 | over | 70 | y 224 |
| 640 x 480, Stage pane alone | 640 x 207 | under | 105 | y 227 |
| 412 x 900 | 412 x 133 | under | 90 | y 153 |
| 900 x 412, one window | 731 x 236, right edge | over | 80 | y 254 |
| 320 x 240 | 318 x 103 | over | 36 | y 119 |
| The detail, 320 x 240 | 183 x 59, right edge | over | 36 | buttons one under the other |

A 16:9 picture fills the stage at every size (owner's rule) and puts its
words in the same places, over the picture with today's scrim.

## Wireframes

1920 x 1080, one window:

```
+--------------------------------------------------------------+
|░ HERO 1920 x 620, whole, still            4:19 AM  [ Menu ] ░|
|░ (valance at the top, the sides shaded)                     ░|
|  [LOGO + rim, over the lower left]                           |
|~~~~~~ the hero's last 35 % dissolves into the echo ~~~~~~~~~~|
|  [> Play]  GBA  Played 2 days ago  14 h 20 min  This device  |
|  THE ECHO: the same picture, cut to fill, enlarged, veiled   |
|  [tile][tile][tile][tile][All games]                         |
+--------------------------------------------------------------+
```

1920 x 1080, the Stage pane alone (Thor top display):

```
+--------------------------------------------------------------+
|░ HERO 1920 x 620, whole, still            4:19 AM  [ Menu ] ░|
|~~~~~~~~~~~~~~~~~~~~~~ dissolves ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~|
|  [LOGO, 159 high, rim]                                       |
|  GBA  Played 2 days ago  14 h 20 min  This device            |
|  [> Play]                                                    |
|  THE ECHO, darker toward the foot                            |
+--------------------------------------------------------------+
```

1240 x 1080, 640 x 480 and 320 x 240:

```
+------------------------------------+  +---------------------------+  +----------------+
| HERO 1240 x 400   4:19 [Menu]      |  | HERO 640 x 207 4:19 [Menu]|  | HERO 318 x 103 |
|~~~~~~~~~~~ dissolves ~~~~~~~~~~~~~~|  | [LOGO] ~~~~~ dissolves ~~~|  | [LOGO]  ~~~~~~ |
| [LOGO, 219 high]                   |  | [> Play] GBA Played 2 d...|  | [> Play] GBA ..|
| GBA Played 2 days ago 14 h 20 min  |  | ECHO [t][t][t][t][All]    |  | [t][t][t][t][A]|
| [> Play]                           |  +---------------------------+  +----------------+
| ECHO                               |
| [tile][tile][tile][tile][All]      |
+------------------------------------+
```

412 x 900 and 900 x 412:

```
+------------------+   +----------------------------------------+
| HERO 412 x 133   |   |ECHO | HERO 731 x 236, right edge [Menu]|
|~~~ dissolves ~~~~|   |[LOGO over the echo and the fade]        |
| [LOGO]           |   |~~~~~|~~~~~~~~~~ dissolves ~~~~~~~~~~~~~~|
| GBA Played 2 ... |   | [> Play] GBA Played 2 days ago 14 h... |
| [> Play]         |   | [t][t][t][t][All]                      |
| ECHO             |   +----------------------------------------+
| [t][t][All]      |
+------------------+
```

The detail, 1920 x 1080:

```
+--------------------------------------------------------------+
| HERO 1920 x 620, whole                    4:19 AM  [ Menu ]  |
|~~~~~~~~~~~~~~~~~~~~~~ dissolves ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~|
|  [LOGO, 165 high]                                            |
|  [> Play on This device] [Play on aka]  ... More             |
|  ECHO                                                        |
|  GBC  Played yesterday  31 h  This device                    |
+--------------------------------------------------------------+
```

## Stress set

| Case | What the Echo does | Deck toggle |
| --- | --- | --- |
| Loading: the hero is not in yet (ADR 0065) | The light alone, no echo; the words stand where they will stand | State: Hero not yet in |
| Partial: no logo | The title in Big Shoulders caps, fitted to three lines in the logo's box | State: No logo: title |
| Ideal | Hero, echo, logo with rim | default |
| A 16:9 picture | Fills the stage, today's dimmer and scrim, the same word places | Game: Hades, 16:9 art |
| Error, empty library | Unchanged: the curtain and the empty page draw no hero | not drawn |
| No hero, a poster or no art | Unchanged: the card at the left and the plate or the light | not drawn |
| Longest title with no logo (The Legend of Zelda: Oracle of Seasons) | Three lines; the title gives way before a button | Game: Zelda OoS + State: No logo |
| Long fact line at 320 x 240 | Facts leave last first; the one fact left ends in an ellipsis ("Ready to res…"); the detail says them all | State: Ready to resume, at 320 x 240 |
| Never played, no time | The fact is left out | Game: Tetris, never played |
| Pale hero | Valance, sides and a tinted echo; still the weakest case | Game: Chrono Trigger, pale hero |
| Bright hero (Sonic Mania) | The echo is a bright olive field; the words still read | Game: Sonic Mania |
| Dark hero (Hollow Knight, Super Metroid) | The best case: hero and echo read as one deep picture | Game: Hollow Knight |
| Subject at the picture's foot (Celeste) | The foot dissolves over 35 %, so faces at the foot are dimmer than under round 1's 22 % fade | Game: Celeste, faces at foot |
| Dark logo | The rim | Logo edge |
| Row counts 0, 1, many | Unchanged: the strip is the Floor's | |

## Motion

| What | Kind | How |
| --- | --- | --- |
| Focus moves to another game | Structural | The new stage (hero, echo, light) and the new words cross-fade over the old in 380 ms, cubic-bezier(0.4, 0, 0.2, 1), no dark dip. The words do not move. |
| At rest | Idle | After 2 s, the echo (or a 16:9 picture) pushes in 6 % and drifts, 18 s each way, a seamless loop. The band does not move. A press stops it at once. |
| Reduced motion | — | Every change lands at once; no drift |

Evidence: the "Focus" clip in the deck (16:9 Hades, then Wario Land 4 and
Chrono Trigger as Bands), recorded at 1280 x 720. Its filmstrip shows each
cross-fade over 3 cells (100 ms a cell) where the spec says 380 ms: the
recorder dropped about a quarter of its frames, so the count is within one
cell, not exact. The logo, Play and the facts stay in the same pixels in
every cell. The reduced-motion clip shows end states only.

## Copy

No new string. Every visible string is one Velvet already says: `Play`,
`Resume`, `Play on <place>`, `More`, `Menu`, the clock, the system badge,
`Ready to resume`, `Played 2 days ago`, `14 h 20 min`, `This device`,
`Never played`, `All games`.

## Eye flow (1920 x 1080, one window)

| First | Then | Then | Quiet | The one filled action |
| --- | --- | --- | --- | --- |
| The hero's subject, in the sharp band | The logo at its lower left | Play | The echo, the facts, the clock, the strip | Play |

Checked in the renders: the sharp band is brighter and finer than its
veiled echo, so the eye lands on the band, not on the echo's enlarged
copy of the same subject.

## Held back

| Considered | Why not |
| --- | --- |
| A blurred copy behind the band | Live blur is forbidden. A stored blurred copy needs a new asset from korrid (a schema change with no producer). The echo's veil and enlargement do most of the work. |
| A pale stage for pale heroes (the house turns to paper) | Needs a lightness Korri does not have, and it breaks the dark house and the focus edge |
| The bleed (stretched edge rows) | Round 1: a pole or stripe on all 10 heroes |
| A mirror of the hero under it | The software renderer has no flip; and it reads as a 2008 web effect |
| A drift of the band itself | Every frame of it would be a cut picture |
| A frame or rule round the band | Decoration with no information (Screen keeps one, as its idea) |

## Grounding

The pattern of a whole picture over its own enlarged, dimmed copy is the
standard answer to art that does not fit its frame in shipped media apps
(album and film detail pages that sit on a blurred copy of their own art).
Inferred from memory of those products; the Mobbin tools are not available
in this runtime, so no reference was searched. Steam's library page is
still the source of the band and the logo over its lower left.

Slint facts this design depends on, read in the source of
`i-slint-renderer-software` 1.18.1 (the version the shell pins):

- A scaled picture is sampled at the nearest pixel (`draw_functions.rs`).
  The mockup draws the echo the same way.
- `colorize` paints an image's alpha in one colour (`lib.rs`). The rim
  depends on it.
- Gradients are linear and circular (`scene.rs`). The shades are linear.

## Cost

| Item | What |
| --- | --- |
| The double image | On some heroes the echo shows a large, dim copy of the subject next to the sharp one (Wario's hand, Chrono Trigger's legs) |
| Pale heroes | Improved, not solved: a bright middle and a dull echo |
| Draw cost | The echo is today's full-stage cut picture plus the band (one more image), 24 strips, the veil, and 16 logo copies. Today's stage already draws a full-stage picture and drifts it. Not measured on the RG353M or the RP Mini V2 |
| Pixel budget | None: the echo is the same image as the band |
| `src/plan.ts` | The band's slots (round 1) and the rule that a 16:9 picture uses the same slots; tests one pixel either side of `UNDER_LOGO` and `BAND_SHARE` |
| `src/art.ts`, `src/view.ts` | The stage view tells the band from a 16:9 picture (needs the picture's shape, which Korri already sends) |
| `VelvetStage.slint` | A third picture layer (the echo) per cross-fade side, the 24 strips, the veil, the shades. Round 1's dark belt goes |
| `VelvetMark` / `VelvetPick` | The rim (16 `Image` copies with `colorize`); the logo over or under the band |
| Gates | `the_backdrop_has_no_seam` must hold across the dissolve; `contrast-gate` should cover Play and the facts over the echo |

## What is still weak

- Pale heroes: better, but still the weakest case.
- The double image on heroes with a large subject at the band's foot.
- The 16:9 rule cuts 74 % of a 16:9 picture's width on a portrait phone
  (412 x 900). The owner's rule says fill; the Band would show it whole.
  Not decided here.
- The 16:9 source is not settled (ADR 0091 "Unresolved"). The deck's 16:9
  pictures are TheGamesDB fanart, as stand-ins for IGDB artworks; some
  fanart carries its own logo (Hollow Knight's did, so it is not used).
- Thresholds and the veil are guesses until seen on the TV and the Thor at
  viewing distance.

## Round 1, in short

Round 1 compared three compositions (A Band, B Bleed, C Beside) against
today's cut. The owner first picked "Today (cut to fill)", then chose the
Band for wide-only heroes after he learned that 16:9 art exists. Round 1's
problem table: the share of a 3.1:1 hero that today's cut shows is
`min(1, (W / H) / 3.1)`: 57 % at 1920 x 1080 and 1280 x 720, 37 % at
1240 x 1080, 43 % at 640 x 480, 71 % at 900 x 412, 15 % at 412 x 900. B
put a pole or stripe on all 10 heroes; C shrank the art to a card. Round 1's
slots and stress cases carry into round 2 unchanged.

## Checks

- `review-deck check deck.json --toggles` in the mockup folder: see the
  report named in the round-2 handoff.
- Pixel reads in the mockup at 1920 x 1080, Chrono Trigger: the top row's
  mean luminance 115 (round 1 band), 46 (Echo), 46 (Spill), 54 (Screen).

## The one thing not to lose

The hero is whole and still, and the stage is still full of picture: the
band in front is never cut and never moves, the echo behind it fills the
room, and the words stand in the same places for every game.
