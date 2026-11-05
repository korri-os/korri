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

## (b) 10 000 games: 60 fps met; PSS under 80 MiB met for Zone idle only

PSS in KiB, `large` scenario (Zone idle, Zone after 30 moves, then F2 to Pico
and 30 moves; Zone's definition stays compiled while Pico shows):

| step | Zone idle | Zone after moves | Pico after moves | file |
| --- | --- | --- | --- | --- |
| 0. before | 94 613 | 100 263 | 110 406 | `large-10000-step0.txt` |
| 1. lowered unit shrunk to fit | 91 078 | 95 617 | 104 548 | `large-10000-step1-shrink.txt` |
| 2. + compiler-only tables dropped, compiler code released | 80 944 | 85 797 | 93 015 | `large-10000.txt` |

fps at step 2: 60.8-62.4 in every 5 s window (`large-10000-fps.txt`).

Step 2, by part (smaps PSS, Zone idle): heap 51.3 MB (was 56.3), the binary's
mapped code and data 15.5 MB (was 21.0), anonymous maps 10.0 MB, libraries
3.5 MB.

- Compiler-only tables: a definition that keeps no type loaders drops each
  property's analysis (`prop_analysis`, read only by the code generators) and
  the items' source locations (`debug_info`, read only by highlighting).
  `vendor/README.md`.
- Compiler code released: after a compile, `memory::release_code` drops the
  resident pages of the binary's mappings that the program never wrote
  (`MADV_DONTNEED`); the code the shell runs afterwards is read back from the
  page cache. About 13-15 MB of the binary stays resident while the shell
  runs: that is its working set.

## Tried and not kept

| | Zone, compiled and created (headless RSS) | why not kept |
| --- | --- | --- |
| compile on a thread, lower on another (own malloc arenas) | 73.0 → 66.9 MB headless; real shell no change (91.9 vs 91.1 MB) | the instance had been reusing the freed compiler pages |
| the above, then copy the unit into the main arena | 73.1 MB | worse: what the copy leaves behind keeps the arena's pages |
| lowering twice, keep the second | no change | |
| jemalloc | 123 MB | returns memory only after a delay |
| mimalloc (purge delay 0) | 80.7 MB after compile | worse than glibc (67.8 MB) |

Where Zone's heap still goes (heaptrack, compiled and created, live 31.1 MB):
bindings (`property_init`, 376 bytes each, 18 580) 9.6 MB, item geometries
5.3 MB (each a struct literal held as a `BTreeMap` leaf of 11 slots for 4
fields), other lowered expressions 7.4 MB, the instance about 5.5 MB.
