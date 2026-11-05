# RG353M session: native-shell-fix-5 at aac32548b (dist build)

2026-11-05 09:49-09:54 device time. Same method as `../rg353m-cycles-699670725`
(korri-limits stopped, cap lifted, schedutil not pinned; work = cgroup CPU
time x time_in_state-weighted mean MHz). `s5-run.sh`; raw lines in
`measure.txt`, PSS in `rollup.txt`, shell lines in `shell.log`. The fixture
korrid serves 40 games; the four L1+R1 switches are a fixture gamepad's.
The trap restored the kiosk, inputplumber and inputd (all active afterwards)
and korri-limits with the saved cap.

| What ran | CPU (cores) | mean MHz | Mcycles/s | at 1.8 GHz | °C |
| --- | --- | --- | --- | --- | --- |
| Chromium kiosk idle 30 s | 0.345 | 717 | 247 | 11 % | 46.7 |
| Zone still 25 s | 0.166 | 776 | 129 | 13 % | 49.4 |
| Zone attract 60 s | 0.521 | 753 | 392 | 10 % | 47.8 |
| Zone after four switches 20 s | 0.162 | 750 | 121 | 12 % | 45.6 |

Before (699670725): kiosk 243, Zone still 155, Zone attract 424 Mcycles/s.

Loads (SHELL_STATS):

| Load | compile_ms | first_frame_ms |
| --- | --- | --- |
| Zone, first in the process | 3 068 | 3 299 |
| Pico, first (switch 1) | 852 | 1 100 |
| Zone again (switch 2) | 0.004 | 200 |
| Pico again (switch 3) | 0.004 | 200 |
| Zone again (switch 4) | 0.005 | 229 |

Before, every Zone switch compiled: first_frame 2 834-4 044 ms.

Shell PSS (40 games):

| When | PSS | anonymous | file-backed | shmem |
| --- | --- | --- | --- | --- |
| Zone after the attract | 78 716 | 60 700 | 16 478 | 1 538 |
| Pico after switch 3 (Zone's definition kept) | 82 418 | 64 548 | 16 670 | 1 200 |

No budget unloads. fps 59.6-75.2 in every 5 s window except the one holding
Pico's first load (49.6: a 1 056 ms frame while it loaded).

The kiosk screenshot (`kiosk-idle.png`, 15 s into its idle window) shows the
kiosk's LIBRARY page with a "Catalog problem" dialog (no installed Linux
runner for two games): the device's own state.
