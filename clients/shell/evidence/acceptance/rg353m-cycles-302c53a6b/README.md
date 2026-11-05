# RG353M work in megacycles a second: kiosk vs native shell (302c53a6b)

2026-11-04 23:36-23:39 device time. korri-limits stopped and the 61 % cap
lifted for every window, the kiosk's included; the schedutil governor left as
it is (no pinned clock). Work = the unit's cgroup CPU time over the window x
the policy's mean frequency over the window (time_in_state weighted; one
policy covers the four cores). `idle3-run.sh`; raw lines in `measure.txt`.

The script's windows did not all hold their surface: the shell unloaded Zone
at 23:38:04 by its 50 ms budget ("view ran 50 ms"), at the attract's next
game, and restored Pico. What ran in each window:

| Window | What ran | CPU (cores) | mean MHz | Mcycles/s | at 1.8 GHz | °C |
| --- | --- | --- | --- | --- | --- | --- |
| kiosk-idle 30 s | Chromium kiosk, Zone web, idle | 0.355 | 686 | 243 | 10 % | 52.5 |
| zone-still 15 s | Zone still | 0.183 | 914 | 167 | 15 % | 57.8 |
| zone-attract 30 s | Zone attract ~7 s, then Pico ~22 s | 0.393 | 789 | 310 | 9 % | 51.3 |
| pico 30 s | Zone (still, after L1+R1 from Pico) | 0.216 | 847 | 183 | 13 % | 55.0 |
| zone-still-again 15 s | Pico (after L1+R1) | 0.266 | 541 | 144 | 3 % | 50.6 |

Budget unloads in this session: 1 (two more in the 23:30 session, 52 and
58 ms). The attract alone could not be measured: it is unloaded at its
second game at these clocks.
