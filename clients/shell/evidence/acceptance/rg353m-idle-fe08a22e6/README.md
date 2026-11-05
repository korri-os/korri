# RG353M idle CPU and RSS after the frame-pacing fix (fe08a22e6)

2026-11-04 20:55–21:00 device time. The shell and the fixture korrid ran as in
the acceptance session (`idle-run.sh`). The kiosk, inputd, inputplumber and
korri-limits were stopped and restored by a trap. scaling_max_freq was 1.8 GHz
for the run and restored to 1104 MHz afterwards. Nothing was pressed except
L1+R1 to switch.

| Window | Shell CPU (one core = 100 %) | Fixture korrid CPU | Shell fps (SHELL_STATS) |
| --- | --- | --- | --- |
| Zone idle, 60 s | 125 % (6 threads) | 0 % | 56–60, p95 interval 23–24 ms |
| Pico idle, 60 s | 13 % | 1 % | 60–61 (paced) |
| Zone idle again, 30 s | 93 % | 0 % | 59–60, p95 17.5 ms |

Before the fix (acceptance session, 4b1cb626b, capped at 1104 MHz): Pico idle
ran at 91 % with 5600–8700 frames a second. Zone was at 66 % over 50 s of
idle and attract, measured at a lower clock, so it does not compare directly.

During the Zone minute, scaling_max_freq read 1416 MHz at the end, and
time_in_state shows time at 600–1800 MHz. Something lowered the cap even with
korri-limits stopped; I suspect thermal throttling (not checked).

RSS from `/proc/PID/smaps_rollup` (smaps.txt):

| Point | Shell RSS | of which [heap] | Fixture korrid RSS |
| --- | --- | --- | --- |
| Zone loaded (idle) | 143.1 MB | 105.0 MB | 4.4 MB |
| Pico loaded too | 181.0 MB | 143.1 MB | 4.9 MB |
| Back on Zone | 188.2 MB | 145.5 MB | 4.9 MB |

The shell's mapped files (its 18.8 MB binary and bundled libraries) are about
21 MB of that. Most of the rest is anonymous heap.
