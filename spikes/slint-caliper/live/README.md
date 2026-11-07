# Spike `live`: a surface package live, with live reload

THROWAWAY SPIKE. One of four `slint-caliper` spikes that test whether
Caliper's affordances work for Korri's native Slint surface packages.

**Question.** Can Zone and Pico run with their real logic on fixture data, at a
device's size, and reload a `.slint` or `logic.ts` save without a restart and
without losing the logic's state?

**Answer.** Yes. A `.slint` save shows in about 0.9 s for Zone (0.35 s for
Pico), and the logic is not touched. A `logic.ts` save shows in about 0.15 s.
The state survives in `replay` and `transfer` modes, and no change to
`clients/shell` is needed. The 2026-10-30 analysis said a logic change needs a
Rust rebuild. That is no longer true.

## Run it

```sh
nix build .#slint-caliper-live            # or: nix run .#slint-caliper-live -- ARGS
korri-live [--surface zone|pico] [--device rg35xxsp|rg353m|odin2portal|rpminiv2|r36tmax|rgds] \
           [--mode replay|transfer|fresh] [--scenario NAME] [--packages DIR]
korri-live-proof OUT_DIR                  # inside a Korri checkout: the Xvfb proof
```

- Inside a Korri checkout, `korri-live` watches and edits that checkout's
  `clients/shell/packages`. Anywhere else, it copies the packages it was built
  with to `${XDG_STATE_HOME:-~/.local/state}/korri-live/<build>/` and prints
  the path to edit.
- Two windows open: the package, at the device's CSS size, and the bench.
- Keys in the package window are the shell's own map
  (`clients/shell/src/backend/winit.rs`, `src/input/mod.rs`): arrows move,
  Enter is confirm, Esc or Backspace is back, M menu, O options, S system,
  F2 the next package.
- The bench picks the package, the device, the logic mode and the fixture
  model. It lists the last three reloads with their times.

## How it works

`src/main.rs` uses the shell's own pieces: `package::Package::at`,
`package::compile`, `logic::Logic` (QuickJS on its own thread),
`view::Binder`, `korrid::Korrid`, `input::Router` and the winit backend.
`fixture-korrid` is `clients/shell`'s fixture korrid, started as a child
process. It writes the RPC capability into a private 0700 credentials
directory, and the spike reads it through `Connection::from_environment`. No
argument, URL or log carries the capability.

- **Fixture models.** The picker lists `clients/shell/fixtures/model.json`,
  then Zone's `preview/scenarios/*.json` or the output of Pico's
  `preview/scenarios.ts`. Choosing one writes `model FILE` to the fixture's
  stdin; korrid publishes it and the running logic gets it as a model update.
- **`.slint` save.** `package::compile` again, then a new instance of the
  Surface (or Overlay) in the same window, with the last view applied whole.
  The logic and its state are not touched. A compile error keeps the old
  views and is listed as refused.
- **`.ts` save.** `bun run zone:build` (or `pico:build`) in `clients/shell`,
  the package's own transpile step, then a new `Logic` from the new
  `logic.js`, in one of three modes:
  - `replay`: the new engine gets the first model, `init`, and then every
    message and model since the load, in order. The requests it returns are
    dropped, because korrid already did them.
  - `transfer`: the old engine's state, as JSON, becomes the new engine's
    `init` result. A spike-only wrapper around `logic.js` adds this
    (`shim` in `main.rs`); the shell's engine is unchanged.
  - `fresh`: `init` on the current model. The state is lost.
  A transpile error keeps the old logic and is listed as failed.
- **Timing.** In the process: from the saved file's mtime to the first
  `RedrawRequested` of the new view. From outside: `probe.py` reads the
  window's pixels through `xwd` and reports the time from the mtime to the
  first sample of the final pixels.

## Results (proof run on zao, `evidence/`)

Built with `nix build .#slint-caliper-live`, run with `korri-live-proof`
under Xvfb. The load average on zao was 11 to 33 on 16 cores during the run,
from other work.

| Save | Samples | Save to frame, median (range) | Parts, median | Pixels from outside, median (range) |
|---|---|---|---|---|
| Zone `.slint` (`ui/tokens.slint`) | 13 | 943 ms (698–1140) | compile 860 ms, swap 36 ms | 965 ms (724–1147), 12 samples |
| Zone `logic.ts` | 19 | 144 ms (83–199) | bun build 27 ms, reload 29 ms | 148 ms (86–204), 12 samples |
| Pico `.slint` (`ui/palette.slint`) | 2 | 326, 420 ms | compile 249–341 ms | 424 ms |
| Pico `logic.ts` | 3 | 112–149 ms | bun build 18–34 ms | 112, 151 ms |

