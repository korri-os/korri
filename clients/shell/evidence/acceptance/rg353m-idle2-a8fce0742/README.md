# RG353M per-thread idle CPU and RSS: fe08a22e6 (old) vs a8fce0742 (new)

2026-11-04, same setup as `../rg353m-idle-fe08a22e6` (kiosk, inputd,
inputplumber and korri-limits stopped and restored by trap; scaling_max_freq
1.8 GHz). Each bundle ran once: Zone still (15 s, before its attract opens 30 s
after the last press), Zone attract (30 s), Pico (30 s), Zone still again
after switching back (15 s). One core = 100 %. "band" is CPU of threads that
exited during the window: the per-frame band threads of the parallel raster.
Every other thread (logic, korrid requests, input, watch) read 0 %.

| Window | old total | old main | old band | new total | new main | new band | old RSS / heap (MB) | new RSS / heap (MB) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Zone still | 41 % | 23 % | 18 % | 19 % | 18 % | 0 % | 142 / 105 | 98 / 61 |
| Zone attract | 125 % | 52 % | 72 % | 101 % | 61 % | 38 % | 142 / 105 | 99 / 61 |
| Pico | 12 % | 11 % | 0 % | 13 % | 12 % | 0 % | 180 / 143 | 105 / 74 |
| Zone still, after Pico | 76 % | 33 % | 43 % | 19 % | 19 % | 0 % | 183 / 145 | 124 / 85 |

During both attract windows scaling_max_freq read 1416 MHz at the end and the
SoC was at 69-71 °C: thermal throttling, not korri-limits.

## 302c53a6b (identical frames not drawn), same script, 23:28-23:33

The windows did not stay on their surfaces. The script's "attract" window
(23:30:41-23:31:11) was Zone's attract until 23:31:09, when the shell unloaded
Zone: "zone unloaded (time): view ran 52 ms, over the 50 ms budget" (the view
at the attract's next game), and Pico was restored. The L1+R1 that followed
went to Zone again; at 23:31:51 Zone was unloaded the same way (58 ms). So the
"pico" window was Zone (still, then attract), and "Zone still again" was Zone.
time_in_state shows most of each window at 408-1104 MHz: with the load lower,
the governor no longer held 1.8 GHz (scaling_max_freq stayed 1.8 GHz;
57 °C or less, not thermal).

| Window (what ran) | Total | main | band | RSS / heap (MB) | time at 1.8 GHz |
| --- | --- | --- | --- | --- | --- |
| Zone still, 15 s | 18 % | 18 % | 0 % | 99 / 61 | 16 % |
| Zone attract 28 s, then Pico 2 s | 49 % | 37 % | 12 % | 56 / 27 (Pico) | 11 % |
| Zone still then attract, 30 s | 22 % | 20 % | 1 % | 56 / 26 (Pico) | 15 % |
| Zone still, 15 s | 16 % | 15 % | 0 % | 119 / 82 | 14 % |
