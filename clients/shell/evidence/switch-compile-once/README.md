# Switches with each package compiled once per process

`nix run .#shell-proof -- switch OUT` (Xvfb 640x480, fixture korrid, 12 games,
20 F2 switches Zone ↔ Pico), workstation, 2026-11-05. `old` is the same tree
with `src/shell.rs` compiling `main.slint` at every load; `new` keeps each
package's compiled views for the life of the process.

| | old | new |
| --- | --- | --- |
| Zone compile_ms, first load | 394 | 369 |
| Zone compile_ms, later loads | 321-362 | 0.001-0.002 |
| Zone first_frame_ms, later loads | 355-393 | 31-66 |
| Pico first_frame_ms, later loads | 141-158 | 25-42 |
| Zone PSS after switch 2 / 20 (KiB) | 115 542 / 120 496 | 108 668 / 108 548 |
| Pico PSS after switch 1 / 19 (KiB) | 100 887 / 111 209 | 102 729 / 104 389 |

Keeping both definitions costs less than compiling again: each compile left
the heap fragmented, and the old process grew 5 MiB over the first switches.

Where Zone's compile goes (perf, workstation, 3 compiles): the compiler's
passes 51 % (binding analysis 12 %, move_declarations 9 %, const propagation
5 %), lowering to the interpreter's item tree 23 %, loading and parsing 7 %,
dropping the compiler's trees 4 %. Nothing is debug-built, no font or image is
loaded, and no single pass dominates: Zone is 12 300 lines of `.slint`. The
RG353M takes 7-9 times as long (2.6-3.6 s). Its first load in a process still
pays that; later loads do not.
