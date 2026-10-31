# THROWAWAY BENCHMARK: surface logic as TypeScript in QuickJS

## The question

If a surface plugin keeps its Elm `update` in TypeScript and the Rust shell
runs it in QuickJS (korrid's engine, rquickjs 0.14), how much performance is
lost on the RG35XX SP?

## What runs

`harness.ts` imports Zone's real, unchanged `update`, `zoneShown` and
`zoneSurfaceStart` (`surfaces/zone/src/state/surface.ts`) and Zone's own
fixtures. bun bundles it into `zone-logic.js` (one IIFE, 24 modules,
119 KB). The Rust host (`src/main.rs`) loads that bundle into a fresh
QuickJS runtime, supplies a microsecond clock, and calls `runBench`.

Cases: a navigation script of Zone's own messages (FocusedCard across the
band, ChoseCard, FocusedAction, Back, Menu, Back), with and without a JSON
read-back of what the shell would need; `KorriPublished` (every model settled
against Korri's model); the model handed in as JSON text; with the 7-game
fixture and with a 500-game library (513 KB of JSON).

## Result on the SP (2026-10-31)

Three runs of 20,000 steps, from a `/tmp` bundle, the Chromium kiosk still
running (load average 1.3). Medians of the three runs, in microseconds:

| Case | p50 | p99 | max |
| --- | ---: | ---: | ---: |
| One input through `update` (7 games) | 116 | 179 | 1652 |
| One input + JSON read-back | 207 | 286 | 420 |
| One input (500 games) | 116 | 185 | 294 |
| `KorriPublished` (7 or 500 games) | 355 | 446 | 538 |
| Surface start | 374 | 617 | 617 |
| Model JSON in, 7 games (12.5 KB) | 283 | 359 | 449 |
| Model JSON in, 500 games (513 KB) | 10717 | 11260 | 14294 |
| Same 513 KB parse in native Rust (serde_json) | 4131 (p50) | | |

- One frame at 60 fps is 16,667 µs. One input costs about 1 % of a frame
  at p99 (2 % with the read-back).
- Load, once per surface start or switch: runtime and context 1.7 ms;
  evaluating the 119 KB bundle 40 ms.
- Memory: PSS 2.2 MiB before the runtime, 6.7 MiB after loading Zone's logic
  and running every case.
- Reference on the x86 workstation: the same input case is 1.2 µs p50 in bun
  (a JIT) and 15.7 µs in QuickJS, about 13x slower. Native Rust was not
  measured; Zone's `update` has no Rust port.

Raw output: `evidence-sp/run-{1,2,3}.json`.

## Rebuild and rerun

```sh
bun build harness.ts --format=iife --target=browser --outfile=zone-logic.js
nix build --impure --no-link --print-out-paths --expr 'let pkgs = import (builtins.getFlake (toString ./.)).inputs.nixpkgs { system = "aarch64-linux"; }; in pkgs.callPackage ./clients/zone-native-spike/logic-bench/package.nix {}'
clients/zone-native-spike/logic-bench/run-on-sp.sh <store path> <output dir> 20000
```

## Limits

- Zone's messages assume the host moves focus (`FocusedCard index`). A native
  surface would compute directional focus in `update`; that work is not in
  these numbers.
- Layout plans (`src/plan/`) and transpiling TypeScript at load (oxc) were
  not timed.
- The kiosk ran during the test, so these numbers are conservative.
