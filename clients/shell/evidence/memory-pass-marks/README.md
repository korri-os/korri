# Contract section 4 memory pass marks (workstation, 2026-11-05)

`proof/run.sh` under Xvfb 640x480 over the fixture korrid, release build of
native-shell-fix-5 (each package compiled once per process; the lowered unit
shrunk to fit).

## (a) Memory flat over 20 Zone ↔ Pico switches: met

`switch-20.csv`, PSS after each switch (KiB). The first visit of each package
adds its compiled definition; after that:

| | first repeat | last | per visit |
| --- | --- | --- | --- |
| Zone (switches 2 → 20) | 102 713 | 102 910 | +22 KiB |
| Pico (switches 3 → 19) | 97 352 | 97 637 | +36 KiB |

`tests/memory_probe.rs` bounds a round at 48 KiB.

## (b) 10 000 games: 60 fps met, PSS under 80 MiB (81 920 KiB) NOT met

`large-10000.txt`, `large-10000-fps.txt`:

| | PSS (KiB) | before the shrink | fps |
| --- | --- | --- | --- |
| Zone idle | 91 078 | 94 613 | 59.6-62.4 |
| Zone after 30 moves | 95 617 | 100 263 | |
| Pico after 30 moves (Zone's definition kept) | 104 548 | 110 406 | |

Where Zone's 91 MB goes (smaps, PSS): heap 56.3 MB, the binary's mapped
code 21.0 MB (`.text` is 20.1 MB), anonymous maps 10.0 MB (frame buffers,
thread stacks, QuickJS), libraries 3.0 MB.

The heap is mostly Zone's compiled definition. After a compile and
`malloc_trim` (headless, Zone alone): 30.8 MB in use, 67.8 MB resident. The
interpreter's lowered unit is the live part; the rest is pages that the
compiler's freed trees left partly used. The contract's 80 MiB mark was met
when Zone was smaller (`../large/`: 49-63 MB); Zone's `.slint` is now
12 300 lines and its definition about 3 times larger.

Tried and not kept: lowering twice so the kept unit lands past the holes (no
change), jemalloc (123 MB resident: it returns memory only after a delay).
