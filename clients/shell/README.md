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

Environment: `KORRID_ADDRESS` (127.0.0.1:PORT), `KORRID_SHELL_ORIGIN` (the
Origin korrid accepts from its surface host), and
`CREDENTIALS_DIRECTORY/KORRID_RPC_CAPABILITY` (private to the user). The shell
refuses to run as root and never writes the capability or port to a file, URL
or log. F2 or L1+R1 switches to the next surface.

Nix: `nix run .#shell-check` (fmt, clippy, tests), `nix run .#shell-run`,
`nix run .#shell-proof`, `nix build .#korri-shell`.

The LG B7 sample-data IPK and workstation SDL proof are described in
[`webos/README.md`](webos/README.md).

## Device migration: the `korri-shell` user (2026-11)

The shell runs as the system user `korri-shell`, with its state in
`/var/lib/korri-shell` and `/run/korri-shell`. Devices installed before this
change ran it as `korri-portal`. Korri reads no old name. Each such device
needs one run of `migrate-shell-user.sh`, in this order:

1. Update the device to a system generation that contains this change, with
   the device's normal update path. The switch starts `korri-shell.service` as
   `korri-shell` with a new, empty home. NixOS normally deletes the old
   account at the switch; its files keep the old numeric owner.
2. Copy `migrate-shell-user.sh` to the device and run it as root. It stops
   the shell, moves `/var/lib/korri-portal` to `/var/lib/korri-shell` (the
   empty home goes to `/var/lib/korri-shell.before-migration`), gives every
   file the old account owned on the root filesystem to `korri-shell`,
   removes `/run/korri-portal`, deletes the old user and group if they still
   exist, and starts the shell again. A second run finds nothing to do.
3. Check: `systemctl show korri-shell -p User` prints `User=korri-shell`,
   `getent passwd korri-portal` prints nothing, and the screen shows the shell.
   Then remove `/var/lib/korri-shell.before-migration`.

A device installed after this change needs no migration.

## Package

Zone's package (`packages/zone`, its README) is built from TypeScript:
`bun install && bun run zone:build` here (`package.json`); its gates run
with `bun test packages/zone/test`, and `tests/zone_package.rs` draws its
previews and walks its focus at five sizes.

`globalThis.surface = { displays?, preferences, init, update, view, overlay? }`.
`displays` is `"main"` (the default when absent) or `"all"`; the shell passes
it in `app.surface.declare` and korrid publishes `model.displays` only for
`"all"`. Zone declares `"main"`, Pico `"all"`.

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
  it into view. From a focused state, a direction leaves exactly one focused,
  visible target: a press that clears every `focused` flag, or sets two, fails.
- None that way: the region's exit. `first` is the destination's target drawn
  first, top to bottom then left to right by box centre, also when every one of
  them is scrolled out of view; `active` is its
  `focusActive` target; `remembered` is the target that region last had
  focused along the walked path, or `first` if it never had focus.
- No exit: focus stays.

A target's drawn box and identity are its element's (Focus targets above).
The probe runs on its own clock, starting at the same instant every run, with
`animation-tick()` held still, so frames it compares differ only by what the
views changed and two runs draw the same pixels; `--still` fails a state
whose unchanged view, applied again, changes a pixel.

## Wire

`POST /rpc` with `app.surface.declare`, `.preferences`, `.model`, `.request`;
the model watch is a root WebSocket (`Origin`, then `Bearer CAP`, then
`app.surface.watch`), one push per ack. Treaty values go through the
`korri-surface-treaty` crate (`contracts/surface/`).

Once a second the shell sends the active surface `{"_tag":"Tick","idleMs":N}`
(contract amendment A1, section 7.1): `N` is the time since the last `Pressed`
it delivered to that surface, or since the surface loaded. Each `Tick` is an
`update` call measured against the 50 ms budget. The verifier's sweep includes ticks.