- The two measures agree to within 10 ms where Zone's animations did not
  move the probed region. Three probe samples are marked as moved by an
  animation (`first_change_ms` differs from `save_to_pixels_ms` in
  `probes.txt`) and are left out of the table.
- Replay grows with the journal: 40 calls replay in about 25 ms, 617 calls in
  63–71 ms. The idle tick adds one call per second, so an hour adds about
  0.3 s.
- An earlier run on the same machine under heavier load (a parallel Rust
  build) gave Zone `.slint` 1.0–3.7 s. The Slint compile is the cost, and it
  scales with CPU.

### State after each save

| Save | Mode | Focus before → after | View values that differ | State survived |
|---|---|---|---|---|
| Zone `.slint` | (logic untouched) | `.cardMarks[3]` kept | only colours | yes |
| Zone `logic.ts` | replay | `.cardMarks[2]` → `.cardMarks[2]` | 3: the three edited labels | yes |
| Zone `logic.ts` | transfer | `.cardMarks[4]` → `.cardMarks[4]` | 3: the three edited labels | yes |
| Zone `logic.ts` | fresh | `.cardMarks[5]` → `.cardMarks[0]` | 25 | no (as designed) |
| Zone `logic.ts`, 617 calls | replay | `.cardMarks[1]` kept | 3 | yes |
| Pico `.slint` | (logic untouched) | `.home.carts[4]` kept | only colours | yes |
| Pico `logic.ts` | replay | `.home.carts[5]` kept | 1: `fontFamily` | yes |
| Pico `logic.ts` | transfer | `.home.carts[7]` kept | 1: `fontFamily` | yes |

A `.slint` save resets only state that lives inside the Slint instance
(running animations, a Flickable's scroll offset). Zone and Pico keep focus
and layers in their logic, so nothing they show is lost.

## Affordances against Caliper

| Caliper affordance | Here | Evidence |
|---|---|---|
| A device's CSS viewport | Works. The window is the device's CSS size: 640×480 for rg35xxsp, 1920×1080 for odin2portal, switched while running. | `zone-16-odin2portal.png` (1920×1080), `zone-17-rg35xxsp.png` |
| True physical size (calibrated px per mm) | Does not work. Not built. Slint can scale a window, so it is possible. | — |
| Scenarios: product-owned fixture data with real action handlers | Works. Zone's and Pico's own fixture models, switched live through fixture-korrid; the real `update` handles every press and its requests go to the fixture korrid. | `zone-0-shell-fixture.png`, `zone-1a-games-empty.png`, `zone-1-games-model.png`, `bench-0-empty-picked.png` |
| Interaction inside the frame | Works. The shell's key map and button repeat; three presses move focus, Options opens a layer, Back closes it. | `zone-2..6`, `pico-2..6` |
| A style save updates the frame | Works, state kept. About 0.9 s for Zone, because Slint compiles the whole package again. | `zone-7-slint-edit.png`, `zone-8-after-slint-press.png`, `pico-7-slint-edit.png` |
| A code save reloads the frame | Works, and keeps the state, which Caliper does not (it reloads the frame). | `zone-9`, `zone-10`, `zone-14-logic-edit-transfer.png`, `pico-9`, `pico-12` |
| Every failure is visible | Partly works. A broken `.slint` or `logic.ts` is refused and the package keeps running, but the reason shows in the bench and the log, not in the package window. | `zone-18-broken-slint.png`, `zone-19-broken-logic.png`, `zone-20-after-broken-press.png`, `korri-live.log` |
| Scenario steps | Partly works. The bench's "Play its N steps" button replays a scenario's `steps`; the proof does not use it. | — |
| Code pane, knobs, takes, checks, agent | Not in this spike (the other `slint-caliper` spikes). | — |

## Files

- `src/main.rs`: the tool. `src/bin/fixture-korrid.rs`: the shell's fixture
  korrid. `ui/tool.slint`: the bench.
- `proof.sh`: the Xvfb proof. It edits Zone's `ui/tokens.slint` and
  `logic.ts` and Pico's `ui/palette.slint` and `logic.ts`, and restores them
  and both `logic.js` byte for byte, also on failure.
- `probe.py`: the outside timer. `package.nix`: the Nix output.
- `evidence/`: the last proof run: screenshots, `korri-live.log`,
  `reloads.txt` (`LIVE_RELOAD` lines), `probes.txt`, `marks.txt`.
