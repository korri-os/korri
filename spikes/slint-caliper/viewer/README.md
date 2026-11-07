# THROWAWAY SPIKE: viewer

Question: can a native Slint app give Caliper's part list, states and true-size device
frames for the Slint surface packages (Pico, Zone), with reload on save?

Answer: yes, with two limits. A Zone page reloads in 0.75 to 1.3 s, not under 1 s. The
millimetres are only as true as the monitor's px per mm, so you must calibrate.

## Run

```sh
nix run .#slint-caliper-viewer -- [--packages DIR] [--devices ID,ID] [--part PATH-SUFFIX] [--state NAME]
nix build .#slint-caliper-viewer            # then nix copy --to ssh://myoko <out>
```

The viewer finds `clients/shell/packages` above the working directory. Run it inside a
Korri checkout, so that it reloads the files you edit. `--packages`, or `KORRI_PACKAGES`,
names another folder. With no checkout, it shows a read-only copy that the Nix package
contains. Devices: `rg35xxsp` (the default, because Caliper makes the first device the
default), `rg35xxpro`, `rpminiv2`, `rg353m`, `odin2portal`.

Development: `cargo build --release` here, with the shell's toolchain
(`nix shell --inputs-from . nixpkgs#cargo nixpkgs#rustc nixpkgs#gcc`).

## How it works

- **Discovery** (`src/parts.rs`) uses Caliper's rule (`docs/guide.md`): each
  `*.<layer>.preview.slint` file is a part. The layer is the suffix. The list shows Pages,
  Templates, Organisms, Molecules, Atoms, then Unclassified, in path order inside each layer.
- **States.** Each `export component` is one state, and its label comes from its name
  ("NoResults" becomes "No results"). The viewer compiles the file once with
  slint-interpreter. If the file has errors, it compiles each state alone, with the other
  states' blocks blanked. Thus one broken state fails alone and shows its diagnostic.
- **Frames.** The viewer creates each state on its own `MinimalSoftwareWindow` at the
  device's pixel size. The shell's software renderer draws it into an RGB image. The
  viewer's window shows that image at `width-mm x px-per-mm` physical pixels (Slint's
  `phx`). The height follows the CSS aspect, as in Caliper's `frameGeometry`. When the
  frames do not fit the stage width, they all shrink together and the header says
  "Scaled to N%". The frames wrap into columns.
- **px per mm** (`src/monitor.rs`) uses the first source that exists:
  1. The saved calibration for the monitor.
  2. The monitor's EDID image size (`/sys/class/drm/*-<connector>/edid`; the winit
     monitor name is the Wayland output name, for example `eDP-1`).
  3. `KORRI_VIEWER_MONITOR_MM=WxH`, for headless sessions, which have no EDID.
  4. Caliper's 96 dpi assumption.

  The viewer checks the monitor under the window every 500 ms.
- **Calibrate** works like Caliper's: an ID-1 card outline (85.6 x 53.98 mm), a 100 mm
  bar for a ruler, and a px per mm slider. Each change is saved to
  `$XDG_STATE_HOME/korri-slint-caliper/px-per-mm`, one `<monitor> <px/mm>` line for each
  monitor. That location and format are the spike's own and are **unresolved**. Caliper
  keeps one value in localStorage (`caliper:px-per-mm`).
- **Reload** uses `notify` on the packages folder. A changed `.slint` file evicts every
  compiled part that imports it, directly or not. The shown part is recompiled and drawn
  again. Each reload prints one `reload:` line with its times.

## Knobs and takes (the `tune` spike)

Question: can the viewer change design inputs while you watch, and show an alternate
version beside the original, without changing the real files?

Answer: yes. A knob change reaches every live frame through the interpreter in about
1 ms, and one 640 x 480 frame is on the screen 12 ms (median) after the key. A take
compiles from its copies and draws beside the original on each device. Code: `src/tune.rs`,
the `knob_*`, `new_take` and `end_take` parts of `src/main.rs`, `KnobView` in
`ui/viewer.slint`.

Use: **Knobs** (bar, right) opens the panel of the token files that the shown part
imports. Drag a number's name sideways (Shift: ten steps) or type a value and press Enter.
Type a colour; it shows as soon as it is a valid `#rrggbb`. Pick a sibling token from the
list. The frames change at once; the file changes only when you let go: the end of a drag,
Enter, or a choice. This is Caliper's rule. Choose one state, then **New take**. The
viewer copies the part's preview file and its `Name.slint` into
`<packages>/.caliper/takes/<n>/`, at the same relative paths, and prints their paths. Edit
them in your editor. The take's frame reloads beside the original. **Accept** or
**Discard** sit on the take's caption.

How it works:

