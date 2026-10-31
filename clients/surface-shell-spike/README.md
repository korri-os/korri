# THROWAWAY: runtime surface-shell spike

A fixed Rust executable compiles a package's `main.slint` at runtime and runs
its bundled `logic.js` in a fresh QuickJS heap. This is not a product client,
plugin runtime or replacement for `contracts/surface/`. `CONTRACT.md` binds it.

## Run

```sh
SHELL_ROOT="$PWD/clients/surface-shell-spike" \
SHELL_SURFACES=_test/a,_test/b SHELL_STATS=1 \
  clients/surface-shell-spike/target/release/surface-shell-spike
```

The default is `zone,pico`. F2 or gamepad L1+R1 switches packages and resets
state. Arrows, Enter, Escape/Backspace, M and O send the contract's messages.
Directions repeat after 400 ms, then every 100 ms. Closing the window quits;
Escape is **back**, not quit. Gamepad readers scan readable BTN_SOUTH event
nodes, never grab them, and dedupe actions from different sources within 40 ms.

`SHELL_ROOT` defaults to the executable's directory, which must contain
`surfaces/` and `fixtures/`. `SHELL_SIZE=WxH` defaults to 640x480.
`SHELL_FULLSCREEN=1` requests fullscreen. `SHELL_STATS=1` reports actual
redraw frames/s every five seconds and per-load timings in milliseconds.

The shell applies `view()` before showing the new window. It keeps the old
window alive until the new window completes its first redraw, then hides and
drops it. Compilation is synchronous on the UI thread: the old frame remains
visible, but input/animations pause during compilation. A compile/load error
keeps the current package. Diagnostics go to stderr. There is no production
sandbox/permission claim here; load only the spike's trusted local packages.

State stays in a lexical JavaScript closure, including non-JSON values such as
Map. Only view output is serialized back to Rust. Images are decoded once and
cached by data URL or resolved package-relative file path. Null and omitted
properties retain their values. Struct field names follow the same camelCase
to kebab-case conversion as root properties. Arrays become Slint models.

`ComponentDefinition::properties()` validates public types. Slint 1.18.1's
public `ValueType::Number` unifies int and float, and its setter does **not**
truncate int values. The pinned interpreter's `internal` detailed metadata
therefore supplies recursive int/float coercion, including structs/models.
This deliberately uses an unstable API in a throwaway spike; it must be
revisited for a product shell. Out-of-range ints/floats fail explicitly.

## Checks and workstation proof

```sh
nix shell nixpkgs#cargo nixpkgs#rustc nixpkgs#gcc nixpkgs#clippy nixpkgs#rustfmt \
  -c sh -c 'cd clients/surface-shell-spike && cargo fmt --check && cargo test --release && cargo clippy --release --all-targets -- -D warnings && cargo build --release'

export LD_LIBRARY_PATH="$(nix eval --impure --raw --expr 'let p = import <nixpkgs> {}; in p.lib.makeLibraryPath (with p; [ wayland libxkbcommon libx11 libxcursor libxrandr libxi libxcb fontconfig.lib ])')"

nix shell nixpkgs#xorg.xorgserver nixpkgs#xdotool nixpkgs#imagemagick \
  nixpkgs#libxkbcommon nixpkgs#xkeyboard-config \
  -c clients/surface-shell-spike/proof.sh \
  clients/surface-shell-spike/target/release/surface-shell-spike \
  clients/surface-shell-spike/evidence/xvfb
```

For an unpatched cargo binary, add the winit runtime libraries to
`LD_LIBRARY_PATH`; the Nix package adds their RUNPATH automatically. The proof
uses two coloured test packages in `surfaces/_test/` and an actual image data
URL from `fixtures/model.json`. It checks arrows, F2, reset-on-switch, retaining
the old surface after a failed compile, and the 1280x720 size. Pixel comparisons
assert that movement changes the frame and Left restores the initial frame.

See `device/README.md` for aarch64 build, self-contained bundle, the reusable
`drive.sh` package-list command and kiosk recovery. Device runs use root only
for the existing gamepad read permissions; no permissions are changed.

The real Pico package has also passed `proof-pico.sh BINARY OUT_DIRECTORY`
under the same Xvfb/tool environment (add `nixpkgs#python3`). This covers absent
art fields and `stage.cart: null`, as well as arrow selection and an F2 round
trip. Measured times and build/device status: `evidence/STATUS.md`.

`proof-surfaces.sh` checks both real packages, arrows, F2 and 20 switches.
`proof-bundle.sh` executes the exact aarch64 archive's unchanged launcher under
QEMU/PRoot on the workstation. See `evidence/STATUS.md` for hashes and results.
The 20-switch RSS trace grows by 104 KiB/switch on x86_64; it is not flat.
Slint's imported-font registration is a remaining spike limit. Zone's declared
initial band snap is retained.
