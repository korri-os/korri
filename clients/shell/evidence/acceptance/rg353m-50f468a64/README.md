# Acceptance session: RG353M, `stage-review-3` 50f468a64

On 2026-11-04, 20:20–20:25 device time. The shell and fixture korrid were built
with `clients/shell/preview-device.nix` (from the `preview` scratch worktree,
plus button logging in fixture-korrid; not committed) as a /tmp bundle. The
fixture korrid ran with `--evdev` on the real pad (every button and every
scripted press is logged as `PAD` in `fixture.err`). `accept-run.sh` stopped
`korri-chromium-kiosk`, `korri-inputd` and `inputplumber` and restored them
with a trap; `after.txt` shows all three active. Before the run, DPMS was
turned on through swaymsg and the backlight went from `bl_power=4` (off) to `0`.
Screenshots are from grim on the compositor; RSS and CPU are read from
`/proc` after each step (`steps.txt`). The SP (10.42.5.1) was not run: a game
(`korri-game-af82d2ce…`, eggshell-rust) ran in korri-games.slice at every
check.

The pad was driven by script through the fixture's input (scripted presses);
no button was pressed by hand.

| # | Step | Expected | Seen | Screenshot |
| --- | --- | --- | --- | --- |
| 1 | Zone start | Games tab, a focused card | Games, Celeste Classic focused | 01-zone-start.png |
| 2 | right | next card | Hollow Knight focused, band moved | 02-zone-right.png |
| 3 | down | the card's actions | PLAY focused (red triangles) | 03-zone-down.png |
| 4 | left | stays in actions row (PLAY is first) | PLAY focused | 04-zone-left.png |
| 5 | up | back to the card | Hollow Knight card focused | 05-zone-up.png |
| 6 | down, A ("open game") | Zone has no game page: A on PLAY launches | launch title card "Hollow Knight" (LaunchGame sent) | 06-zone-open-game.png |
| 7 | MENU | menu or nothing on the title card | Games screen (title card ended); no menu | 07-zone-menu.png |
| 8 | B | close | Games screen | 08-zone-menu-closed.png |
| 9 | A (launch) | launch | title card "Hollow Knight" (LaunchGame) | 09-zone-launch.png |
| 10 | fixture overlay on | the Overlay window | "Paused" overlay with RESUME | 10-zone-overlay.png |
| 11 | B | dismiss the overlay | title card again (DismissGameplayOverlay sent) | 11-zone-overlay-back.png |
| 12 | fixture overlay off | Surface | title card | 12-zone-overlay-off.png |
| 13 | B ×3 | back out to Games | Games screen | 13-zone-back-out.png |
| 14 | idle 50 s | attract (45 s Tick) | attract card "Tetris" | 14-zone-attract.png |
| 15 | L1+R1 | switch to Pico | Pico library | 15-zone-switched.png |
| 16 | Pico start | library, a focused cart | Celeste Classic | 16-pico-start.png |
| 17 | right | next cart | Hollow Knight | 17-pico-right.png |
| 18 | down | next row | MENU button focused (bottom bar) | 18-pico-down.png |
| 19 | left | stays | MENU focused | 19-pico-left.png |
| 20 | up | back to the shelf | Hollow Knight | 20-pico-up.png |
| 21 | down, A ("open game") | A on MENU opens the menu | menu FIND / SETTINGS / VIEW | 21-pico-open-game.png |
| 22 | MENU | menu action | view switched to GRID, menu open | 22-pico-menu.png |
| 23 | B | close | grid | 23-pico-menu-closed.png |
| 24 | A | open the game | "LIBRARY > Hollow Knight" page, PLAY | 24-pico-launch.png |
| 25 | fixture overlay on | the Overlay | "Paused / Resume" overlay | 25-pico-overlay.png |
| 26 | B | dismiss | game page | 26-pico-overlay-back.png |
| 27 | fixture overlay off | Surface | game page | 27-pico-overlay-off.png |
| 28 | B ×3 | back out | library grid | 28-pico-back-out.png |
| 29 | idle 50 s | attract | "Lantern Keep — PRESS ANY BUTTON" | 29-pico-attract.png |
| 30 | L1+R1 | switch to Zone | Zone Games (band settling) | 30-pico-switched.png |
| 31 | 2 s later | Zone settled | Games, Celeste Classic | 31-zone-again.png |

No crash, no blank or wrong screen, and no lost focus: every screenshot has a
focused control or an attract screen.

RSS: 141.6 MiB at start, 147.8 MiB after the Zone script, 184.9 MiB with Pico,
189.3 MiB back on Zone (both surfaces' fonts and caches loaded once).

CPU (from `cpu_ticks`, 100 per second): 66 % of a core during Zone's 50 s
idle/attract window, and 91 % during Pico's. `shell.log` shows why for Pico:
`SHELL_STATS fps=5600–8700` with frame intervals of 0.1 ms for the whole Pico
period, against 48–60 fps on Zone. On Pico, frames have nothing to draw between
the focus cycle's 200 ms steps. Upstream winit's software path presents only
when the damage is not empty, and with an `animation-tick()` binding active it
asks for the next frame at once. With no present there is no frame callback to
pace it, so the event loop spins. The quiet-frame skip makes each such frame
cost 0.1 ms instead of a walk, which turns the spin into thousands of frames a
second. This needs a fix before landing: pace frames that draw nothing (for
example by waiting for the next frame interval, or by presenting with no
change).

Install (`korri-shell install`) took about 60 s of CPU on the device and wrote
5 503 lines of "The use of the ElementHandle API requires the presence of
debug info …" to the journal (removed from `shell.log` here).
