# korri-shell

The native shell for Korri surfaces (`docs/briefs/2026-10-31-native-surfaces-contract.md`).
It loads surface packages (`main.slint` + `logic.js`), runs their logic in
QuickJS, draws them with Slint's software renderer, and talks to the korrid on
its own device.

## Run

```
korri-shell --packages DIR [--cache DIR] [--surfaces ID,ID…] [--placement fullscreen|WxH] [--stats]
korri-shell install --packages DIR [--cache DIR]   # verify every package, write the index
korri-shell verify PACKAGE_DIR                     # one summary as JSON
```

Environment, as for `clients/linux`: `KORRID_ADDRESS` (127.0.0.1:PORT),
`KORRID_PORTAL_ORIGIN`, and `CREDENTIALS_DIRECTORY/KORRID_RPC_CAPABILITY`
(private to the user). The shell refuses to run as root and never writes the
capability or port to a file, URL or log. F2 or L1+R1 switches to the next surface.

Nix: `nix run .#shell-check` (fmt, clippy, tests), `nix run .#shell-run`,
`nix run .#shell-proof`, `nix build .#korri-shell`.

## Package

Zone's package (`packages/zone`, its README) is built from TypeScript:
`bun install && bun run zone:build` here (`package.json`); its gates run
with `bun test packages/zone/test`, and `tests/zone_package.rs` draws its
previews and walks its focus at five sizes.

`globalThis.surface = { displays?, preferences, init, update, view, overlay? }`.
`displays` is `"main"` (the default when absent) or `"all"`; the shell passes
it in `app.surface.declare` and korrid publishes `model.displays` only for
`"all"`. Zone declares `"main"`, Pico `"all"`, as the portal registry does.

## Focus plan

Contract 8.4: a surface groups its focus targets into regions and declares
how focus leaves each region. A package shows this in its view, in three
reserved keys that the shell never binds to Slint (so `.slint` does not
declare them):

| Key | Where | Value |
|---|---|---|
| `focusRegion` | every view entry that has `focused` | the target's region id, a non-empty string (for example `"tabs"`, `"band"`) |
| `focusActive` | optional, on a target | `true` on the region's active target (for example the active tab); at most one per region |
| `focusPlan` | top level of `view()` or `overlay()` | `{ "<region>": { "up" \| "down" \| "left" \| "right": { "to": "<region>", "entry": "remembered" \| "active" \| "first" } } }` |

The plan computes `focusPlan`, so the exits may differ by size. A direction a
region does not list has no exit: focus stays. The verifier fails a package
whose view has a focus target without `focusRegion`, more than one active
target in a region, a direction other than the four, an entry rule other
than the three, or an exit whose `to` region has no target in that view.

### Focus targets

Every element that draws a focus target sets three accessible properties
(also what a screen reader needs):

```slint
accessible-role: button;                 // or another role that fits
accessible-item-selectable: true;
accessible-item-selected: entry.focused; // true exactly while its view entry is focused
```

The shell finds which element draws which view entry by focusing each entry
alone and reading which element is selected; the target's box is that
element's geometry in the window (`src/targets.rs`). An entry whose element is
missing, or on a hidden layer (`visible: false` or opacity zero on it or an
ancestor), is not drawn. An element only clipped out of view (by a scrolling or
clipping ancestor, or the window) is drawn, at its real geometry, and not
visible: a press can scroll it in. A target's
identity is its region and its place among that region's entries in the view,
never its label, its values or its drawing order: labels repeat (two route
rows with the same buttons), values change in place (a choice row that left
and right step), and a list or band that recentres changes what is drawn but
not the view. The verifier fails a package, in the first view at 640x480 or
after one press of each button, whose focused entry is not drawn and visible, whose entry
selects more than one element, or that has an element selected while no entry
is focused.

`examples/surface-probe.rs walk` judges every d-pad press, at 320x240,
640x480, 480x800, 1280x720 and 1920x1080, with a display list larger than the
window:

- Inside the focused target's region: the target drawn nearest in the pressed
  direction (spatial-focus.ts: distance along the axis plus 3 x the distance
  across, ahead by more than 1 px), among the region's drawn targets, visible
  or clipped out of view by a scrolling ancestor; targets not drawn are no
  candidates.