Right after `init`, before the first `view()`, and on every window size or
scale-factor change, the shell sends `{"_tag":"Resized","width":W,"height":H}`
in logical pixels (amendment A2, section 8.1). The verifier sweeps 320x240,
640x480, 480x800, 1280x720 and 1920x1080, and runs once under a display list
larger than the window. `examples/surface-probe.rs walk` checks every d-pad
direction against the drawn geometry (section 8.3).

## Windows and placement

Only one shell window is mapped at a time: the Surface, or the Overlay during
the gameplay overlay. korrid places the one window owned by the processes of
the unit named in `KORRID_SHELL_UNIT`, so the shell must run as that unit.
During a switch the new surface's window is mapped until its first frame, then
the old one is removed.

## Limits and performance

The shell compiles a package's `main.slint` the first time the package loads
and keeps the compiled views until it exits; a later switch back creates the
surface from them. Zone's compile is most of its load: 0.4 s on a workstation,
2.6 to 3.6 s on the RG353M.

The shipped binary is Cargo's `dist` profile (`package.nix`): link-time
optimised in one unit, the Slint compiler optimised for size. Its code is
mapped into the shell's memory: `.text` 15.7 MB, against 20.1 MB in
`release`. Measure memory with a `dist` build copied out of the target
directory (a freshly linked file maps differently).

Each surface's logic runs under three limits (`src/engine.rs`):

| Constant | Value | Covers |
|---|---|---|
| `HEAP_LIMIT` | 64 MiB | the surface's QuickJS heap |
| `LOAD_BUDGET` | 2,000 ms | one load: evaluating `logic.js`, reading its declarations and first model, and `init`, together; once per load or switch |
| `CALL_BUDGET` | 50 ms | each `update`, `view` and `overlay` call, and each later model: the performance budget, not a limit |
| `STUCK_LIMIT` | 2,000 ms | each call after the load: a call still running then is interrupted |

Build verification, tests and bench logic budgets use the QuickJS thread's
CPU time (`Limits.clock`, #368). Device runtime keeps wall time, the default.
No environment variable selects the budget clock. The limits are unchanged;
CPU budgets do not catch time spent waiting for locks or I/O.

Contract amendment A5 (owner, 2026-11-06) decides what happens at run time:

- A call over `CALL_BUDGET` is a performance defect. The shell logs
  `performance defect: <id> <call> for <message> took N ms, …` once per
  surface and message type, then at most one summary line per minute
  (`SLOW_LOG_INTERVAL`). The surface keeps running. Each surface's timing
  tests guard the budget (Zone's in `tests/zone_package.rs`).
- A call still running at `STUCK_LIMIT`, a throw, a broken contract (a wrong
  shape, a refused request or view), or the heap limit unloads the surface.
- After an unload the shell never switches on its own. It shows its own
  notice (`src/notice.rs`, `src/notice.slint`): the surface and the reason in
  plain words ("Pico stopped responding"), and the actions "Try Pico again"
  and "Use Zone" (the previous surface, else the first other package). Up and
  down move the focus, confirm takes the focused action, back takes the other
  surface; L1+R1 does nothing while the notice waits. With no other package,
  the notice offers "try again" only. A choice that does not load shows the
  notice again ("Zone could not start"). `tests/surface_notice.rs` draws the
  notice at the five sizes and fails if anything is squeezed or clipped.

The logic runs on a thread of its own (`src/logic.rs`). Presses, ticks and
models go to it as calls, one at a time in order; a `Tick` sent while the last
waiting call is a `Tick` replaces it. Each outcome (requests, and the views
when the state or model changed) is applied on the UI thread in the next
frame, so a slow `update` or `view()` never holds up a frame. The limits above
are enforced on that thread, and each outcome carries its slowest engine call
for the performance-defect log; unloading or switching interrupts a running
call and joins the thread. The verifier
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
  `overlay PACKAGE --model FILE --backdrops IMG,…` draws the package's
  Overlay with its alpha kept over each picture (a game frame), and over
  black, as a window without alpha shows it.

## Backends

`winit` (desktop, default), `headless` (tests), `sdl` (webOS, through
`window-sdl/`, built with `--no-default-features --features webos`).
