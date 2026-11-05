# RG353M megacycles: 699670725 (zone-2's attract fix 597896ae3 + band pool 43c9c91db)

2026-11-05 00:17-00:19 device time; same method as `../rg353m-cycles-302c53a6b`
(korri-limits stopped, cap lifted, schedutil not pinned; work = cgroup CPU
time x time_in_state-weighted mean MHz). The kiosk row is carried over from
that session. Zone stayed loaded the whole time: no budget unload.

| What ran | CPU (cores) | mean MHz | Mcycles/s | at 1.8 GHz | °C |
| --- | --- | --- | --- | --- | --- |
| Chromium kiosk idle 30 s (9bbce5c2e) | 0.355 | 686 | 243 | 10 % | 52.5 |
| Zone still 25 s | 0.195 | 796 | 155 | 12 % | 52.5 |
| Zone attract 60 s | 0.513 | 827 | 424 | 9 % | 61.1 |
| Pico 30 s | 0.261 | 573 | 149 | 3 % | 48.9 |

Zone still is 25 s, not 30: the attract opens 30 s after the last press, and
the window starts 6 s after the shell does.
SHELL_STATS: Pico at 48-50 fps (interval p50 21 ms), Zone still at 61-70 fps;
both from the redraw pacing, fixed in 87900ea3f.