- None drawn that way, along the region's drawn targets (a column for up and
  down, a row for left and right): the region's next entry in the view (down,
  right) or the previous one (up, left), when it is not drawn: a scroll brings
  it into view.
- After every press the focused entry is drawn and visible: the press scrolled
  it into view.
- None that way: the region's exit. `first` is the destination's target drawn
  first, top to bottom then left to right by box centre, also when every one of
  them is scrolled out of view; `active` is its
  `focusActive` target; `remembered` is the target that region last had
  focused along the walked path, or `first` if it never had focus.
- No exit: focus stays.

A target's drawn box and identity are its element's (Focus targets above).
The probe runs on its own clock with `animation-tick()` held still, so frames
it compares differ only by what the views changed; `--still` fails a state
whose unchanged view, applied again, changes a pixel.

## Wire

`POST /rpc` with `app.surface.declare`, `.preferences`, `.model`, `.request`;
the model watch is a root WebSocket (`Origin`, then `Bearer CAP`, then
`app.surface.watch`), one push per ack. Treaty values go through the
`korri-surface-treaty` crate (`contracts/surface/`).

Once a second the shell sends the active surface `{"_tag":"Tick","idleMs":N}`
(contract amendment A1, section 7.1): `N` is the time since the last `Pressed`
it delivered to that surface, or since the surface loaded. Each `Tick` is an
`update` call under the 50 ms budget. The verifier's sweep includes ticks.

Right after `init`, before the first `view()`, and on every window size or
scale-factor change, the shell sends `{"_tag":"Resized","width":W,"height":H}`
in logical pixels (amendment A2, section 8.1). The verifier sweeps 320x240,
640x480, 480x800, 1280x720 and 1920x1080, and runs once under a display list
larger than the window. `examples/surface-probe.rs walk` checks every d-pad
direction against the drawn geometry (section 8.3).

## Windows and placement

Only one shell window is mapped at a time: the Surface, or the Overlay during
the gameplay overlay. korrid places the one window owned by the processes of
the unit named in `KORRID_PORTAL_UNIT`, so the shell must run as that unit.
During a switch the new surface's window is mapped until its first frame, then
the old one is removed.

## Limits and performance

Each surface's logic runs under three limits (`src/engine.rs`):

| Constant | Value | Covers |
|---|---|---|
| `HEAP_LIMIT` | 64 MiB | the surface's QuickJS heap |
| `LOAD_BUDGET` | 2,000 ms | one load: evaluating `logic.js`, reading its declarations and first model, and `init`, together; once per load or switch |
| `CALL_BUDGET` | 50 ms | each `update`, `view` and `overlay` call, and each later model |

A violation unloads the package and restores the previous one.

The logic runs on a thread of its own (`src/logic.rs`). Presses, ticks and
models go to it as calls, one at a time in order; a `Tick` sent while the last
waiting call is a `Tick` replaces it. Each outcome (requests, and the views
when the state or model changed) is applied on the UI thread in the next
frame, so a slow `update` or `view()` never holds up a frame. The limits above
are enforced on that thread; unloading or switching interrupts a running call
and joins the thread. The verifier
records the slowest load as `loadMs` and the slowest call as `maxCallMs` in
each package's summary. Slint is patched (`vendor/README.md`) to cache
still layers. Numbers and screenshots: `evidence/`.

## Development tools

- `examples/fixture-korrid.rs`: the fixture korrid as a process.
  `--model FILE` serves another `SurfaceModel` (a Ready catalog's `games`
  become the window's games; a model's `fixtureGameActions` are published for
  the focused game), `--displays JSON` sets `model.displays`; on stdin
  `model FILE` and `displays JSON` change them while the shell runs.
- `examples/surface-probe.rs`: `previews DIR` renders every
  `*.preview.slint` state headless at given sizes and fails one that does not
  compile or draws nothing; `walk PACKAGE --model FILE` presses every button
  from every state reached and fails a state whose focused control is not
  drawn inside the window. Pico's tests use both (`packages/pico/README.md`).

## Backends

`winit` (desktop, default), `headless` (tests), `sdl` (webOS, through
`window-sdl/`, built with `--no-default-features --features webos`).