- **Knobs.** Each `export global` property whose value is a literal is a knob: a colour,
  a number with its unit, or, for a colour, the name of a sibling colour literal (Pico's
  `bg: black`; Caliper's `var(--p8-black)` picker). Everything else is listed under
  **Not knobs** with the reason: computed, animated by the clock, an input that
  `main.slint` or the preview frame writes, or a type with no control.
  `/** … */` hints above a declaration (`@label`, `@min`, `@max`, `@step`,
  `@knob ignore`) are read as Caliper reads them; no package uses them yet.
- **Run-time set.** Slint makes an `out` property read-only from outside, and the compiler
  may fold a constant one into its readers. So the viewer compiles through
  `Compiler::set_file_loader`: the token file is served with each knob declared
  `in-out`, and the compiled preview gets one appended line per global,
  `export { Tokens as Tune<n>Tokens } from "<abs path>";`. Then
  `ComponentInstance::set_global_property` reaches it. The real file does not change.
  Each frame is its own instance with its own globals, so the viewer sets each one.
- **Save** writes only the value's byte span. It checks first that the file is the
  version the knob read and that the old value is still at that span (Caliper's
  `editSource`). Reload on save then recompiles from the file.
- **Takes** use Caliper's layout (`src/takes/store.js`): `.caliper/.gitignore` (`*`),
  `.caliper/takes/<n>/<path>`, `.caliper/takes/<n>.json` with `part` (the preview
  file, root-relative), `state`, `device`, `created`, and `.caliper/accepted.json` on
  Accept. The same file loader serves a take's copy of a file when it has one, so a take
  compiles from its copies and the real tree. Accept copies the take's files over the real
  files and removes the take. Discard removes it.

Measurements (`evidence/knob-latency.txt`, Xvfb 2880 x 1800 at scale 1.5, zao):

| Frames on the stage | Key to new pixels on the screen (median, range) | Set + render in the viewer |
|---|---|---|
| 1 (RG353M, 640 x 480) | 12.0 ms (7.1 to 92.3), 10 runs | 0.9 ms + 5 to 8 ms |
| 12 (6 states x RG353M and Odin 2 Portal 1920 x 1080) | 166 ms (86 to 258), 10 runs | 0.1 ms + 77 to 139 ms |

The software render of each frame, on the UI thread, is the cost. Saving the file costs a
reload: 0.9 to 1.2 s for ZoneGames.

Limits and differences from Caliper:

- The panel lists every knob of each token file the part imports. Caliper lists only the
  properties the part reads (it tries a test value and watches the frames).
- A colour knob is a text field and a swatch. Slint's std-widgets have no colour picker.
- A knob always writes the real token file. In Caliper, a knob on a take's frame writes
  the take's copy. Here a live knob value shows on the original and the take alike.
- "Make a literal a token" is not built. Zone's components hold many art-px literals
  (`104 * Tokens.a`) that it would list.
- A take needs one state chosen. Agent-made takes are out of scope.

## Unresolved data

- The device table is Caliper's `Device` form, kept in code (`src/devices.rs`). Korri has
  no device list file. Sources: `evidence/device-sources.md`.
- The RP Mini V2's millimetres (75 x 65) come from Korri's old Caliper config, not from
  Retroid. The viewer marks them "millimetres unresolved".
- The location and format of the calibration file are unresolved.
- The take root. Caliper puts `.caliper/` in the Vite root. The viewer puts it in the
  packages folder (`clients/shell/packages/.caliper/` in a checkout). The repository root
  would also fit. Its `.gitignore` keeps it out of Git.
- The alias the viewer gives a re-exported global (`Tune<n><Global>`) is the viewer's own.

## Evidence

`evidence/`. The screenshots come from headless sway (2880x1800 at scale 1.5, like
myoko's panels) and from Xvfb (xdotool clicks). `measure.py` reads frame widths from a
screenshot. `measure.txt` has the results. `reload-*.log` has the reload times.

Knobs and takes (`tune-*`, the knobs were driven in Xvfb at 2880x1800 and
`SLINT_SCALE_FACTOR=1.5`; under headless sway the virtual pointer of `wlrctl` reached no
window, so sway shows the take only). Each proof ran on a copy of the packages in `/tmp`,
which was then restored to equal the checkout:

- `tune-knob-live-ground-games-rg353m-odin2portal-50pct.png`: Zone `Tokens.ground-games`
  typed as `#e70000`, live on all 12 frames, file unchanged. `knob-save.diff.txt`: what
  Enter (Save) wrote.
- `tune-knob-scrub-card-aspect-50pct.png`: `card-aspect` dragged from 0.78 to 0.98.
- `tune-knobs-pico-panel-50pct.png`, `tune-knob-pico-bg-token-picker-50pct.png`: Pico's
  `P8` knobs, and the role `bg` set to the sibling token `blue` (the write is `bg: blue;`).
- `tune-take-beside-original-sway-50pct.png` (sway), `tune-take-beside-original-x11-50pct.png`
  (Xvfb): take 1 of ZoneCard / Focused, edited by hand, beside the original on two devices.
  `take-accept.diff.txt`: the take's files and record, and what Accept wrote.
- `tune-narrow-knobs-drawer-50pct.png`, `tune-narrow-parts-drawer-50pct.png`: a 600 x 800
  window; both panels are drawers, each button stays in the bar.
- `knob-latency.py`, `knob-latency.txt`: the timing.
