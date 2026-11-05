# Contract section 4 memory pass marks (workstation, 2026-11-05)

`proof/run.sh` under Xvfb 640x480 over the fixture korrid, release build of
native-shell-fix-5 (each package compiled once per process). The pass mark is
81 920 KiB (80 MiB) of shell PSS.

## (a) Memory flat over 20 Zone ↔ Pico switches: met

`switch-20.csv`, PSS after each switch (KiB). The first visit of each package
adds its compiled definition; after that:

| | first repeat | last | per visit |
| --- | --- | --- | --- |
| Zone (switches 2 → 20) | 102 713 | 102 910 | +22 KiB |
| Pico (switches 3 → 19) | 97 352 | 97 637 | +36 KiB |

`tests/memory_probe.rs` bounds a round at 48 KiB. (Measured before the step 2
and 3 cuts below; they lower the level, not the slope.)

## (b) 10 000 games: 60 fps met; under 80 MiB met for Zone, NOT for Pico after Zone

The mark counts in steady use: Zone after moves and Pico after moves. PSS in
KiB, `large` scenario (Zone idle, Zone after 30 moves, then F2 to Pico and 30
moves; Zone's definition stays compiled while Pico shows):

| step | Zone idle | Zone after moves | Pico after moves | file |
| --- | --- | --- | --- | --- |
| 0. before | 94 613 | 100 263 | 110 406 | `large-10000-step0.txt` |
| 1. lowered unit shrunk to fit | 91 078 | 95 617 | 104 548 | `large-10000-step1-shrink.txt` |
| 2. + compiler-only tables dropped | 79 805 | 84 313 | 93 349 | `large-10000.txt` |
| 3. + `dist` build, desktop portal reader off | 80 297 | 85 323 | 94 385 | `large-10000-step3-dist.txt` |
| 4. + bindings evaluated in place, geometries as four fields | 77 773-77 865 | 81 239-81 547 | 87 046-87 130 | `large-10000-step4-run{1,2,3}.txt` |

Steps 0-2 ran the release binary from the target directory; step 3 a
settled copy of the `dist` binary. The same step-2 source, settled, measured
Zone 90 274-90 526 and Pico 98 774-98 922 after moves (file-backed 22.9-23.0
MB): a freshly linked file maps fewer pages, so the step 0-2 file-backed
figures are low.

fps at step 4: 60.2-62.4 in every 5 s window, three runs (`large-10000-fps.txt`).

Step 2 split by kind (`smaps_rollup`, `rollup-*.txt`):

| after moves | PSS | anonymous | file-backed | shmem |
| --- | --- | --- | --- | --- |
| Zone, step 2 (fresh file) | 84 481 | 66 564 | 17 317 | 600 |
| Pico, step 2 (fresh file) | 93 509 | 75 408 | 17 501 | 600 |
| Zone, step 2 (settled copy) | 90 274 | 66 768 | 22 906 | 600 |
| Pico, step 2 (settled copy) | 98 774 | 75 236 | 22 938 | 600 |
| Zone, step 3 (`dist`, settled) | 85 464 | 66 548 | 18 316 | 600 |
| Pico, step 3 (`dist`, settled) | 94 536 | 75 492 | 18 444 | 600 |
| Zone, step 4 (`dist`, settled) | 81 394 | 62 380 | 18 414 | 600 |
| Pico, step 4 (`dist`, settled) | 87 198 | 67 992 | 18 606 | 600 |

The file-backed part is almost all the binary's code (shared, clean,
reclaimable). The anonymous part is the heap: Zone's compiled definition, the
pages its compile left partly used, Pico's definition, the instances.

Compiler-only tables: a definition that keeps no type loaders drops each
property's analysis (`prop_analysis`, read only by the code generators) and
the items' source locations (`debug_info`, read only by highlighting).
`vendor/README.md`.

Keeping Zone's definition while Pico shows costs little: with each package
compiled at every load instead, Pico after moves was 91 622 KiB (step 2
without the release below), because the freed definition's pages stay partly
used.

## Tried and not kept

- `MADV_DONTNEED` on the binary's code pages after a compile (9b978dc3e,
  reverted): the code faults back in as it runs, so it moved the measurement,
  not the use, and cost page faults on presses.


| | Zone, compiled and created (headless RSS) | why not kept |
| --- | --- | --- |
| compile on a thread, lower on another (own malloc arenas) | 73.0 → 66.9 MB headless; real shell no change (91.9 vs 91.1 MB) | the instance had been reusing the freed compiler pages |
| the above, then copy the unit into the main arena | 73.1 MB | worse: what the copy leaves behind keeps the arena's pages |
| lowering twice, keep the second | no change | |
| jemalloc | 123 MB | returns memory only after a delay |
| mimalloc (purge delay 0) | 80.7 MB after compile | worse than glibc (67.8 MB) |
| per-package lowering threads (own arenas), real shell, after moves | Zone anon 71.1, Pico 79.8 MB (kept); Pico 76.3 (not kept) | worse than one arena (66.8 / 75.2) |
| jemalloc, decay 0 (one arena / default), real shell, after moves | Zone anon 67.7-69.8, Pico 72.2-74.4 MB | at most 3 MB on Pico, worse on Zone |

Where Zone's heap still goes (heaptrack, compiled and created, live 31.1 MB):
bindings (`property_init`, 376 bytes each, 18 580) 9.6 MB, item geometries
5.3 MB (each a struct literal held as a `BTreeMap` leaf of 11 slots for 4
fields), other lowered expressions 7.4 MB, the instance about 5.5 MB.

## What the anonymous part holds in Pico after Zone

With Zone's definition kept, the heap holds 49.5 MB live (mallinfo) in Pico.
Not kept, 25 MB live, yet anonymous PSS only falls 1.6 MB (73.7 MB): Zone's
freed definition, instance and catalog state leave the heap's pages partly
used under Pico's. Headless, without the 10 000-game state, dropping Zone's
views does return 29 MB. The gap is the main heap's fragmentation across a
surface switch.
