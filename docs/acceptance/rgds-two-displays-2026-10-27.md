# RG DS two displays — 2026-10-27

The RG DS plays a one-pane game on its top display with the bottom display
off, and opens the gameplay overlay on the bottom display. The library shows
on the main (top) display only. One portal window spanning both displays was
**rejected on hardware** and by owner decision (ADR 0039, amended
2026-10-27). Nintendo DS panes across both displays are **not yet accepted**.

## Device and build

| Fact | Value |
| --- | --- |
| Device | Anbernic RG DS, booting from eMMC |
| Displays | DSI-2 (top, main) and DSI-1 (bottom), both 640×480 touch |
| Final system | `1wqzjsag` |
| Portal bundle | `88vxm5fy`: core `dfb38ea55` plus Pico |
| Verified tree | `823ea4985`: full verification passed on unzen (`korrid-check`, `pico-check`, 12 device and product checks, `rgds-update-check`, `korri-product-module`) |

The commit hashes below name the build branch before it was rebased onto
local main `3601c3610`. Their rebased equivalents carry the same subjects.

## Accepted on hardware

| Behaviour | Result | Change |
| --- | --- | --- |
| A Game Boy game runs fullscreen on the top display; the bottom display is off. | Accepted. | — |
| The game has input focus as soon as it launches. | Accepted. | `978d9d02f` |
| Home opens the gameplay overlay on the free bottom display, and powers that display on every time. | Accepted. | `48b316fd4` |
| The game dims after Home, 50–57 ms after korrid receives the Home press. | Accepted. | `d59e9385c`; fullscreen opacity workaround `df10d3679` |
| Opening the overlay takes about 1.0 s, of which the panel's power-on is about 550 ms (measured). Returning to the game takes about 0.6 s. No inputd timeouts. | Accepted. | — |
| The library shows on the main display only. | Accepted. | `dfb38ea55` |
| Cold kiosk start without a presentation stall. | 6/6. | Build's harness |
| Play → leave → end without a stall. | 6/6. | Build's harness |
| Attract ends on any input, including the d-pad. | Accepted. | Pico |

The stall counts come from build's harness. Its detector and its positive and
negative controls are in the
[main-display portal brief](../briefs/2026-10-27-main-display-portal.md), for
example the
[Loading control](../briefs/assets/2026-10-27-single-detector/main-only-loading-positive.png)
and the
[attract row](../briefs/assets/2026-10-27-single-detector/main-only-attract-row.png)
on the main-only build.

## Rejected on hardware

One window spanning both displays was rejected. Chromium 153 stalled while
presenting in 3–4 of 6 cold starts in every spanning variant. Main-only
stalled in 0 of 6. The owner rejected spanning as a mechanism, not only for
now: a device's displays need not form a rectangle. ADR 0039 (amended
2026-10-27) records the decision, and the brief records the measurements.

Pico keeps its two-region layout, unused on devices, until per-display
surfaces are designed.

## Not yet accepted

Nintendo DS panes across both displays, and touch on the bottom pane, are
not accepted. They need the published melonDS/RetroArch plugin candidate
pinned to this core. That work is in progress.

## Screenshots

Grim captures of one output each, 640×480. The four game and overlay captures
come from earlier builds in the same acceptance session, before the final
system; they show the behaviour, not the final build. The library capture is
from the main-only build.

| Capture | What it shows | Captured (UTC) |
| --- | --- | --- |
| [library-main-display.png](rgds-two-displays-2026-10-27/library-main-display.png) | DSI-2: the library on the main display. | 2026-10-27 15:39 |
| [gameboy-top-display.png](rgds-two-displays-2026-10-27/gameboy-top-display.png) | DSI-2: a Game Boy game on top; the bottom display was off. | 2026-10-26 02:26 |
| [dim-after-home.png](rgds-two-displays-2026-10-27/dim-after-home.png) | DSI-2: the game dimmed after Home. | 2026-10-26 19:05 |
| [overlay-bottom-display.png](rgds-two-displays-2026-10-27/overlay-bottom-display.png) | DSI-1: the gameplay overlay on the bottom display. | 2026-10-26 18:00 |
| [resumed.png](rgds-two-displays-2026-10-27/resumed.png) | DSI-2: the game resumed at full brightness. | 2026-10-26 19:05 |

## Issues filed along the way

- #264
- #266
- #269: the Paused label
- #287: the power key powers the device off
- #301: `pico-check` needs node
- A comment on #188
