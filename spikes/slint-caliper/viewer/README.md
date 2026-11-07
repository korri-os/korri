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

## Unresolved data

- The device table is Caliper's `Device` form, kept in code (`src/devices.rs`). Korri has
  no device list file. Sources: `evidence/device-sources.md`.
- The RP Mini V2's millimetres (75 x 65) come from Korri's old Caliper config, not from
  Retroid. The viewer marks them "millimetres unresolved".
- The location and format of the calibration file are unresolved.

## Evidence

`evidence/`. The screenshots come from headless sway (2880x1800 at scale 1.5, like
myoko's panels) and from Xvfb (xdotool clicks). `measure.py` reads frame widths from a
screenshot. `measure.txt` has the results. `reload-*.log` has the reload times.
