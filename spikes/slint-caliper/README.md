# Slint Caliper spikes (throwaway)

Four spikes test whether Caliper's affordances
(`github.com/simonwjackson/caliper`, 0.1.1) can work for Korri's native Slint
surface packages, Pico and Zone. They live on branch `spike/slint-caliper` and
never land on `main`. Each folder has its own README and evidence:

| Folder | Nix output | Question |
|---|---|---|
| `viewer/` (includes `tune`) | `slint-caliper-viewer` | Part list, states, true-size device frames, reload on save, knobs, takes |
| `live/` | `slint-caliper-live` (`korri-live`) | The whole package with its real logic and fixture data, with live reload of `.slint` and `logic.ts` |
| `render/` | `slint-caliper-render` | `caliper-render` for agents: PNGs, a JSON verdict, checks |

The user first ran them on myoko on 2026-11-06: Hyprland 0.56.2 (Lua config),
two 2880x1800 panels at scale 1.5. Unless a row says myoko, the times come from
zao under load.

## Findings

| Caliper affordance | Caliper's behaviour | Spike result | Evidence | To make it production-ready |
|---|---|---|---|---|
| Part discovery and layers | `*.part.tsx`, with a layer suffix or a `layer` export, in Pages → Atoms → Unclassified order | **Works**: 155 parts from `*.<layer>.preview.slint`, in Caliper's order. Slint has no `layer`/`name` exports and no Git-ignore rule | `viewer/README.md`, `viewer/evidence/unclassified-layer-25pct.png` | Decide where part metadata lives (file name or comment) |
| States side by side, or one alone | Each exported component is a state, and each state has its own frame | **Works**, tested with real clicks | `viewer/evidence/x11-click-*.png` | — |
| A broken state fails alone | One frame throws, the others render | **Works**: a file with errors is compiled one state at a time | `viewer/evidence/reload-pico-3-short-fails-alone-50pct.png` | — |
| Device list with CSS size and mm | `devices` option, Caliper's built-ins | **Partly works**: rg35xxsp, rg35xxpro, rg353m and odin2portal are grounded. rpminiv2's mm is unresolved | `viewer/evidence/device-sources.md` | An agreed Korri device table with px and mm for each device |
| True physical size | CSS px scaled by the calibrated px/mm | **Works, by arithmetic**: RG353M 72.01 mm against 72 mm, RG35XX Pro 71.07–71.19 mm against 71.12 mm, at EDID px/mm. Each frame is drawn by the device's software renderer at device pixels | `viewer/evidence/measure.txt` | Ruler check on myoko (the user approved without notes), and a test when the window moves to another monitor |
| Calibration | Match an on-screen object to a ruler, then save | **Works**: a 100 mm bar, a card outline and a slider. Where it is saved is unresolved. After Reset the slider does not move back | `viewer/evidence/x11-calibrate-50pct.png` | Decide where the calibration is stored |
| Reload on save | Vite HMR reloads the frame | **Partly works**: Pico 101–187 ms. Zone 0.74–1.31 s, because Slint compiles the whole package again | `viewer/evidence/reload-*.log` | Incremental Slint compile, or compile off the UI thread |
| Scenarios with fixture data | The part's own state and real handlers | **Works**: the real package (`update`, `view`) on fixture-korrid. Models switch while it runs | `live/evidence/zone-*.png`, `bench-*.png` | — |
| Interaction in the frame | Click and type in the iframe | **Works** in `live` (the shell's key map, layers, Back). **Does not work** in the viewer, where frames are pictures | `live/evidence/zone-1..6*.png` | Route input into viewer frames, or embed `live` in the viewer |
| Live logic reload | Vite reloads the module and the frame loses its state | **Works, better than Caliper**: a `logic.ts` save is on screen in a median of 144 ms (Zone) and keeps the state in `replay` and `transfer` modes. The 2026-10-30 "Rust logic" gap is gone, because logic is QuickJS | `live/README.md`, `live/evidence/reloads.txt` | Optional: export and import of state in `engine.rs`'s `BOOTSTRAP` |
| Device viewport for the live package | The iframe has the device's CSS size | **Partly works**: correct under Xvfb. On myoko the user's Hyprland config makes the window fullscreen at 1920x1162, and `hyprctl dispatch` could not undo it, so the package laid out at the window's size | myoko screenshot, this run | Draw the package into a fixed-size frame inside the tool, not a separate top-level window |
| Knobs | Literal CSS custom properties and `@container` thresholds. A change is live, Save writes the source, computed values are never knobs | **Works**: Zone `Tokens`, Pico `P8` and `Tokens`. One frame updates in a median of 12 ms (12 frames: 166 ms). Save writes the literal back. Colour is a text field plus swatch only, with no picker. The list shows every token the part imports, not only the ones it reads | `viewer/evidence/tune-knob-*.png`, `knob-save.diff.txt`, `knob-latency.txt` | Colour picker, a list of only the tokens the part reads, "make a literal a token", knobs that write into a take |
| Takes | Edited copies under `.caliper/takes/`, Accept and Discard | **Works** for a take made by hand: it compiles from its copies (through a file loader overlay) and shows beside the original | `viewer/evidence/tune-take-beside-original-*.png`, `take-accept.diff.txt` | Agent-made takes. Decide whether the take root is `packages/.caliper/` or the repo root |
| Workspaces (answer a question) | Rows that check | **Not spiked** | — | Same overlay mechanism as takes |
| Code editing in the app | An editor in the Caliper app | **Not spiked**. You edit files in your own editor, and reload picks the change up | — | — |
| Agent render CLI | `caliper-render`: PNGs and a JSON verdict | **Works**: 540 results (155 parts, 270 states, 2 devices) and 1,080 PNGs in 86.8 s. The output is in Caliper's shape, plus a native `focus` check | `render/EVIDENCE.md`, `render/PARTS.md` | Update the `caliper-render` skill for the extra enum value |
| Compile and empty-draw checks | Errors and blank frames | **Works** | `render/EVIDENCE.md` | — |
| Spill | Content outside the frame | **Works**: a deliberate spill was caught. 86 existing findings await product review | `render/EVIDENCE.md` | Review the 86 findings |
| Focus visibility | — (native addition) | **Partly works**: passes with `--view FILE` on a real Zone view. Preview files report "Inconclusive", because they carry no JSON view entries | `render/EVIDENCE.md` | Run scenarios to get real views |
| Accessibility audit | axe | **Gap**: axe has no Slint equivalent. The two native rules (role and label on focusable elements, one focus at most) work, and 24 Zone failures exist. All 67 axe rules are listed as gaps | `render/axe-gaps.txt` | Grow native rules on the accessible tree |
| Approved baselines | Approve images, diff later renders | **Works** (approve, match, change, reject a tampered baseline) | `render/EVIDENCE.md` | Choose where baselines are kept |

## User notes from myoko

> The user approved the "Look on myoko" stop with no written notes (2026-11-06).

## Gaps

- **axe audit:** no Slint equivalent. Only the two native rules exist.
- **Focus coverage on preview files:** reported as Inconclusive, because previews carry no view entries.
- **Interaction inside viewer frames:** frames are pictures.
- **Zone reload time:** 0.74–1.31 s, from Slint's full recompile.
- **Live package viewport on Hyprland:** the window is made fullscreen, so the device size is not kept.
- **Viewer redraw stall:** under headless sway the window sometimes stopped redrawing. A frame request every 16 ms works around it. The cause is unknown (the vendored winit frame throttle or Slint 1.18.1 on Wayland).
- **Knobs:** no colour picker, no "make a literal a token", no knobs on take copies, and the list is not limited to the tokens a part reads.
- **Takes:** agent-made takes are not built.
- **Workspaces and in-app code editing:** not spiked.

## Unresolved data

- **rpminiv2 screen size:** goretroid.com refused every request. 75 x 65 mm comes from Korri's old Caliper config.
- **Calibration file:** `$XDG_STATE_HOME/korri-slint-caliper/px-per-mm`. The location and format are the spike's own.
- **Take root:** `clients/shell/packages/.caliper/` was chosen; the repo root would also fit.
- **Alias name for re-exported globals:** `Tune<n><Global>` is the viewer's own invention.
- **Baseline folder:** `render` takes `--baselines DIR`, and no product location is chosen.

## Changes outside `spikes/`

- **`flake.nix`:** seven lines, the three spike outputs and the viewer's import. Nothing else.
- **`pub` changes in `clients/shell/src`:** none.

## Verification on the merged branch

- `nix build .#slint-caliper-viewer .#slint-caliper-live .#slint-caliper-render`: passes.
- `nix build .#korri-shell` (the shell's full test suite): one failure, the
  `pico_package::calls_stay_well_inside_the_budget_on_a_slow_clock` perf guard,
  at 27.7 ms against a 26 ms limit and 12.4 ms against 10 ms, under load 13–20.
  - **Not caused by the spikes:** the shell derivation is identical to `main`'s
    (`g0nawhjshbk8g44miz5hbl9kabixdi03-korri-shell-0.0.0.drv`).
  - **Accepted:** issue #368 records perf-guard failures under heavy load as
    accepted (rerun on a quiet host).
